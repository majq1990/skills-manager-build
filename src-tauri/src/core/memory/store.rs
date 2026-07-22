//! CRUD helpers for the canonical unified-memory directory.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use super::{materializer, shared_root};

#[derive(Debug, Clone, Serialize)]
pub struct SavedMemory {
    pub path: String,
    pub slug: String,
    pub created: bool,
}

pub fn remember(
    title: &str,
    description: &str,
    content: &str,
    memory_type: &str,
    source_agent: Option<&str>,
) -> Result<SavedMemory> {
    let title = title.trim();
    let content = content.trim();
    if title.is_empty() {
        bail!("memory title cannot be empty");
    }
    if content.is_empty() {
        bail!("memory content cannot be empty");
    }
    let root = shared_root::ensure_shared_root()?;
    let slug = materializer::kebab_case(title);
    let path = root.join(format!("{}.md", slug));
    let created = !path.exists();
    let description = if description.trim().is_empty() {
        title
    } else {
        description.trim()
    };
    let memory_type = if memory_type.trim().is_empty() {
        "reference"
    } else {
        memory_type.trim()
    };
    let source_line = source_agent
        .filter(|v| !v.trim().is_empty())
        .map(|v| format!("\n  source_agent: {}", yaml_string(v.trim())))
        .unwrap_or_default();
    let body = format!(
        "---\nname: {}\ndescription: {}\nmetadata:\n  node_type: memory\n  type: {}{}\n---\n\n{}\n",
        yaml_string(title),
        yaml_string(description),
        yaml_string(memory_type),
        source_line,
        content
    );
    atomic_write(&path, body.as_bytes())?;
    Ok(SavedMemory {
        path: path.to_string_lossy().to_string(),
        slug,
        created,
    })
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination.parent().context("memory path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp: PathBuf = parent.join(format!(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_string_quotes_special_characters() {
        assert_eq!(yaml_string("a: b"), "\"a: b\"");
    }
}
