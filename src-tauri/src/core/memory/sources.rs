//! Discover native memory directories and reconcile them into the shared root.
//!
//! The shared root is the single source of truth. Agent-native memory folders
//! remain valid write targets: every memory sync imports newer files from those
//! folders before materialising read replicas for all installed agents.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::shared_root;

const STATE_DIR: &str = ".skills-manager";

#[derive(Debug, Clone, Serialize)]
pub struct MemorySourceReport {
    pub agent: String,
    pub path: String,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconcileReport {
    pub shared_root: String,
    pub sources: Vec<MemorySourceReport>,
    pub discovered: usize,
    pub imported: usize,
    pub replaced_newer: usize,
    pub deduplicated: usize,
    pub ignored_older: usize,
    pub backups: usize,
    pub errors: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
struct SourceDir {
    agent: &'static str,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct Candidate {
    agent: &'static str,
    path: PathBuf,
    modified: u64,
}

/// Reconcile all known agent-native memory directories into `~/.agent-memory`.
pub fn reconcile(dry_run: bool) -> Result<ReconcileReport> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let root = shared_root::shared_root();
    reconcile_at(&home, &root, dry_run)
}

fn reconcile_at(home: &Path, root: &Path, dry_run: bool) -> Result<ReconcileReport> {
    if !dry_run {
        fs::create_dir_all(root)
            .with_context(|| format!("creating shared memory root {}", root.display()))?;
    }

    let sources = discover_sources(home, root);
    let mut report = ReconcileReport {
        shared_root: root.to_string_lossy().to_string(),
        sources: Vec::new(),
        discovered: 0,
        imported: 0,
        replaced_newer: 0,
        deduplicated: 0,
        ignored_older: 0,
        backups: 0,
        errors: Vec::new(),
        dry_run,
    };

    let mut candidates = Vec::new();
    for source in sources {
        let mut count = 0;
        match memory_files(&source.path) {
            Ok(files) => {
                count = files.len();
                for path in files {
                    candidates.push(Candidate {
                        agent: source.agent,
                        modified: modified_secs(&path),
                        path,
                    });
                }
            }
            Err(err) => report.errors.push(format!(
                "{} ({}): {}",
                source.agent,
                source.path.display(),
                err
            )),
        }
        report.sources.push(MemorySourceReport {
            agent: source.agent.to_string(),
            path: source.path.to_string_lossy().to_string(),
            files: count,
        });
    }
    candidates.sort_by(|a, b| a.modified.cmp(&b.modified).then(a.path.cmp(&b.path)));
    report.discovered = candidates.len();

    let mut hashes: HashMap<String, PathBuf> = HashMap::new();
    if root.exists() {
        for path in memory_files(root)? {
            if let Ok(bytes) = fs::read(&path) {
                hashes.insert(content_hash(&bytes), path);
            }
        }
    }

    for candidate in candidates {
        if same_file_or_path(&candidate.path, root) {
            continue;
        }
        let bytes = match fs::read(&candidate.path) {
            Ok(bytes) => bytes,
            Err(err) => {
                report
                    .errors
                    .push(format!("read {}: {}", candidate.path.display(), err));
                continue;
            }
        };
        let hash = content_hash(&bytes);
        if hashes.contains_key(&hash) {
            report.deduplicated += 1;
            continue;
        }

        let Some(filename) = candidate.path.file_name() else {
            continue;
        };
        let destination = root.join(filename);
        if !destination.exists() {
            report.imported += 1;
            if !dry_run {
                if let Err(err) = atomic_write(&destination, &bytes) {
                    report
                        .errors
                        .push(format!("write {}: {}", destination.display(), err));
                    report.imported -= 1;
                    continue;
                }
            }
            hashes.insert(hash, destination);
            continue;
        }

        let destination_modified = modified_secs(&destination);
        if candidate.modified > destination_modified {
            report.replaced_newer += 1;
            report.backups += 1;
            if !dry_run {
                if let Err(err) = backup_and_replace(root, candidate.agent, &destination, &bytes) {
                    report
                        .errors
                        .push(format!("replace {}: {}", destination.display(), err));
                    report.replaced_newer -= 1;
                    report.backups -= 1;
                    continue;
                }
            }
            hashes.insert(hash, destination);
        } else {
            report.ignored_older += 1;
        }
    }

    if !dry_run {
        append_audit(root, &report)?;
    }
    Ok(report)
}

fn discover_sources(home: &Path, root: &Path) -> Vec<SourceDir> {
    let mut sources = Vec::new();
    let fixed = [
        ("claude", home.join(".claude/memory")),
        ("codex", home.join(".codex/memory")),
        ("codex", home.join(".Codex/memory")),
        ("opencode", home.join(".config/opencode/memory")),
        ("opencode", home.join(".local/share/opencode/memory")),
        ("workbuddy", home.join(".workbuddy/memory")),
        ("cursor", home.join(".cursor/memory")),
    ];
    for (agent, path) in fixed {
        push_source(&mut sources, agent, path, root);
    }

    for (agent, base) in [
        ("claude", home.join(".claude/projects")),
        ("codex", home.join(".codex/projects")),
        ("codex", home.join(".Codex/projects")),
        ("workbuddy", home.join(".workbuddy/projects")),
        ("cursor", home.join(".cursor/projects")),
    ] {
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    push_source(&mut sources, agent, entry.path().join("memory"), root);
                }
            }
        }
    }
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    sources.dedup_by(|a, b| paths_equal(&a.path, &b.path));
    sources
}

fn push_source(sources: &mut Vec<SourceDir>, agent: &'static str, path: PathBuf, root: &Path) {
    if path.is_dir()
        && !paths_equal(&path, root)
        && !sources
            .iter()
            .any(|source| paths_equal(&source.path, &path))
    {
        sources.push(SourceDir { agent, path });
    }
}

fn memory_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|v| v.to_str()) != Some("md") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        if name.eq_ignore_ascii_case("MEMORY.md")
            || name.eq_ignore_ascii_case("SKILL.md")
            || name.starts_with('.')
        {
            continue;
        }
        files.push(path);
    }
    files.sort();
    Ok(files)
}

fn modified_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let (a, b) = match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => (a.to_path_buf(), b.to_path_buf()),
    };
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn same_file_or_path(file: &Path, root: &Path) -> bool {
    file.parent().map(|p| paths_equal(p, root)).unwrap_or(false)
}

fn content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination.parent().context("destination has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        destination
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("memory"),
        std::process::id()
    ));
    fs::write(&temp, bytes)?;
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(temp, destination)?;
    Ok(())
}

fn backup_and_replace(root: &Path, agent: &str, destination: &Path, bytes: &[u8]) -> Result<()> {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let backup_dir = root.join(STATE_DIR).join("backups").join(stamp);
    fs::create_dir_all(&backup_dir)?;
    let filename = destination
        .file_name()
        .context("destination has no filename")?;
    let backup = backup_dir.join(format!("{}--{}", agent, filename.to_string_lossy()));
    fs::copy(destination, backup)?;
    atomic_write(destination, bytes)
}

fn append_audit(root: &Path, report: &ReconcileReport) -> Result<()> {
    let state = root.join(STATE_DIR);
    fs::create_dir_all(&state)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.join("reconcile.jsonl"))?;
    let event = serde_json::json!({
        "at": Utc::now().to_rfc3339(),
        "report": report,
    });
    writeln!(file, "{}", serde_json::to_string(&event)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn imports_and_deduplicates_native_memories() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let source = home.join(".workbuddy/memory");
        let root = home.join("shared");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("one.md"), "hello").unwrap();
        fs::write(source.join("two.md"), "hello").unwrap();

        let report = reconcile_at(home, &root, false).unwrap();
        assert_eq!(report.discovered, 2);
        assert_eq!(report.imported, 1);
        assert_eq!(report.deduplicated, 1);
        assert_eq!(fs::read_to_string(root.join("one.md")).unwrap(), "hello");
    }

    #[test]
    fn dry_run_does_not_create_shared_root() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let source = home.join(".claude/memory");
        let root = home.join("shared");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("one.md"), "hello").unwrap();

        let report = reconcile_at(home, &root, true).unwrap();
        assert_eq!(report.imported, 1);
        assert!(!root.exists());
    }

    #[test]
    fn newer_native_memory_replaces_with_backup() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let source = home.join(".workbuddy/memory");
        let root = home.join("shared");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("topic.md"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(source.join("topic.md"), "new").unwrap();

        let report = reconcile_at(home, &root, false).unwrap();
        assert_eq!(report.replaced_newer, 1);
        assert_eq!(report.backups, 1);
        assert_eq!(fs::read_to_string(root.join("topic.md")).unwrap(), "new");
        let backup_root = root.join(STATE_DIR).join("backups");
        assert!(fs::read_dir(backup_root).unwrap().next().is_some());
    }
}
