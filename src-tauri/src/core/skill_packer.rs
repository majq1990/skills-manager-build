use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn pack_skill(skill_dir: &Path) -> Result<Vec<u8>> {
    let has_skill_md = skill_dir.join("SKILL.md").exists() || skill_dir.join("skill.md").exists();
    if !has_skill_md {
        anyhow::bail!("SKILL.md not found in {}", skill_dir.display());
    }

    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in WalkDir::new(skill_dir).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        name != ".git" && name != "node_modules" && name != "__pycache__"
    }) {
        let entry = entry.context("Failed to walk skill directory")?;
        let path = entry.path();
        let rel = path
            .strip_prefix(skill_dir)
            .context("Failed to compute relative path")?;

        if rel.as_os_str().is_empty() || path.is_dir() {
            continue;
        }

        let rel_str = rel.to_string_lossy().replace('\\', "/");
        files.push((rel_str, path.to_path_buf()));
    }

    // Sort: SKILL.md/skill.md first for Node.js unzipper streaming compatibility
    files.sort_by(|a, b| {
        let a_is_skill = a.0.eq_ignore_ascii_case("skill.md");
        let b_is_skill = b.0.eq_ignore_ascii_case("skill.md");
        b_is_skill.cmp(&a_is_skill).then_with(|| a.0.cmp(&b.0))
    });

    for (rel_str, path) in &files {
        zip.start_file(rel_str, options)?;
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        zip.write_all(&data)?;
    }

    let cursor = zip.finish().context("Failed to finalize ZIP")?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_requires_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        let result = pack_skill(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SKILL.md"));
    }

    #[test]
    fn pack_creates_valid_zip() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("SKILL.md"), "# Test skill").unwrap();
        std::fs::write(tmp.path().join("main.py"), "print('hi')").unwrap();
        let bytes = pack_skill(tmp.path()).unwrap();
        assert!(bytes.len() > 0);
        assert_eq!(&bytes[0..4], &[0x50, 0x4B, 0x03, 0x04]);
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    fn save_zip_for_debug() {
        let skill_dir = dirs::home_dir().unwrap().join(".skills-manager/skills/dws");
        if !skill_dir.exists() {
            println!("dws skill not found, skipping");
            return;
        }
        let bytes = pack_skill(&skill_dir).unwrap();
        let out = std::env::temp_dir().join("dws-ps.zip");
        std::fs::write(&out, &bytes).unwrap();
        println!("Saved ZIP to: {:?} ({} bytes)", out, bytes.len());
        assert!(bytes.len() > 0);
        assert_eq!(&bytes[0..4], &[0x50, 0x4B, 0x03, 0x04]);
    }
}
