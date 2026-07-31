use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::central_repo;
use super::content_hash;
use super::skill_metadata::{self, sanitize_skill_name};
use super::sync_engine;

pub struct InstallResult {
    pub name: String,
    pub description: Option<String>,
    pub central_path: PathBuf,
    pub content_hash: String,
    pub source_subpath: Option<String>,
}

enum PreparedSource {
    Directory {
        root: PathBuf,
        skill_dirs: Vec<PathBuf>,
    },
    Archive {
        _temp_dir: tempfile::TempDir,
        root: PathBuf,
        skill_dirs: Vec<PathBuf>,
    },
}

impl PreparedSource {
    fn open(source: &Path) -> Result<Self> {
        if source.is_dir() {
            Ok(PreparedSource::Directory {
                root: source.to_path_buf(),
                skill_dirs: discover_skill_dirs(source)?,
            })
        } else {
            Self::from_archive(source)
        }
    }

    fn from_archive(source: &Path) -> Result<Self> {
        let ext = source
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        if ext != "zip" && ext != "skill" {
            bail!("Unsupported archive format: {}", ext);
        }

        let temp_dir = tempfile::tempdir()?;
        let file = std::fs::File::open(source)?;
        let mut archive = zip::ZipArchive::new(file)?;
        safe_extract(&mut archive, temp_dir.path())?;
        let root = temp_dir.path().to_path_buf();
        let skill_dirs = discover_skill_dirs(&root)?;

        Ok(PreparedSource::Archive {
            _temp_dir: temp_dir,
            root,
            skill_dirs,
        })
    }

    fn skill_dir(&self) -> &Path {
        self.skill_dirs()
            .first()
            .map(|p| p.as_path())
            .unwrap_or_else(|| self.root())
    }

    fn root(&self) -> &Path {
        match self {
            PreparedSource::Directory { root, .. } => root,
            PreparedSource::Archive { root, .. } => root,
        }
    }

    fn skill_dirs(&self) -> &[PathBuf] {
        match self {
            PreparedSource::Directory { skill_dirs, .. } => skill_dirs,
            PreparedSource::Archive { skill_dirs, .. } => skill_dirs,
        }
    }

    fn source_subpath(&self, skill_dir: &Path) -> Option<String> {
        let rel = skill_dir.strip_prefix(self.root()).ok()?;
        if rel.as_os_str().is_empty() {
            None
        } else {
            Some(rel.to_string_lossy().replace('\\', "/"))
        }
    }
}

pub fn install_from_local(source: &Path, name: Option<&str>) -> Result<InstallResult> {
    let prepared = PreparedSource::open(source)?;
    if prepared.skill_dirs().len() > 1 {
        bail!(
            "Multiple skill directories found; use install_all_from_local for multi-skill packages"
        );
    }
    let skill_dir = prepared.skill_dir();

    let sanitized_name = match name {
        Some(n) if !n.is_empty() => {
            sanitize_skill_name(n).ok_or_else(|| anyhow::anyhow!("Invalid skill name: '{}'", n))?
        }
        _ => skill_metadata::infer_skill_name(skill_dir),
    };

    let skills_dir = central_repo::skills_dir();
    let dest = unique_skill_dest(&skills_dir, &sanitized_name, skill_dir)?;
    let final_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| sanitized_name.clone());

    let mut result = install_skill_dir_to_destination(skill_dir, &final_name, &dest)?;
    result.source_subpath = prepared.source_subpath(skill_dir);
    Ok(result)
}

pub fn install_all_from_local(source: &Path, name: Option<&str>) -> Result<Vec<InstallResult>> {
    let prepared = PreparedSource::open(source)?;
    let skill_dirs = if prepared.skill_dirs().is_empty() {
        vec![prepared.root().to_path_buf()]
    } else {
        prepared.skill_dirs().to_vec()
    };
    let use_override_name = skill_dirs.len() == 1;
    let skills_dir = central_repo::skills_dir();
    let mut results = Vec::new();

    for skill_dir in skill_dirs {
        let sanitized_name = if use_override_name {
            match name {
                Some(n) if !n.is_empty() => sanitize_skill_name(n)
                    .ok_or_else(|| anyhow::anyhow!("Invalid skill name: '{}'", n))?,
                _ => skill_metadata::infer_skill_name(&skill_dir),
            }
        } else {
            skill_metadata::infer_skill_name(&skill_dir)
        };

        let dest = unique_skill_dest(&skills_dir, &sanitized_name, &skill_dir)?;
        let final_name = dest
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| sanitized_name.clone());

        let mut result = install_skill_dir_to_destination(&skill_dir, &final_name, &dest)?;
        result.source_subpath = prepared.source_subpath(&skill_dir);
        results.push(result);
    }

    Ok(results)
}

pub fn install_from_local_to_destination(
    source: &Path,
    name: Option<&str>,
    destination: &Path,
) -> Result<InstallResult> {
    let prepared = PreparedSource::open(source)?;
    let skill_dir = prepared.skill_dir();

    let skill_name = match name {
        Some(n) if !n.is_empty() => {
            sanitize_skill_name(n).ok_or_else(|| anyhow::anyhow!("Invalid skill name: '{}'", n))?
        }
        _ => skill_metadata::infer_skill_name(skill_dir),
    };
    install_skill_dir_to_destination(skill_dir, &skill_name, destination)
}

pub fn resolve_local_skill_name(source: &Path, name: Option<&str>) -> Result<String> {
    let prepared = PreparedSource::open(source)?;
    let skill_dir = prepared.skill_dir();

    Ok(match name {
        Some(n) if !n.is_empty() => {
            sanitize_skill_name(n).ok_or_else(|| anyhow::anyhow!("Invalid skill name: '{}'", n))?
        }
        _ => skill_metadata::infer_skill_name(skill_dir),
    })
}

pub fn hash_local_source(source: &Path) -> Result<String> {
    let prepared = PreparedSource::open(source)?;
    content_hash::hash_directory(prepared.skill_dir())
}

pub fn install_from_git_dir(source: &Path, name: Option<&str>) -> Result<InstallResult> {
    install_from_local(source, name)
}

pub fn install_skill_dir_to_destination(
    source: &Path,
    name: &str,
    destination: &Path,
) -> Result<InstallResult> {
    let meta = skill_metadata::parse_skill_md(source);

    sync_engine::ensure_dst_not_inside_src(source, destination)?;

    if destination.exists() {
        std::fs::remove_dir_all(destination)
            .with_context(|| format!("Failed to remove existing {:?}", destination))?;
    }

    copy_skill_dir(source, destination)?;

    let hash = content_hash::hash_directory(destination)?;

    Ok(InstallResult {
        name: name.to_string(),
        description: meta.description,
        central_path: destination.to_path_buf(),
        content_hash: hash,
        source_subpath: None,
    })
}

fn discover_skill_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    if skill_metadata::is_valid_skill_dir(root) {
        return Ok(vec![root.to_path_buf()]);
    }

    let mut found = Vec::new();
    collect_skill_dirs(root, &mut found)?;
    found.sort();
    found.dedup();
    Ok(found)
}

fn collect_skill_dirs(dir: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == "node_modules" || name == ".hub" {
            continue;
        }

        if skill_metadata::is_valid_skill_dir(&path) {
            found.push(path);
        } else {
            collect_skill_dirs(&path, found)?;
        }
    }

    Ok(())
}

/// Extract a ZIP archive into `dest`, skipping any entry whose path would
/// escape the destination directory (Zip Slip defence).
fn safe_extract(archive: &mut zip::ZipArchive<std::fs::File>, dest: &Path) -> Result<()> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;

        // enclosed_name() returns None for absolute paths and entries that
        // contain `..` components, so those are silently skipped.
        let entry_path = match entry.enclosed_name() {
            Some(name) => dest.join(name),
            None => continue,
        };

        // Belt-and-suspenders: verify the resolved path stays inside dest.
        if !entry_path.starts_with(dest) {
            continue;
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&entry_path)?;
        } else {
            if let Some(parent) = entry_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&entry_path)?;
            std::io::copy(&mut entry, &mut outfile)?;

            // Restore Unix file permissions (especially executable bits)
            // from the ZIP entry metadata.
            #[cfg(unix)]
            {
                if let Some(mode) = entry.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &entry_path,
                        std::fs::Permissions::from_mode(mode),
                    );
                }
            }
        }
    }
    Ok(())
}

/// Return a collision-safe destination directory for an install.
///
/// Rules:
/// - Prefer `<name>` if missing.
/// - Reuse an existing directory when it clearly belongs to the same skill
///   (same metadata `name`, or legacy no-metadata `<name>` directory).
/// - Otherwise allocate `<name>-2`, `<name>-3`, ...
fn unique_skill_dest(parent: &Path, sanitized_name: &str, source: &Path) -> Result<PathBuf> {
    let source_hash = content_hash::hash_directory(source)?;

    for i in 1u32.. {
        let candidate = if i == 1 {
            parent.join(sanitized_name)
        } else {
            parent.join(format!("{}-{}", sanitized_name, i))
        };

        if !candidate.exists() {
            return Ok(candidate);
        }

        if content_hash::hash_directory(&candidate).ok().as_deref() == Some(source_hash.as_str()) {
            return Ok(candidate);
        }
    }

    Ok(parent.join(sanitized_name))
}

fn copy_skill_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str == ".git" || name_str == ".DS_Store" {
            continue;
        }

        // Skip symlinks to prevent exfiltration of files outside the skill directory
        if ft.is_symlink() {
            continue;
        }

        let dest_path = dst.join(&name);
        if ft.is_dir() {
            copy_skill_dir(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    fn make_skill_dir(parent: &Path, dir_name: &str, meta_name: Option<&str>) -> PathBuf {
        let dir = parent.join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(name) = meta_name {
            std::fs::write(dir.join("SKILL.md"), format!("---\nname: {}\n---\n", name)).unwrap();
        }
        dir
    }

    #[test]
    fn unique_dest_returns_base_when_free() {
        let tmp = tempdir().unwrap();
        let source = make_skill_dir(tmp.path(), "source", Some("a-b"));
        let dest = unique_skill_dest(tmp.path(), "a-b", &source).unwrap();
        assert_eq!(dest, tmp.path().join("a-b"));
    }

    #[test]
    fn unique_dest_reuses_base_for_same_content() {
        let tmp = tempdir().unwrap();
        let existing = make_skill_dir(tmp.path(), "a-b", Some("A B"));
        let source = make_skill_dir(tmp.path(), "source", Some("A B"));
        std::fs::write(existing.join("body.md"), "same").unwrap();
        std::fs::write(source.join("body.md"), "same").unwrap();

        let dest = unique_skill_dest(tmp.path(), "a-b", &source).unwrap();
        assert_eq!(dest, tmp.path().join("a-b"));
    }

    #[test]
    fn unique_dest_uses_suffix_for_different_content_even_if_name_matches() {
        let tmp = tempdir().unwrap();
        let existing = make_skill_dir(tmp.path(), "a-b", Some("A-B"));
        let source = make_skill_dir(tmp.path(), "source", Some("A-B"));
        std::fs::write(existing.join("body.md"), "old").unwrap();
        std::fs::write(source.join("body.md"), "new").unwrap();

        let dest = unique_skill_dest(tmp.path(), "a-b", &source).unwrap();
        assert_eq!(dest, tmp.path().join("a-b-2"));
    }

    #[test]
    fn unique_dest_reuses_existing_suffix_for_same_content() {
        let tmp = tempdir().unwrap();
        let first = make_skill_dir(tmp.path(), "a-b", Some("A-B"));
        let second = make_skill_dir(tmp.path(), "a-b-2", Some("A-B"));
        let source = make_skill_dir(tmp.path(), "source", Some("A-B"));
        std::fs::write(first.join("body.md"), "first").unwrap();
        std::fs::write(second.join("body.md"), "second").unwrap();
        std::fs::write(source.join("body.md"), "second").unwrap();

        let dest = unique_skill_dest(tmp.path(), "a-b", &source).unwrap();
        assert_eq!(dest, tmp.path().join("a-b-2"));
    }

    #[test]
    fn install_skill_dir_refuses_destination_inside_source() {
        let tmp = tempdir().unwrap();
        let source = make_skill_dir(tmp.path(), "skills", Some("skills"));
        std::fs::write(source.join("body.md"), "data").unwrap();
        let destination = source.join("skills");

        let err = install_skill_dir_to_destination(&source, "skills", &destination)
            .err()
            .expect("expected refusal");
        assert!(
            err.to_string().contains("infinite recursion"),
            "unexpected error: {err}"
        );
        // The source must not be touched, and no nested copy must exist.
        assert!(source.join("body.md").exists());
        assert!(!destination.exists());
    }

    #[test]
    fn unique_dest_legacy_no_metadata_base_can_reinstall_if_content_matches() {
        let tmp = tempdir().unwrap();
        let existing = make_skill_dir(tmp.path(), "legacy", None);
        let source = make_skill_dir(tmp.path(), "source", None);
        std::fs::write(existing.join("body.md"), "same").unwrap();
        std::fs::write(source.join("body.md"), "same").unwrap();

        let dest = unique_skill_dest(tmp.path(), "legacy", &source).unwrap();
        assert_eq!(dest, tmp.path().join("legacy"));
    }

    fn write_skill_archive(path: &Path, body: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("demo-skill/SKILL.md", options).unwrap();
        zip.write_all(b"---\nname: Demo Skill\n---\n").unwrap();
        zip.start_file("demo-skill/body.md", options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    fn with_test_repo<T>(f: impl FnOnce() -> T) -> T {
        let _lock = central_repo::test_base_dir_lock();
        let tmp = tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(tmp.path().join("repo")));
        let result = f();
        central_repo::set_test_base_dir_override(None);
        result
    }

    #[test]
    fn hash_local_source_matches_extracted_archive_representation() {
        let tmp = tempdir().unwrap();
        let archive = tmp.path().join("demo.skill");
        write_skill_archive(&archive, "same content");

        let extracted = tmp.path().join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        std::fs::write(extracted.join("SKILL.md"), "---\nname: Demo Skill\n---\n").unwrap();
        std::fs::write(extracted.join("body.md"), "same content").unwrap();

        let archive_hash = hash_local_source(&archive).unwrap();
        let dir_hash = content_hash::hash_directory(&extracted).unwrap();

        assert_eq!(archive_hash, dir_hash);
    }

    #[test]
    fn hash_local_source_detects_archive_content_changes() {
        let tmp = tempdir().unwrap();
        let archive = tmp.path().join("demo.zip");
        write_skill_archive(&archive, "v1");
        let first_hash = hash_local_source(&archive).unwrap();

        write_skill_archive(&archive, "v2");
        let second_hash = hash_local_source(&archive).unwrap();

        assert_ne!(first_hash, second_hash);
    }

    #[test]
    fn install_all_from_archive_installs_multiple_skill_dirs() {
        with_test_repo(|| {
            let tmp = tempdir().unwrap();
            let archive_path = tmp.path().join("bundle.zip");
            let file = std::fs::File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = SimpleFileOptions::default();

            zip.start_file("category-a/alpha/SKILL.md", options)
                .unwrap();
            zip.write_all(b"---\nname: Alpha\n---\n").unwrap();
            zip.start_file("category-a/alpha/references/a.md", options)
                .unwrap();
            zip.write_all(b"alpha ref").unwrap();
            zip.start_file("category-b/beta/SKILL.md", options).unwrap();
            zip.write_all(b"---\nname: Beta\n---\n").unwrap();
            zip.finish().unwrap();

            let results = install_all_from_local(&archive_path, Some("ignored")).unwrap();
            let names: Vec<_> = results.iter().map(|r| r.name.as_str()).collect();

            assert_eq!(results.len(), 2);
            assert!(names.contains(&"Alpha"));
            assert!(names.contains(&"Beta"));
            assert!(results
                .iter()
                .any(|r| r.source_subpath.as_deref() == Some("category-a/alpha")));
            assert!(results
                .iter()
                .any(|r| r.source_subpath.as_deref() == Some("category-b/beta")));
        });
    }

    #[test]
    fn install_all_from_archive_keeps_nested_skill_md_inside_single_skill() {
        with_test_repo(|| {
            let tmp = tempdir().unwrap();
            let archive_path = tmp.path().join("nested.zip");
            let file = std::fs::File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = SimpleFileOptions::default();

            zip.start_file("parent/SKILL.md", options).unwrap();
            zip.write_all(b"---\nname: Parent\n---\n").unwrap();
            zip.start_file("parent/child/SKILL.md", options).unwrap();
            zip.write_all(b"---\nname: Child\n---\n").unwrap();
            zip.finish().unwrap();

            let results = install_all_from_local(&archive_path, None).unwrap();

            assert_eq!(results.len(), 1);
            assert_eq!(results[0].name, "Parent");
            assert!(results[0].central_path.join("child/SKILL.md").exists());
        });
    }
}
