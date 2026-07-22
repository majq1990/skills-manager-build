//! Shared memory root — the single filesystem location where all agents' memory
//! converges. Defaults to `~/.agent-memory/`. Overridable via `AGENT_MEMORY_ROOT`
//! env var so power users can point at a synced folder (Syncthing, git-annex,
//! NAS mount, …) without editing config.

use std::path::PathBuf;

use anyhow::{Context, Result};

pub const SHARED_ROOT_ENV: &str = "AGENT_MEMORY_ROOT";
const DEFAULT_DIR_NAME: &str = ".agent-memory";

pub fn shared_root() -> PathBuf {
    if let Ok(p) = std::env::var(SHARED_ROOT_ENV) {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(DEFAULT_DIR_NAME)
}

pub fn ensure_shared_root() -> Result<PathBuf> {
    let root = shared_root();
    std::fs::create_dir_all(&root)
        .with_context(|| format!("creating shared memory root {}", root.display()))?;
    Ok(root)
}
