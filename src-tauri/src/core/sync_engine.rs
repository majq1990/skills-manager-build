use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Refuse to copy when `dst` would land inside `src` (or equal `src`).
/// Otherwise the recursive copy walks into the freshly-created `dst` and
/// produces unbounded `<dst>/<dst>/<dst>/...` nesting (issue #61).
///
/// A `dst` that is itself a link is judged by its own location (parent + name),
/// never by what it resolves to. Resolving it would report our own deployed
/// link as "inside the source" and refuse — which is what broke switching an
/// already-deployed skill from symlink to copy mode. Removing that link cannot
/// touch the source (links are unlinked, not followed), so its pointee is
/// irrelevant to both hazards this guard exists for.
pub(crate) fn ensure_dst_not_inside_src(src: &Path, dst: &Path) -> Result<()> {
    // Same path, before any link-aware special casing below: if `src` is itself
    // a link, judging `dst` lexically and `src` canonically would compare a link
    // against its pointee and miss the equality. Unlinking `dst` would then
    // destroy `src`, and the copy would "succeed" from the empty directory it
    // just created in its place.
    if src == dst {
        anyhow::bail!(
            "Source and destination are the same path {:?}; refusing to copy",
            src
        );
    }
    let Ok(src_canon) = src.canonicalize() else {
        return Ok(());
    };
    let dst_is_link = std::fs::symlink_metadata(dst)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);
    let lexical_dst = || -> Option<PathBuf> {
        let parent = dst.parent()?.canonicalize().ok()?;
        let name = dst.file_name()?;
        Some(parent.join(name))
    };
    let dst_canon: Option<PathBuf> = if dst_is_link {
        lexical_dst()
    } else {
        dst.canonicalize().ok().or_else(lexical_dst)
    };
    // The same link reached by two spellings (`./x` vs `x`) is still the same
    // object, and the lexical/canonical mismatch above would hide it.
    if dst_is_link {
        let lexical_src = || -> Option<PathBuf> {
            let parent = src.parent()?.canonicalize().ok()?;
            Some(parent.join(src.file_name()?))
        };
        if let (Some(a), Some(b)) = (lexical_src(), dst_canon.as_ref()) {
            if a == *b {
                anyhow::bail!(
                    "Source and destination are the same path {:?}; refusing to copy",
                    src
                );
            }
        }
    }
    if let Some(dst_canon) = dst_canon {
        if dst_canon.starts_with(&src_canon) {
            anyhow::bail!(
                "Destination {:?} is inside source {:?}; refusing to copy to avoid infinite recursion",
                dst,
                src
            );
        }
    }
    Ok(())
}

/// What is actually sitting at a deployment target right now.
///
/// Classified from `symlink_metadata` only — never `is_dir()`, which follows
/// links and would report a Windows junction or a directory symlink as a real
/// directory, sending it down the `remove_dir_all` path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetState {
    /// Nothing there. Deploying cannot destroy anything.
    Absent,
    /// A link that resolves to the source we are about to deploy — ours by
    /// construction, and unlinking it cannot touch the source.
    LinkToSource,
    /// A link pointing somewhere else, or a dangling one. The link itself is
    /// user configuration even when its pointee is gone.
    ForeignLink,
    /// A real directory. Indistinguishable from a copy-mode deployment by
    /// filesystem inspection alone.
    RealDir,
    /// A real file. We never deploy a bare file, so this is always the user's.
    RealFile,
}

/// What the caller is authorized to destroy at the target.
///
/// This is an enum rather than a bool so that adding it to `sync_skill` breaks
/// every call site and forces each one to state its intent — the property worth
/// having in a fix for silent data loss (upstream #363).
#[derive(Debug, Clone, Copy)]
pub enum ReplacePolicy<'a> {
    /// Only proceed when nothing of the user's can be lost. Anything we cannot
    /// prove is ours is refused.
    NoClobber,
    /// The caller found a `skill_targets` row claiming exactly this path.
    ///
    /// The row is historical intent, not proof of what is on disk now, so it
    /// only authorizes removing an object whose *current* type matches the
    /// recorded mode: a row saying `symlink` must never authorize deleting a
    /// real directory that replaced our link.
    Recorded { mode: &'a str },
    /// Explicit user intent: adopt an existing directory into the library,
    /// or "update this copy from center".
    UserConfirmed,
}

impl ReplacePolicy<'_> {
    /// Whether this policy authorizes destroying `state`.
    fn permits(self, state: TargetState) -> bool {
        match state {
            // Nothing to lose either way.
            TargetState::Absent | TargetState::LinkToSource => true,
            TargetState::ForeignLink => match self {
                ReplacePolicy::NoClobber => false,
                // Our own link can dangle after the library moves, so a
                // symlink row still authorizes replacing a link. A copy row
                // does not: something turned our directory into a link.
                ReplacePolicy::Recorded { mode } => mode.trim() == "symlink",
                ReplacePolicy::UserConfirmed => true,
            },
            TargetState::RealDir => match self {
                ReplacePolicy::NoClobber => false,
                ReplacePolicy::Recorded { mode } => mode.trim() == "copy",
                ReplacePolicy::UserConfirmed => true,
            },
            // We never deploy a bare file, so no record can vouch for one.
            TargetState::RealFile => matches!(self, ReplacePolicy::UserConfirmed),
        }
    }

    fn describe(self) -> &'static str {
        match self {
            ReplacePolicy::NoClobber => "is not managed by Skills Manager",
            ReplacePolicy::Recorded { .. } => "does not match its recorded deployment",
            ReplacePolicy::UserConfirmed => "cannot be replaced",
        }
    }
}

/// A write was refused because the target is not ours to destroy.
///
/// A distinct type so callers can tell an ownership refusal — where the right
/// response is to report the conflict to the user — from an ordinary IO error,
/// which reconcile paths have always tolerated and logged.
#[derive(Debug)]
pub struct ReplaceRefused {
    pub target: PathBuf,
    pub reason: &'static str,
}

impl std::fmt::Display for ReplaceRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Refusing to replace {:?}: it {}. The existing content was left untouched — \
             import it into the library, or move it aside, and try again.",
            self.target, self.reason
        )
    }
}

impl std::error::Error for ReplaceRefused {}

/// Classify what is at `target` without following links.
///
/// `source` is the deployment source, used only to recognize a link that already
/// points at it. Pass `None` when there is no source to compare against.
pub fn classify_target(target: &Path, source: Option<&Path>) -> Result<TargetState> {
    let metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(TargetState::Absent),
        // A target we cannot even stat is not evidence that it is ours.
        Err(err) => {
            return Err(anyhow::Error::from(err)
                .context(format!("Cannot inspect existing target {:?}", target)));
        }
    };

    if metadata.file_type().is_symlink() {
        // `LinkToSource` is universally permitted, so it must only be reached
        // with a real source to compare against. Callers with no source (undeploy
        // and friends) pass `None`: an empty path would canonicalize to the
        // process CWD and turn a user's link into "ours".
        if source.is_some_and(|source| symlink_points_to(target, source)) {
            return Ok(TargetState::LinkToSource);
        }
        return Ok(TargetState::ForeignLink);
    }
    if metadata.is_dir() {
        Ok(TargetState::RealDir)
    } else {
        Ok(TargetState::RealFile)
    }
}

/// The single place the refusal decision and its wording live, so `sync_skill`
/// and the removal helpers can never disagree.
fn authorize_replacement(
    source: &Path,
    target: &Path,
    policy: ReplacePolicy<'_>,
) -> Result<TargetState> {
    let state = classify_target(target, Some(source))?;
    if !policy.permits(state) {
        return Err(ReplaceRefused {
            target: target.to_path_buf(),
            reason: policy.describe(),
        }
        .into());
    }
    Ok(state)
}

#[derive(Debug, Clone, Copy)]
pub enum SyncMode {
    Symlink,
    Copy,
}

impl SyncMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncMode::Symlink => "symlink",
            SyncMode::Copy => "copy",
        }
    }
}

pub fn sync_mode_for_tool(_tool_key: &str, configured_mode: Option<&str>) -> SyncMode {
    match configured_mode {
        Some("copy") => SyncMode::Copy,
        Some("symlink") => SyncMode::Symlink,
        _ => SyncMode::Symlink,
    }
}

pub fn target_dir_name(central_path: &Path, skill_name: &str) -> String {
    central_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| skill_name.to_string())
}

pub fn sync_skill(
    source: &Path,
    target: &Path,
    mode: SyncMode,
    policy: ReplacePolicy<'_>,
) -> Result<SyncMode> {
    // Internal self-check uses no hash context, so Copy mode always
    // proceeds — the caller (e.g. `sync_desired_targets`) is the place
    // that knows about freshness and can short-circuit.
    if is_target_current(source, target, mode, None, None) {
        return Ok(mode);
    }

    // Decide what we are allowed to destroy before touching anything. This
    // runs here, immediately before the removal, rather than only in a caller
    // preflight: a preflight result cannot authorize a later deletion (#363).
    let state = authorize_replacement(source, target, policy)?;

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent dir {:?}", parent))?;
    }

    ensure_dst_not_inside_src(source, target)?;

    // Remove exactly what we classified. Passing `state` keeps a directory
    // that appeared since the check from being swallowed by `remove_dir_all`:
    // the removal is type-specific, so it fails instead. Errors propagate —
    // the old `.ok()` could leave the target in place and then create over it.
    remove_classified_target(target, state)
        .with_context(|| format!("Failed to remove existing target {:?}", target))?;

    match mode {
        SyncMode::Symlink => {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(source, target).with_context(|| {
                    format!("Failed to create symlink {:?} -> {:?}", target, source)
                })?;
                Ok(SyncMode::Symlink)
            }
            #[cfg(windows)]
            {
                match std::os::windows::fs::symlink_dir(source, target) {
                    Ok(()) => Ok(SyncMode::Symlink),
                    Err(err) => {
                        // Typical causes: missing SeCreateSymbolicLinkPrivilege,
                        // Developer Mode disabled, or non-NTFS target volume.
                        log::warn!(
                            "symlink_dir {:?} -> {:?} failed, falling back to copy: {err}",
                            target,
                            source
                        );
                        copy_dir_recursive(source, target)?;
                        Ok(SyncMode::Copy)
                    }
                }
            }
            #[cfg(all(not(unix), not(windows)))]
            {
                copy_dir_recursive(source, target)?;
                Ok(SyncMode::Copy)
            }
        }
        SyncMode::Copy => {
            copy_dir_recursive(source, target)?;
            Ok(SyncMode::Copy)
        }
    }
}

/// Deploy a single agent definition file (opencode `*.md`, codex `*.toml`)
/// to `<tool agents_dir>/<agent-id>.<ext>`.
///
/// Mirrors [`sync_skill`] at file granularity: symlink mode links the file
/// into the tool's agents directory (falling back to a plain copy where
/// symlinks are unavailable, e.g. Windows without Developer Mode), copy
/// mode writes the file directly. Freshness for copy mode reuses the
/// stored-vs-current source hash comparison (issue #153 semantics).
pub fn sync_agent_file(
    source: &Path,
    target: &Path,
    mode: SyncMode,
    last_synced_source_hash: Option<&str>,
    current_source_hash: Option<&str>,
) -> Result<SyncMode> {
    if is_target_current(source, target, mode, last_synced_source_hash, current_source_hash) {
        return Ok(mode);
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent dir {:?}", parent))?;
    }

    ensure_dst_not_inside_src(source, target)?;
    remove_target(target).ok();

    match mode {
        SyncMode::Symlink => {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(source, target).with_context(|| {
                    format!("Failed to create symlink {:?} -> {:?}", target, source)
                })?;
                Ok(SyncMode::Symlink)
            }
            #[cfg(windows)]
            {
                match std::os::windows::fs::symlink_file(source, target) {
                    Ok(()) => Ok(SyncMode::Symlink),
                    Err(err) => {
                        log::warn!(
                            "symlink_file {:?} -> {:?} failed, falling back to copy: {err}",
                            target,
                            source
                        );
                        std::fs::copy(source, target)
                            .with_context(|| format!("copying {:?} -> {:?}", source, target))?;
                        Ok(SyncMode::Copy)
                    }
                }
            }
            #[cfg(all(not(unix), not(windows)))]
            {
                std::fs::copy(source, target)?;
                Ok(SyncMode::Copy)
            }
        }
        SyncMode::Copy => {
            std::fs::copy(source, target)
                .with_context(|| format!("copying {:?} -> {:?}", source, target))?;
            Ok(SyncMode::Copy)
        }
    }
}

/// Remove a deployed agent artifact: a single file (File kind) or the
/// skill-wrapped directory. Only ever called with paths recorded in
/// `agent_targets` — never scans or deletes anything else in the tool's
/// directories (built-in agents are untouchable by construction).
pub fn remove_agent_target(target: &Path) -> Result<()> {
    remove_target(target)
}

/// Decide whether the existing target is already in the desired state.
///
/// - **Symlink mode**: the target must be a symlink pointing at `source`.
/// - **Copy mode**: the target must still exist on disk **and** the
///   previously synced source hash must equal the current source hash
///   (both must be `Some`). The existence check protects against a
///   user manually deleting the synced directory between sessions —
///   without it a stale hash would cause us to skip a re-copy the
///   user needs. Callers without hash context should pass `None`,
///   which preserves the historical "always recopy" behavior. See
///   `SkillTargetRecord.source_hash` and issue #153 for context.
pub fn is_target_current(
    source: &Path,
    target: &Path,
    mode: SyncMode,
    last_synced_source_hash: Option<&str>,
    current_source_hash: Option<&str>,
) -> bool {
    match mode {
        SyncMode::Symlink => symlink_points_to(target, source),
        SyncMode::Copy => match (last_synced_source_hash, current_source_hash) {
            (Some(stored), Some(current)) if stored == current => {
                std::fs::symlink_metadata(target).is_ok()
            }
            _ => false,
        },
    }
}

fn symlink_points_to(target: &Path, source: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(target) else {
        return false;
    };
    if !metadata.file_type().is_symlink() {
        return false;
    }

    let Ok(link_target) = std::fs::read_link(target) else {
        return false;
    };
    let resolved_link_target = if link_target.is_absolute() {
        link_target
    } else {
        target
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(link_target)
    };

    if resolved_link_target == source {
        return true;
    }

    match (resolved_link_target.canonicalize(), source.canonicalize()) {
        (Ok(link), Ok(src)) => link == src,
        _ => false,
    }
}

pub fn remove_target(target: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    if metadata.file_type().is_symlink() {
        #[cfg(windows)]
        {
            if target.is_dir() {
                std::fs::remove_dir(target)?;
            } else {
                std::fs::remove_file(target)?;
            }
        }
        #[cfg(not(windows))]
        {
            std::fs::remove_file(target)?;
        }
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(target)?;
    } else {
        std::fs::remove_file(target)?;
    }
    Ok(())
}

/// Remove a deployment target only while what is on disk still matches the
/// deployment we recorded. Returns `false` when the object was preserved
/// because it no longer does.
///
/// Undeploy and cleanup paths reach a path through a `skill_targets` row, and
/// a row is a statement about the past: if the user replaced our link with a
/// real directory of their own, the row still points at it. Removing on the
/// row's word alone deletes that directory (#363, expectation 5).
///
/// Link-vs-source identity is irrelevant here — a `symlink` row authorizes
/// removing a link whether or not it still resolves to the skill we deployed
/// (the library may have moved) — so no source path is required.
pub fn remove_recorded_target(target: &Path, recorded_mode: &str) -> Result<bool> {
    let state = classify_target(target, None)?;
    let policy = ReplacePolicy::Recorded {
        mode: recorded_mode,
    };
    if !policy.permits(state) {
        return Ok(false);
    }
    remove_classified_target(target, state)?;
    Ok(true)
}

/// Row-driven removal for cleanup paths: remove only while the disk still
/// matches the record, and report a preserved replacement instead of falling
/// back to [`remove_target`]. Every caller that deletes what a `skill_targets`
/// row points at should go through here (or [`remove_recorded_target`]).
/// Returns whether the path was removed; the record should be dropped either
/// way.
pub fn remove_recorded_target_or_warn(target: &Path, recorded_mode: &str) -> bool {
    match remove_recorded_target(target, recorded_mode) {
        Ok(true) => true,
        Ok(false) => {
            log::warn!(
                "Preserving {}: no longer matches its recorded {recorded_mode} deployment; \
                 removing the record only",
                target.display()
            );
            false
        }
        Err(e) => {
            log::warn!("Failed to remove sync target {}: {e}", target.display());
            false
        }
    }
}

/// Whether what is at `target` is still consistent with a deployment recorded
/// as `recorded_mode`. `false` means something else took the path over, which
/// is why an undeploy may leave a path behind on purpose.
pub fn matches_recorded_deployment(target: &Path, recorded_mode: &str) -> Result<bool> {
    let state = classify_target(target, None)?;
    if state == TargetState::Absent {
        return Ok(false);
    }
    let policy = ReplacePolicy::Recorded {
        mode: recorded_mode,
    };
    Ok(policy.permits(state))
}

/// Remove a target that was already classified, using an operation that only
/// works on that kind of object.
///
/// This is the difference between "we checked, then deleted whatever we found"
/// and "we deleted the thing we checked". If a real directory replaces a link
/// between the classification and this call, `remove_file` fails with an error
/// instead of `remove_dir_all` quietly eating it — the exact race that makes a
/// check-then-`remove_target` sequence still unsafe.
pub fn remove_classified_target(target: &Path, state: TargetState) -> Result<()> {
    match state {
        TargetState::Absent => Ok(()),
        TargetState::LinkToSource | TargetState::ForeignLink => remove_link(target),
        TargetState::RealFile => std::fs::remove_file(target).map_err(Into::into),
        TargetState::RealDir => {
            // Only reachable via `Recorded { mode: "copy" }` or UserConfirmed.
            std::fs::remove_dir_all(target).map_err(Into::into)
        }
    }
}

/// Unlink a symlink (or, on Windows, a directory symlink) without following it.
fn remove_link(target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        let metadata = match std::fs::symlink_metadata(target) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        if !metadata.file_type().is_symlink() {
            anyhow::bail!(
                "Refusing to remove {:?}: expected a link but found a real file or directory",
                target
            );
        }
        // `remove_dir` on a directory link removes the link, not its contents.
        if metadata.file_type().is_symlink_dir() {
            std::fs::remove_dir(target)?;
        } else {
            std::fs::remove_file(target)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(target).map_err(Into::into)
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ft.is_dir() {
            let name = entry.file_name();
            if name == ".git" {
                continue;
            }
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Creating symlinks on Windows needs SeCreateSymbolicLinkPrivilege
    /// (admin or Developer Mode). Probe it so privilege-dependent tests skip
    /// cleanly on locked-down machines instead of failing; they still run for
    /// real wherever symlinks work.
    #[cfg(windows)]
    fn windows_symlink_privilege_available() -> bool {
        let Ok(tmp) = tempdir() else {
            return false;
        };
        let target = tmp.path().join("probe-target");
        let link = tmp.path().join("probe-link");
        if fs::create_dir_all(&target).is_err() {
            return false;
        }
        std::os::windows::fs::symlink_dir(&target, &link).is_ok()
    }

    // ── sync_mode_for_tool ──

    #[test]
    fn sync_mode_defaults_to_symlink() {
        assert!(matches!(
            sync_mode_for_tool("claude-code", None),
            SyncMode::Symlink
        ));
    }

    #[test]
    fn sync_mode_cursor_defaults_to_symlink() {
        assert!(matches!(
            sync_mode_for_tool("cursor", None),
            SyncMode::Symlink
        ));
    }

    #[test]
    fn sync_mode_explicit_copy_overrides_default() {
        assert!(matches!(
            sync_mode_for_tool("claude-code", Some("copy")),
            SyncMode::Copy
        ));
    }

    #[test]
    fn sync_mode_explicit_symlink_overrides_cursor_default() {
        assert!(matches!(
            sync_mode_for_tool("cursor", Some("symlink")),
            SyncMode::Symlink
        ));
    }

    #[test]
    fn sync_mode_unknown_config_falls_back_to_tool_default() {
        assert!(matches!(
            sync_mode_for_tool("cursor", Some("invalid")),
            SyncMode::Symlink
        ));
        assert!(matches!(
            sync_mode_for_tool("claude-code", Some("invalid")),
            SyncMode::Symlink
        ));
    }

    #[test]
    fn sync_mode_as_str() {
        assert_eq!(SyncMode::Symlink.as_str(), "symlink");
        assert_eq!(SyncMode::Copy.as_str(), "copy");
    }

    #[test]
    fn target_dir_name_uses_central_directory_name() {
        let central_path = Path::new("/central/skill123-2");

        assert_eq!(target_dir_name(central_path, "skill123"), "skill123-2");
    }

    #[test]
    fn target_dir_name_falls_back_to_skill_name() {
        assert_eq!(target_dir_name(Path::new(""), "skill123"), "skill123");
    }

    // ── sync_skill (filesystem) ──

    #[test]
    fn sync_skill_copy_creates_directory_with_files() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# hello").unwrap();

        let mode = sync_skill(&src, &tgt, SyncMode::Copy, ReplacePolicy::NoClobber).unwrap();
        assert!(matches!(mode, SyncMode::Copy));
        assert!(tgt.join("SKILL.md").exists());
        assert_eq!(fs::read_to_string(tgt.join("SKILL.md")).unwrap(), "# hello");
    }

    #[cfg(unix)]
    #[test]
    fn sync_skill_symlink_creates_symlink() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# hello").unwrap();

        let mode = sync_skill(&src, &tgt, SyncMode::Symlink, ReplacePolicy::NoClobber).unwrap();
        assert!(matches!(mode, SyncMode::Symlink));
        assert!(tgt.is_symlink());
    }

    #[cfg(windows)]
    #[test]
    fn sync_skill_symlink_creates_symlink_on_windows() {
        if !windows_symlink_privilege_available() {
            eprintln!("skipped: no Windows symlink privilege (admin or Developer Mode required)");
            return;
        }
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# hello").unwrap();

        let mode = sync_skill(&src, &tgt, SyncMode::Symlink, ReplacePolicy::NoClobber).unwrap();
        assert!(matches!(mode, SyncMode::Symlink));
        assert!(tgt.is_symlink());
    }

    #[test]
    fn sync_skill_replaces_existing_target() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("new.md"), "new").unwrap();

        // Pre-existing target directory
        fs::create_dir_all(&tgt).unwrap();
        fs::write(tgt.join("old.md"), "old").unwrap();

        sync_skill(&src, &tgt, SyncMode::Copy, ReplacePolicy::UserConfirmed).unwrap();
        assert!(tgt.join("new.md").exists());
        assert!(!tgt.join("old.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn sync_skill_symlink_skips_existing_correct_link() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# hello").unwrap();
        std::os::unix::fs::symlink(&src, &tgt).unwrap();

        let before = fs::symlink_metadata(&tgt).unwrap().modified().unwrap();
        let mode = sync_skill(&src, &tgt, SyncMode::Symlink, ReplacePolicy::NoClobber).unwrap();

        assert!(matches!(mode, SyncMode::Symlink));
        assert_eq!(fs::read_link(&tgt).unwrap(), src);
        assert_eq!(
            fs::symlink_metadata(&tgt).unwrap().modified().unwrap(),
            before
        );
    }

    // ── copy_dir_recursive ──

    #[test]
    fn copy_dir_recursive_skips_dot_git() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join(".git/config"), "git config").unwrap();
        fs::create_dir_all(src.join("subdir")).unwrap();
        fs::write(src.join("subdir/file.md"), "content").unwrap();
        fs::write(src.join("root.md"), "root").unwrap();

        let dst = tmp.path().join("dst");
        copy_dir_recursive(&src, &dst).unwrap();

        assert!(!dst.join(".git").exists());
        assert!(dst.join("subdir/file.md").exists());
        assert!(dst.join("root.md").exists());
    }

    // ── ensure_dst_not_inside_src ──

    #[test]
    fn ensure_dst_not_inside_src_rejects_subdirectory() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("skills");
        fs::create_dir_all(&src).unwrap();
        let dst = src.join("skills");

        let err = ensure_dst_not_inside_src(&src, &dst).unwrap_err();
        assert!(err.to_string().contains("infinite recursion"), "{err}");
    }

    #[test]
    fn ensure_dst_not_inside_src_rejects_same_path() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("skills");
        fs::create_dir_all(&src).unwrap();

        let err = ensure_dst_not_inside_src(&src, &src).unwrap_err();
        assert!(err.to_string().contains("same path"), "{err}");
    }

    #[test]
    fn ensure_dst_not_inside_src_allows_disjoint_paths() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("skills");
        let dst = tmp.path().join("other").join("skills");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(dst.parent().unwrap()).unwrap();

        ensure_dst_not_inside_src(&src, &dst).unwrap();
    }

    #[test]
    fn ensure_dst_not_inside_src_allows_sibling_dst() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("skills");
        let dst = tmp.path().join("skills-disabled");
        fs::create_dir_all(&src).unwrap();

        ensure_dst_not_inside_src(&src, &dst).unwrap();
    }

    #[test]
    fn sync_skill_refuses_target_inside_source() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("skills");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# hello").unwrap();
        let tgt = src.join("skills");

        let err = sync_skill(&src, &tgt, SyncMode::Copy, ReplacePolicy::NoClobber).unwrap_err();
        assert!(err.to_string().contains("infinite recursion"), "{err}");
        // Source must be untouched after the rejection.
        assert!(src.join("SKILL.md").exists());
    }

    // ── remove_target ──

    #[test]
    fn remove_target_removes_directory() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("to_remove");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("file.txt"), "data").unwrap();

        remove_target(&dir).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn remove_target_removes_file() {
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("file.txt");
        fs::write(&file, "data").unwrap();

        remove_target(&file).unwrap();
        assert!(!file.exists());
    }

    #[cfg(unix)]
    #[test]
    fn remove_target_removes_symlink() {
        let tmp = tempdir().unwrap();
        let real = tmp.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        remove_target(&link).unwrap();
        assert!(!link.exists());
        assert!(real.exists()); // original untouched
    }

    #[cfg(windows)]
    /// A `skill_targets` row saying we deployed a symlink is not proof that
    /// the directory now sitting there is ours. Removing it blindly destroyed
    /// the user's own copy of the skill, silently.
    #[test]
    fn recorded_link_over_a_real_directory_is_preserved() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("target");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("mine.txt"), "user content").unwrap();

        assert!(!remove_recorded_target_or_warn(&path, "symlink"));
        assert_eq!(
            fs::read_to_string(path.join("mine.txt")).unwrap(),
            "user content"
        );
    }

    /// A copy deployment wrote a real directory, so the row does vouch for it.
    #[test]
    fn recorded_copy_over_a_real_directory_is_removed() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("target");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), "# ours").unwrap();

        assert!(remove_recorded_target_or_warn(&path, "copy"));
        assert!(!path.exists());
    }

    /// Nothing on disk vouches for a bare file, whatever the row claims.
    #[test]
    fn recorded_modes_never_vouch_for_a_bare_file() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("target.md");
        fs::write(&path, "not ours").unwrap();

        assert!(!remove_recorded_target_or_warn(&path, "symlink"));
        assert!(!remove_recorded_target_or_warn(&path, "copy"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not ours");
    }

    /// A row written before the mode column existed, or by a future version,
    /// vouches for nothing — the safe default is to keep what is there.
    #[test]
    fn unknown_recorded_mode_preserves_the_target() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("target");
        fs::create_dir_all(&path).unwrap();

        assert!(!remove_recorded_target_or_warn(&path, ""));
        assert!(!remove_recorded_target_or_warn(&path, "junction"));
        assert!(path.is_dir());
    }

    /// An absent target is already gone; the caller can drop the row.
    #[test]
    fn absent_target_counts_as_removed() {
        let tmp = tempdir().unwrap();
        assert!(remove_recorded_target_or_warn(&tmp.path().join("nope"), "symlink"));
    }

    /// Copy mode is a full-content deployment, so it still replaces a real
    /// directory — that is the documented contract, and its row vouches for it.
    #[test]
    fn copy_deploy_still_replaces_a_real_directory() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# ours").unwrap();

        let target = tmp.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("stale.txt"), "old").unwrap();

        let mode = sync_skill(&source, &target, SyncMode::Copy, ReplacePolicy::UserConfirmed).unwrap();
        assert_eq!(mode.as_str(), "copy");
        assert!(!target.join("stale.txt").exists());
        assert_eq!(
            fs::read_to_string(target.join("SKILL.md")).unwrap(),
            "# ours"
        );
    }

    /// #363 headline: deploying with no recorded claim over a directory we
    /// did not create is refused, and the user's content survives.
    #[test]
    fn no_clobber_refuses_a_real_directory() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# ours").unwrap();

        let target = tmp.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("mine.txt"), "DO_NOT_OVERWRITE").unwrap();

        let err = sync_skill(&source, &target, SyncMode::Copy, ReplacePolicy::NoClobber)
            .unwrap_err();
        assert!(
            err.downcast_ref::<ReplaceRefused>().is_some(),
            "expected ReplaceRefused, got: {err}"
        );
        assert_eq!(
            fs::read_to_string(target.join("mine.txt")).unwrap(),
            "DO_NOT_OVERWRITE"
        );
    }

    /// A bare file at the target is never ours, under any recorded mode.
    #[test]
    fn no_record_vouches_for_a_bare_file() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# ours").unwrap();

        let target = tmp.path().join("target");
        fs::write(&target, "user file").unwrap();

        for policy in [
            ReplacePolicy::NoClobber,
            ReplacePolicy::Recorded { mode: "symlink" },
            ReplacePolicy::Recorded { mode: "copy" },
        ] {
            let err =
                sync_skill(&source, &target, SyncMode::Copy, policy).unwrap_err();
            assert!(err.downcast_ref::<ReplaceRefused>().is_some(), "{err}");
        }
        assert_eq!(fs::read_to_string(&target).unwrap(), "user file");
    }

    /// A `copy` row vouches for the real directory it wrote, so reconcile
    /// paths can refresh it without user confirmation.
    #[test]
    fn recorded_copy_row_authorizes_replacing_its_own_directory() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# v2").unwrap();

        let target = tmp.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("SKILL.md"), "# v1").unwrap();

        sync_skill(
            &source,
            &target,
            SyncMode::Copy,
            ReplacePolicy::Recorded { mode: "copy" },
        )
        .unwrap();
        assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "# v2");
    }

    /// The symlink→copy mode switch used to fail forever: `dst` is our own
    /// deployed link, canonicalizing it resolved back into the source and the
    /// recursion guard refused. Links are judged by their own location now.
    #[test]
    fn switching_a_deployed_link_to_copy_mode_is_allowed() {
        if !windows_symlink_privilege_available() {
            eprintln!("skipped: no Windows symlink privilege (admin or Developer Mode required)");
            return;
        }
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# ours").unwrap();

        let target = tmp.path().join("target");
        std::os::windows::fs::symlink_dir(&source, &target).unwrap();

        let mode = sync_skill(
            &source,
            &target,
            SyncMode::Copy,
            ReplacePolicy::Recorded { mode: "symlink" },
        )
        .unwrap();
        assert_eq!(mode.as_str(), "copy");
        assert!(
            !target.is_symlink(),
            "the link was replaced by a real directory"
        );
        assert!(target.join("SKILL.md").exists());
    }

    /// `classify_target` must judge a link by `symlink_metadata`, or a
    /// Windows directory link would be classified `RealDir` and deleted with
    /// `remove_dir_all` through whatever it points at.
    #[test]
    fn classify_reports_a_directory_link_as_a_link() {
        if !windows_symlink_privilege_available() {
            eprintln!("skipped: no Windows symlink privilege (admin or Developer Mode required)");
            return;
        }
        let tmp = tempdir().unwrap();
        let real = tmp.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let link = tmp.path().join("link");
        std::os::windows::fs::symlink_dir(&real, &link).unwrap();

        assert_eq!(
            classify_target(&link, None).unwrap(),
            TargetState::ForeignLink
        );
        assert_eq!(
            classify_target(&link, Some(&real)).unwrap(),
            TargetState::LinkToSource
        );
        // A file link too.
        let freal = tmp.path().join("file.txt");
        fs::write(&freal, "x").unwrap();
        let flink = tmp.path().join("file-link");
        std::os::windows::fs::symlink_file(&freal, &flink).unwrap();
        assert_eq!(
            classify_target(&flink, None).unwrap(),
            TargetState::ForeignLink
        );
    }

    /// The preserve-path decision used by undeploy: matching recorded
    /// deployments may be removed, everything else is kept.
    #[test]
    fn matches_recorded_deployment_reflects_the_policy_table() {
        let tmp = tempdir().unwrap();

        let dir = tmp.path().join("dir");
        fs::create_dir_all(&dir).unwrap();
        assert!(matches_recorded_deployment(&dir, "copy").unwrap());
        assert!(!matches_recorded_deployment(&dir, "symlink").unwrap());

        assert!(!matches_recorded_deployment(&tmp.path().join("gone"), "copy").unwrap());
    }

    #[test]
    fn remove_target_removes_directory_symlink() {
        if !windows_symlink_privilege_available() {
            eprintln!("skipped: no Windows symlink privilege (admin or Developer Mode required)");
            return;
        }
        let tmp = tempdir().unwrap();
        let real = tmp.path().join("real");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("SKILL.md"), "# hello").unwrap();
        let link = tmp.path().join("link");
        std::os::windows::fs::symlink_dir(&real, &link).unwrap();

        remove_target(&link).unwrap();
        assert!(!link.exists());
        assert!(real.exists());
        assert!(real.join("SKILL.md").exists());
    }

    #[test]
    fn remove_target_nonexistent_is_ok() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("does_not_exist");
        assert!(remove_target(&path).is_ok());
    }

    // ── is_target_current copy-mode freshness (issue #153) ──

    #[test]
    fn is_target_current_copy_skips_when_hashes_match_and_target_exists() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&tgt).unwrap();
        assert!(is_target_current(
            &src,
            &tgt,
            SyncMode::Copy,
            Some("hash-abc"),
            Some("hash-abc"),
        ));
    }

    #[test]
    fn is_target_current_copy_resyncs_when_target_missing_even_if_hashes_match() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target-that-was-deleted");
        // User deleted the synced directory manually; we must re-copy.
        assert!(!is_target_current(
            &src,
            &tgt,
            SyncMode::Copy,
            Some("hash-abc"),
            Some("hash-abc"),
        ));
    }

    #[test]
    fn is_target_current_copy_resyncs_when_hashes_differ() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&tgt).unwrap();
        assert!(!is_target_current(
            &src,
            &tgt,
            SyncMode::Copy,
            Some("hash-old"),
            Some("hash-new"),
        ));
    }

    #[test]
    fn is_target_current_copy_resyncs_when_either_hash_missing() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let tgt = tmp.path().join("target");
        fs::create_dir_all(&tgt).unwrap();
        // No previously recorded hash → must resync (e.g. row predates v6).
        assert!(!is_target_current(
            &src,
            &tgt,
            SyncMode::Copy,
            None,
            Some("hash-abc"),
        ));
        // Source has no current hash → must resync (defensive).
        assert!(!is_target_current(
            &src,
            &tgt,
            SyncMode::Copy,
            Some("hash-abc"),
            None,
        ));
        // Both missing → must resync.
        assert!(!is_target_current(&src, &tgt, SyncMode::Copy, None, None));
    }

    // ── sync_agent_file ──

    #[test]
    fn sync_agent_file_copy_writes_content() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("reviewer.md");
        fs::write(&src, "agent body").unwrap();
        let tgt = tmp.path().join("agents").join("reviewer.md");

        let mode = sync_agent_file(&src, &tgt, SyncMode::Copy, None, None).unwrap();
        assert!(matches!(mode, SyncMode::Copy));
        assert_eq!(fs::read_to_string(&tgt).unwrap(), "agent body");
    }

    #[test]
    fn sync_agent_file_copy_skips_when_hash_unchanged() {
        let tmp = tempdir().unwrap();
        let central = tmp.path().join("central");
        fs::create_dir_all(&central).unwrap();
        let src = central.join("reviewer.md");
        fs::write(&src, "agent body").unwrap();
        let tgt = tmp.path().join("reviewer.md");
        fs::write(&tgt, "stale-on-disk-but-hash-recorded").unwrap();

        // Hash matches and target exists → skip (no clobber of user edits).
        sync_agent_file(&src, &tgt, SyncMode::Copy, Some("h"), Some("h")).unwrap();
        assert_eq!(fs::read_to_string(&tgt).unwrap(), "stale-on-disk-but-hash-recorded");

        // Hash differs → recopy.
        sync_agent_file(&src, &tgt, SyncMode::Copy, Some("old"), Some("new")).unwrap();
        assert_eq!(fs::read_to_string(&tgt).unwrap(), "agent body");
    }

    #[test]
    fn sync_agent_file_copy_resyncs_when_target_deleted() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("reviewer.md");
        fs::write(&src, "agent body").unwrap();
        let tgt = tmp.path().join("deleted-by-user.md");

        sync_agent_file(&src, &tgt, SyncMode::Copy, Some("h"), Some("h")).unwrap();
        assert_eq!(fs::read_to_string(&tgt).unwrap(), "agent body");
    }

    #[cfg(unix)]
    #[test]
    fn sync_agent_file_symlink_links_source() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("reviewer.md");
        fs::write(&src, "agent body").unwrap();
        let tgt = tmp.path().join("reviewer-link.md");

        let mode = sync_agent_file(&src, &tgt, SyncMode::Symlink, None, None).unwrap();
        assert!(matches!(mode, SyncMode::Symlink));
        assert!(tgt.is_symlink());
    }

    #[cfg(windows)]
    #[test]
    fn sync_agent_file_symlink_falls_back_or_links() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("reviewer.md");
        fs::write(&src, "agent body").unwrap();
        let tgt = tmp.path().join("reviewer-link.md");

        let mode = sync_agent_file(&src, &tgt, SyncMode::Symlink, None, None).unwrap();
        match mode {
            SyncMode::Symlink => assert!(tgt.is_symlink()),
            SyncMode::Copy => assert_eq!(fs::read_to_string(&tgt).unwrap(), "agent body"),
        }
    }
}
