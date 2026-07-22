//! Per-agent memory adapter implementations.
//!
//! Claude-lineage agents use a file-link adapter when the platform allows it;
//! all other agents receive safe generated copies. Every agent additionally
//! receives the universal bridge contract, and native memory folders are
//! reconciled into the shared root before deployment.

pub mod symlink;

use std::path::Path;

use anyhow::Result;

use crate::core::memory::materializer::MaterializedMemory;
use crate::core::tool_adapters::ToolAdapter;

/// Determines which adapter strategy to use for a given tool.
///
/// The copy-based fallback (`DefaultAdapter`) is always available. Specialised
/// adapters are selected by tool key.
pub fn resolve(adapter: &ToolAdapter) -> Box<dyn MemoryAdapter> {
    match adapter.key.as_str() {
        "claude_code" | "cursor" | "codex" => Box::new(symlink::SymlinkAdapter),
        _ => Box::new(DefaultAdapter),
    }
}

/// Interface for deploying and removing materialized memories in an agent's
/// skills directory.
pub trait MemoryAdapter {
    /// Deploy a single materialized memory as a skill into `target_dir`.
    fn deploy(&self, memory: &MaterializedMemory, target_dir: &Path) -> Result<()>;

    /// Remove a previously deployed memory skill from `target_dir`.
    fn remove(&self, skill_name: &str, target_dir: &Path) -> Result<()>;

    /// Human-readable name for diagnostics.
    fn name(&self) -> &'static str;
}

/// The default copy-based adapter used by Phase 1 `sync`.
///
/// Writes a `SKILL.md` file into a `memory-<slug>` subdirectory under the
/// agent's skills dir. This is the universal fallback for any agent that does
/// not have a specialised adapter.
pub struct DefaultAdapter;

impl MemoryAdapter for DefaultAdapter {
    fn deploy(&self, memory: &MaterializedMemory, target_dir: &Path) -> Result<()> {
        let skill_dir = target_dir.join(&memory.skill_name);
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(skill_dir.join("SKILL.md"), memory.skill_body.as_bytes())?;
        Ok(())
    }

    fn remove(&self, skill_name: &str, target_dir: &Path) -> Result<()> {
        let dir = target_dir.join(skill_name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "default_copy"
    }
}
