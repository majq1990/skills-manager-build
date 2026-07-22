//! Turn shared-root `.md` memory files into agent-consumable SKILL.md units.
//!
//! Each memory file becomes one skill named `memory-<kebab-slug>`. The prefix
//! is a load-bearing marker: `sync::sync_tool` uses it to identify stale
//! memory-derived skills that should be swept away when a memory file is
//! removed from the shared root. Do not change it without updating the sweep.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const MEMORY_SKILL_PREFIX: &str = "memory-";

/// A single memory file rendered as a deploy-ready skill.
#[derive(Debug, Clone, Serialize)]
pub struct MaterializedMemory {
    pub slug: String,
    pub skill_name: String,
    pub description: String,
    pub source_path: String,
    pub memory_type: String,
    #[serde(skip_serializing)]
    pub skill_body: String,
}

#[derive(Debug, Deserialize)]
struct MemoryFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata: Option<serde_yaml::Value>,
}

/// Scan the shared root and materialize every eligible memory file.
///
/// Skipped: files under nested dirs, dotfiles, and `MEMORY.md` (an index).
pub fn scan_and_materialize(root: &Path) -> Result<Vec<MaterializedMemory>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("read_dir {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if filename == "MEMORY.md" || filename.starts_with('.') {
            continue;
        }
        match materialize_one(&path) {
            Ok(m) => out.push(m),
            Err(err) => {
                log::warn!("memory materializer: skipping {}: {}", path.display(), err);
            }
        }
    }
    out.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(out)
}

fn materialize_one(path: &Path) -> Result<MaterializedMemory> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let (frontmatter_yaml, body) = split_frontmatter(&raw);
    let fm: MemoryFrontmatter = if frontmatter_yaml.is_empty() {
        MemoryFrontmatter {
            name: None,
            description: None,
            metadata: None,
        }
    } else {
        serde_yaml::from_str(frontmatter_yaml).unwrap_or(MemoryFrontmatter {
            name: None,
            description: None,
            metadata: None,
        })
    };
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let slug_source = fm.name.clone().unwrap_or_else(|| stem.clone());
    let slug = kebab_case(&slug_source);
    let skill_name = format!("{}{}", MEMORY_SKILL_PREFIX, slug);
    let description_original = fm
        .description
        .clone()
        .unwrap_or_else(|| format!("Personal memory: {}", slug));
    let description = format!("[memory] {}", description_original);
    let memory_type = fm
        .metadata
        .as_ref()
        .and_then(|m| m.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unspecified")
        .to_string();

    let skill_body = build_skill_body(&skill_name, &description, &memory_type, body, path);
    Ok(MaterializedMemory {
        slug,
        skill_name,
        description,
        source_path: path.to_string_lossy().to_string(),
        memory_type,
        skill_body,
    })
}

/// Split a Markdown file into (frontmatter YAML, body). Empty YAML if no fence.
fn split_frontmatter(raw: &str) -> (&str, &str) {
    for prefix in ["---\n", "---\r\n"] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            for terminator in ["\n---\n", "\n---\r\n", "\r\n---\r\n", "\r\n---\n"] {
                if let Some(end_idx) = rest.find(terminator) {
                    let front = &rest[..end_idx];
                    let body_start = end_idx + terminator.len();
                    return (front, &rest[body_start..]);
                }
            }
        }
    }
    ("", raw)
}

pub(crate) fn kebab_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_sep = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('-');
            last_was_sep = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "memory".to_string()
    } else {
        trimmed.to_string()
    }
}

fn build_skill_body(
    skill_name: &str,
    description: &str,
    memory_type: &str,
    body: &str,
    source_path: &Path,
) -> String {
    format!(
        "---\nname: {name}\ndescription: {desc}\n---\n\n<!-- Auto-generated by skills-manager memory module.\n     Source: {src}\n     Do not edit here — edit the source and run `skills-manager-cli memory sync`. -->\n\n**Memory type**: {mtype}\n\n{body}\n",
        name = skill_name,
        desc = description,
        src = source_path.display(),
        mtype = memory_type,
        body = body.trim(),
    )
}
