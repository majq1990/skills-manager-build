//! Bi-directional symlink adapter for claude-lineage agents.
//!
//! Instead of copying `SKILL.md` into the agent's skills directory (which breaks
//! when the agent writes back to the memory file), this adapter creates a
//! filesystem symlink from the shared root source directly into the agent's
//! skills dir.
//!
//! A symlink means:
//! - Agent-initiated writes to `<agent>/skills/memory-<slug>/<file>` flow back
//!   to `~/.agent-memory/<source>.md`.
//! - `sync_all()` does NOT need to re-deploy unless the file is new or removed.
//!
//! ## Platform notes
//!
//! - **Unix / macOS**: `std::os::unix::fs::symlink` works for both files and
//!   directories.
//! - **Windows**: directory symlinks require `SeCreateSymbolicLinkPrivilege`
//!   (admin / Developer Mode). When symlink creation fails we fall back to
//!   the `DefaultAdapter` copy-based approach.
//!
//! ## Agent eligibility
//!
//! Only agents known to write memory autonomously get the symlink adapter:
//! `claude_code`, `cursor`, `codex`. Other agents continue using the default
//! copy path.

use std::path::Path;

use anyhow::{Context, Result};

use crate::core::memory::adapters::MemoryAdapter;
use crate::core::memory::materializer::MaterializedMemory;

/// Symlink-based memory adapter for claude-lineage agents.
pub struct SymlinkAdapter;

impl MemoryAdapter for SymlinkAdapter {
    fn deploy(&self, memory: &MaterializedMemory, target_dir: &Path) -> Result<()> {
        let skill_dir = target_dir.join(&memory.skill_name);
        let link_path = skill_dir.join("SKILL.md");
        let source = &memory.source_path;

        // If the symlink already points to the right place, skip.
        if link_path.exists() {
            if let Ok(target) = std::fs::read_link(&link_path) {
                if target == Path::new(source) {
                    return Ok(());
                }
            }
            // Wrong target or not a symlink — remove first.
            if link_path.is_dir() {
                let _ = std::fs::remove_dir_all(&link_path);
            } else {
                let _ = std::fs::remove_file(&link_path);
            }
        }

        // Remove the legacy Phase-2 layout where the skill directory itself
        // was incorrectly linked to a Markdown file.
        if let Ok(meta) = std::fs::symlink_metadata(&skill_dir) {
            if meta.file_type().is_symlink() {
                #[cfg(windows)]
                let _ = std::fs::remove_dir(&skill_dir);
                #[cfg(unix)]
                let _ = std::fs::remove_file(&skill_dir);
            }
        }

        std::fs::create_dir_all(&skill_dir)
            .with_context(|| format!("creating skill dir {}", skill_dir.display()))?;

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(source, &link_path)
                .with_context(|| format!("symlink {} -> {}", link_path.display(), source))?;
        }

        #[cfg(windows)]
        {
            if let Err(e) = std::os::windows::fs::symlink_file(source, &link_path) {
                log::warn!(
                    "symlink_file {} -> {} failed ({}), falling back to copy",
                    link_path.display(),
                    source,
                    e
                );
                // Fallback: create a copy via DefaultAdapter.
                let _ = std::fs::remove_dir_all(&skill_dir);
                let fallback = super::DefaultAdapter;
                fallback.deploy(memory, target_dir)?;
            }
        }

        Ok(())
    }

    fn remove(&self, skill_name: &str, target_dir: &Path) -> Result<()> {
        let skill_dir = target_dir.join(skill_name);
        if !skill_dir.exists() {
            return Ok(());
        }
        let link_path = skill_dir.join("SKILL.md");
        if std::fs::read_link(&link_path).is_ok() {
            std::fs::remove_file(&link_path)?;
            std::fs::remove_dir(&skill_dir)?;
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "symlink"
    }
}
