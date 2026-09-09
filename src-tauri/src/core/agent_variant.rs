//! Agent definition format handling: canonical AGENT.md parsing, per-tool
//! variant resolution, and explicit (never implicit) variant generation.
//!
//! Distribution principle: deployment only ever *copies* files. Format
//! conversion happens exclusively through [`generate_variant_for_tool`],
//! which is an explicit user action whose output is archived in the central
//! repo as a reviewable, hand-editable variant file.

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::tool_adapters::AgentDeployKind;

/// Canonical definition file name inside every central agent directory.
pub const CANONICAL_FILE_NAME: &str = "AGENT.md";

/// Default `sandbox_mode` stamped into generated codex variants. Reviewers
/// can hand-edit after generation; the header comment calls this out.
const CODEX_DEFAULT_SANDBOX_MODE: &str = "read-only";

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParsedAgent {
    pub name: Option<String>,
    pub description: Option<String>,
    pub body: String,
}

/// SHA-256 of arbitrary content, hex-encoded. Used for `content_hash` /
/// `source_hash` freshness bookkeeping on the actually-deployed file.
pub fn content_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Split a markdown document into frontmatter fields and body. Returns
/// `None` when the document has no parseable `---`-fenced frontmatter.
fn split_frontmatter(content: &str) -> Option<(serde_yaml::Value, String)> {
    let trimmed = content.trim_start();
    let rest = trimmed.strip_prefix("---")?;
    let end = rest.find("---")?;
    let yaml_str = &rest[..end];
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(yaml_str).ok()?;
    let body = rest[end + 3..].trim_start_matches('\n').to_string();
    Some((yaml, body))
}

/// Parse a canonical AGENT.md (or any markdown agent definition) into
/// name/description/body. `None` when frontmatter is missing or invalid.
pub fn parse_agent_markdown(content: &str) -> Option<ParsedAgent> {
    let (yaml, body) = split_frontmatter(content)?;
    let field = |key: &str| {
        yaml.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    Some(ParsedAgent {
        name: field("name"),
        description: field("description"),
        body,
    })
}

/// Validate a candidate agent definition per the design spec 搂5.2:
/// parseable frontmatter required; empty `description` warns via the
/// returned `Ok(Some(warning))`; an empty body with no `prompt` field is
/// rejected outright.
pub fn validate_agent_markdown(content: &str) -> Result<Option<String>> {
    let parsed = parse_agent_markdown(content)
        .ok_or_else(|| anyhow!("missing or unparseable frontmatter (--- fenced YAML)"))?;
    if parsed.body.trim().is_empty() && !has_prompt_field(content) {
        bail!("agent body is empty and frontmatter has no `prompt` field");
    }
    if parsed.description.is_none() {
        return Ok(Some("no description; add one so tool pickers can rank it".into()));
    }
    Ok(None)
}

fn has_prompt_field(content: &str) -> bool {
    split_frontmatter(content)
        .map(|(yaml, _)| yaml.get("prompt").is_some())
        .unwrap_or(false)
}

/// Variant file name inside the central agent directory:
/// `<tool-key>.<ext>` for file-deploy tools, `<tool-key>.md` for
/// skill-wrapped and expert-plugin tools (the variant holds the would-be
/// agent markdown deployed inside the wrapper).
pub fn variant_filename(tool_key: &str, kind: &AgentDeployKind) -> String {
    let ext = match kind {
        AgentDeployKind::File { ext } => ext.as_str(),
        AgentDeployKind::SkillWrapped | AgentDeployKind::ExpertPlugin => "md",
    };
    format!("{tool_key}.{ext}")
}

/// What a deployment step should materialize for one (agent, tool) pair.
#[derive(Debug, Clone)]
pub enum DeploySource {
    /// Copy/symlink this existing file (a variant, or the canonical AGENT.md
    /// when the tool is md-native).
    File(PathBuf),
    /// Materialize this generated content as `SKILL.md` inside
    /// `<tool skills_dir>/<agent-id>/`. `hash` is the SHA-256 of `content`.
    SkillWrapped { content: String, hash: String },
}

/// Resolve what to deploy for (agent_dir, tool). Variant file wins when
/// present; the canonical AGENT.md is a fallback **only** for md-native
/// file tools (opencode). Anything else missing is an explicit error —
/// never an implicit conversion (design decision #2).
pub fn resolve_deploy_source(
    agent_dir: &Path,
    tool_key: &str,
    kind: &AgentDeployKind,
) -> Result<DeploySource> {
    let variant = agent_dir.join(variant_filename(tool_key, kind));
    if variant.is_file() {
        return Ok(match kind {
            AgentDeployKind::SkillWrapped => {
                let content = std::fs::read_to_string(&variant)
                    .with_context(|| format!("reading variant {}", variant.display()))?;
                let hash = content_sha256(content.as_bytes());
                DeploySource::SkillWrapped { content, hash }
            }
            // Expert plugins and plain file tools copy the variant verbatim.
            AgentDeployKind::File { .. } | AgentDeployKind::ExpertPlugin => {
                DeploySource::File(variant)
            }
        });
    }

    let canonical = agent_dir.join(CANONICAL_FILE_NAME);
    match kind {
        AgentDeployKind::File { ext } if ext == "md" => {
            if canonical.is_file() {
                Ok(DeploySource::File(canonical))
            } else {
                bail!(
                    "no variant {} and no {} in {}",
                    variant_filename(tool_key, kind),
                    CANONICAL_FILE_NAME,
                    agent_dir.display()
                )
            }
        }
        // Expert plugins take the canonical markdown verbatim as
        // agents/<id>.md (WorkBuddy agent definitions are plain markdown),
        // so the canonical file is a valid fallback here too.
        AgentDeployKind::ExpertPlugin => {
            if canonical.is_file() {
                Ok(DeploySource::File(canonical))
            } else {
                bail!(
                    "no variant {} and no {} in {}",
                    variant_filename(tool_key, kind),
                    CANONICAL_FILE_NAME,
                    agent_dir.display()
                )
            }
        }
        _ => bail!(
            "missing variant '{}' for tool '{}' — generate it explicitly (gen-variant) before syncing",
            variant_filename(tool_key, kind),
            tool_key
        ),
    }
}

/// Explicitly generate a variant for `tool_key` from the canonical markdown.
/// Pure function: input content in, variant content out; the caller archives
/// the result in the central agent dir.
pub fn generate_variant_for_tool(
    canonical_content: &str,
    tool_key: &str,
    kind: &AgentDeployKind,
) -> Result<String> {
    let parsed = parse_agent_markdown(canonical_content)
        .ok_or_else(|| anyhow!("canonical definition has no parseable frontmatter"))?;
    match kind {
        AgentDeployKind::File { ext } if ext == "md" => {
            // md-native tools deploy the canonical file verbatim; a variant
            // would be a byte-for-byte copy, so refuse to mint one.
            bail!("tool '{tool_key}' is md-native; it deploys AGENT.md directly — no variant needed")
        }
        AgentDeployKind::ExpertPlugin => {
            // Expert plugins also take the markdown verbatim (inside
            // agents/<id>.md); the plugin.json manifest is generated at
            // deploy time, not as a variant.
            bail!(
                "tool '{tool_key}' expert plugins embed AGENT.md directly — no variant needed"
            )
        }
        AgentDeployKind::File { .. } => Ok(generate_codex_toml(&parsed)),
        AgentDeployKind::SkillWrapped => Ok(generate_skill_md(&parsed)),
        #[allow(unreachable_patterns)]
        _ => bail!("unsupported deploy kind for tool '{tool_key}'"),
    }
}

/// Build a codex agents/*.toml document. Field mapping per spec 搂5.4:
/// `name`/`description` from frontmatter, body → `developer_instructions`,
/// `sandbox_mode` defaults to `read-only`. Frontmatter fields without a
/// TOML counterpart (e.g. permission blocks) are intentionally not mapped.
pub fn generate_codex_toml(parsed: &ParsedAgent) -> String {
    let mut root = toml::map::Map::new();
    if let Some(name) = &parsed.name {
        root.insert("name".into(), toml::Value::String(name.clone()));
    }
    if let Some(description) = &parsed.description {
        root.insert(
            "description".into(),
            toml::Value::String(description.clone()),
        );
    }
    root.insert(
        "sandbox_mode".into(),
        toml::Value::String(CODEX_DEFAULT_SANDBOX_MODE.to_string()),
    );
    root.insert(
        "developer_instructions".into(),
        toml::Value::String(parsed.body.clone()),
    );
    let doc = toml::Value::Table(root);
    format!(
        "# Generated by skills-manager from AGENT.md.\n\
         # sandbox_mode defaults to \"{CODEX_DEFAULT_SANDBOX_MODE}\" — edit freely.\n\
         # MCP server bindings ([mcp_servers.*]) are not derived from markdown; add them by hand.\n{}",
        toml::to_string_pretty(&doc).unwrap_or_default()
    )
}

/// Build the SKILL.md content used by skill-wrapped tools
/// (workbuddy / dsh): frontmatter name/description carried over, body
/// verbatim.
pub fn generate_skill_md(parsed: &ParsedAgent) -> String {
    let mut fm = String::from("---\n");
    if let Some(name) = &parsed.name {
        fm.push_str(&format!("name: {name}\n"));
    }
    match &parsed.description {
        Some(description) => fm.push_str(&format!("description: {description}\n")),
        None => fm.push_str("description: \n"),
    }
    fm.push_str("---\n\n");
    fm.push_str(&parsed.body);
    fm
}

/// Build the minimal `.codebuddy-plugin/plugin.json` expert manifest for a
/// WorkBuddy expert plugin. The result is written once at deploy time and
/// is hand-editable afterwards (displayName, avatar, tags, quickPrompts…).
pub fn generate_expert_plugin_json(name: &str, description: Option<&str>) -> Result<String> {
    let mut root = serde_json::Map::new();
    root.insert("name".into(), serde_json::Value::String(name.to_string()));
    root.insert(
        "version".into(),
        serde_json::Value::String("1.0.0".into()),
    );
    root.insert(
        "description".into(),
        serde_json::Value::String(
            description.unwrap_or(name).to_string(),
        ),
    );
    root.insert(
        "agents".into(),
        serde_json::Value::Array(vec![serde_json::Value::String(format!(
            "./agents/{name}.md"
        ))]),
    );
    root.insert(
        "expertType".into(),
        serde_json::Value::String("agent".into()),
    );
    root.insert(
        "agentName".into(),
        serde_json::Value::String(name.to_string()),
    );
    let value = serde_json::Value::Object(root);
    serde_json::to_string_pretty(&value)
        .map(|s| format!("{s}\n"))
        .context("serializing expert plugin.json")
}

/// Upsert helper: point a marketplace.json `plugins[]` entry at a plugin
/// directory. Creates the marketplace file when missing. Returns the new
/// file content.
pub fn upsert_expert_marketplace_entry(
    existing: Option<&str>,
    marketplace_name: &str,
    plugin_name: &str,
    description: Option<&str>,
) -> Result<String> {
    let mut root: serde_json::Value = match existing {
        Some(raw) => serde_json::from_str(raw)
            .with_context(|| "existing marketplace.json is not valid JSON")?,
        None => serde_json::json!({
            "name": marketplace_name,
            "description": format!("{marketplace_name} marketplace (managed by skills-manager)"),
            "plugins": []
        }),
    };
    let plugins = root
        .get_mut("plugins")
        .and_then(|p| p.as_array_mut())
        .ok_or_else(|| anyhow!("marketplace.json has no plugins[] array"))?;
    let entry = serde_json::json!({
        "name": plugin_name,
        "source": format!("./plugins/{plugin_name}"),
        "description": description.unwrap_or(plugin_name),
    });
    if let Some(slot) = plugins
        .iter_mut()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(plugin_name))
    {
        *slot = entry;
    } else {
        plugins.push(entry);
    }
    serde_json::to_string_pretty(&root)
        .map(|s| format!("{s}\n"))
        .context("serializing marketplace.json")
}

/// Remove a plugin entry from marketplace.json content. Returns `None`
/// when the content has no entry for `plugin_name`.
pub fn remove_expert_marketplace_entry(
    existing: &str,
    plugin_name: &str,
) -> Result<Option<String>> {
    let mut root: serde_json::Value = serde_json::from_str(existing)
        .with_context(|| "existing marketplace.json is not valid JSON")?;
    let Some(plugins) = root.get_mut("plugins").and_then(|p| p.as_array_mut()) else {
        return Ok(None);
    };
    let before = plugins.len();
    plugins.retain(|p| p.get("name").and_then(|n| n.as_str()) != Some(plugin_name));
    if plugins.len() == before {
        return Ok(None);
    }
    serde_json::to_string_pretty(&root)
        .map(|s| Some(format!("{s}\n")))
        .context("serializing marketplace.json")
}

/// Reverse conversion for import: parse a codex TOML definition and render
/// it as canonical markdown. The original TOML is archived separately as
/// the `codex.toml` variant by the importer.
pub fn codex_toml_to_canonical(toml_content: &str) -> Result<String> {
    let value: toml::Value =
        toml::from_str(toml_content).context("invalid codex agent TOML")?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());
    let instructions = value
        .get("developer_instructions")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let mut md = String::from("---\n");
    if let Some(name) = &name {
        md.push_str(&format!("name: {name}\n"));
    }
    if let Some(description) = &description {
        md.push_str(&format!("description: {description}\n"));
    }
    md.push_str("---\n\n");
    md.push_str(&instructions);
    Ok(md)
}

/// Reverse conversion for import: derive canonical markdown from a
/// SKILL.md (workbuddy / dsh). Frontmatter name/description carry over;
/// body verbatim.
pub fn skill_md_to_canonical(skill_md_content: &str) -> Result<String> {
    let parsed = parse_agent_markdown(skill_md_content)
        .ok_or_else(|| anyhow!("SKILL.md has no parseable frontmatter"))?;
    Ok(generate_skill_md(&parsed))
}

/// Determine the central agent directory name from a source file's stem.
/// Falls back to the frontmatter `name` when the stem is unsanitary.
pub fn agent_id_from_source(
    file_stem: &str,
    content: &str,
) -> Result<String> {
    if let Some(id) = super::skill_metadata::sanitize_skill_name(file_stem) {
        return Ok(id);
    }
    let parsed = parse_agent_markdown(content)
        .and_then(|p| p.name)
        .ok_or_else(|| anyhow!("cannot derive a safe agent id from '{file_stem}'"))?;
    super::skill_metadata::sanitize_skill_name(&parsed)
        .ok_or_else(|| anyhow!("frontmatter name is not a usable agent id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MD: &str = "---\nname: implementer\ndescription: 瀹炴柦浠ｇ爜淇敼\nmode: subagent\n---\n\nYou are an implementer.\n";

    // ── parse / validate ──

    #[test]
    fn parse_extracts_fields_and_body() {
        let parsed = parse_agent_markdown(SAMPLE_MD).unwrap();
        assert_eq!(parsed.name.as_deref(), Some("implementer"));
        assert_eq!(parsed.description.as_deref(), Some("瀹炴柦浠ｇ爜淇敼"));
        assert!(parsed.body.contains("You are an implementer."));
    }

    #[test]
    fn parse_rejects_missing_frontmatter() {
        assert!(parse_agent_markdown("# just docs\n").is_none());
    }

    #[test]
    fn validate_rejects_empty_body_without_prompt() {
        let err = validate_agent_markdown("---\nname: x\n---\n").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn validate_allows_empty_body_with_prompt_field() {
        // Empty body is fine when frontmatter carries `prompt`; no warnings
        // either because a description is present.
        let content = "---\nname: x\ndescription: y\nprompt: do things\n---\n";
        assert!(validate_agent_markdown(content).unwrap().is_none());
    }

    #[test]
    fn validate_warns_on_missing_description() {
        let content = "---\nname: x\n---\nbody here\n";
        let warning = validate_agent_markdown(content).unwrap().unwrap();
        assert!(warning.contains("description"));
    }

    // ── variant filename / resolution ──

    #[test]
    fn variant_filenames_follow_tool_kind() {
        assert_eq!(
            variant_filename("opencode", &AgentDeployKind::File { ext: "md".into() }),
            "opencode.md"
        );
        assert_eq!(
            variant_filename("codex", &AgentDeployKind::File { ext: "toml".into() }),
            "codex.toml"
        );
        assert_eq!(
            variant_filename("workbuddy", &AgentDeployKind::SkillWrapped),
            "workbuddy.md"
        );
        assert_eq!(
            variant_filename("workbuddy", &AgentDeployKind::ExpertPlugin),
            "workbuddy.md"
        );
    }

    #[test]
    fn resolve_expert_plugin_falls_back_to_canonical() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENT.md"), SAMPLE_MD).unwrap();
        assert!(resolve_deploy_source(tmp.path(), "workbuddy", &AgentDeployKind::ExpertPlugin)
            .is_ok());

        let empty = tempfile::tempdir().unwrap();
        assert!(resolve_deploy_source(empty.path(), "workbuddy", &AgentDeployKind::ExpertPlugin)
            .is_err());
    }

    #[test]
    fn generate_variant_refuses_expert_plugin_tool() {
        let err = generate_variant_for_tool(SAMPLE_MD, "workbuddy", &AgentDeployKind::ExpertPlugin)
            .unwrap_err();
        assert!(err.to_string().contains("expert plugins embed"));
    }

    #[test]
    fn expert_plugin_json_minimal_manifest() {
        let json = generate_expert_plugin_json("my-expert", Some("does expert things")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["name"], "my-expert");
        assert_eq!(value["agents"][0], "./agents/my-expert.md");
        assert_eq!(value["expertType"], "agent");
        assert_eq!(value["agentName"], "my-expert");
        assert_eq!(value["description"], "does expert things");
    }

    #[test]
    fn marketplace_upsert_and_remove_entry() {
        let upserted =
            upsert_expert_marketplace_entry(None, "my-experts", "alpha", Some("first")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&upserted).unwrap();
        assert_eq!(v["plugins"][0]["source"], "./plugins/alpha");

        // Upsert again updates in place instead of duplicating.
        let updated = upsert_expert_marketplace_entry(Some(&upserted), "my-experts", "alpha", None)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(v["plugins"].as_array().unwrap().len(), 1);

        // Add a second plugin, then remove the first.
        let two = upsert_expert_marketplace_entry(Some(&updated), "my-experts", "beta", None)
            .unwrap();
        let removed = remove_expert_marketplace_entry(&two, "alpha").unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert_eq!(v["plugins"].as_array().unwrap().len(), 1);
        assert_eq!(v["plugins"][0]["name"], "beta");

        // Removing a name that is absent reports no change.
        assert!(remove_expert_marketplace_entry(&two, "gamma").unwrap().is_none());
    }

    #[test]
    fn resolve_prefers_variant_over_canonical() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("opencode.md"), "variant").unwrap();
        std::fs::write(tmp.path().join("AGENT.md"), "canonical").unwrap();
        match resolve_deploy_source(
            tmp.path(),
            "opencode",
            &AgentDeployKind::File { ext: "md".into() },
        )
        .unwrap()
        {
            DeploySource::File(p) => assert!(p.ends_with("opencode.md")),
            _ => panic!("expected file source"),
        }
    }

    #[test]
    fn resolve_canonical_fallback_only_for_md_native() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENT.md"), SAMPLE_MD).unwrap();

        // md-native: falls back to canonical.
        assert!(resolve_deploy_source(
            tmp.path(),
            "opencode",
            &AgentDeployKind::File { ext: "md".into() }
        )
        .is_ok());

        // codex (toml) and skill-wrapped: explicit refusal, no implicit conversion.
        assert!(resolve_deploy_source(
            tmp.path(),
            "codex",
            &AgentDeployKind::File { ext: "toml".into() }
        )
        .is_err());
        assert!(resolve_deploy_source(tmp.path(), "workbuddy", &AgentDeployKind::SkillWrapped)
            .is_err());
    }

    // ── generators ──

    #[test]
    fn generate_codex_toml_maps_fields() {
        let toml_str = generate_variant_for_tool(
            SAMPLE_MD,
            "codex",
            &AgentDeployKind::File { ext: "toml".into() },
        )
        .unwrap();
        assert!(toml_str.contains("name = \"implementer\""));
        assert!(toml_str.contains("sandbox_mode = \"read-only\""));
        assert!(toml_str.contains("developer_instructions"));
        assert!(toml_str.contains("You are an implementer."));
        // Round-trip: our own parser must read it back.
        let canonical = codex_toml_to_canonical(&toml_str).unwrap();
        assert!(canonical.contains("name: implementer"));
    }

    #[test]
    fn generate_codex_toml_keeps_multiline_instructions_intact() {
        let md = "---\nname: m\n---\nline one\n\nline three\n";
        let toml_str = generate_variant_for_tool(
            md,
            "codex",
            &AgentDeployKind::File { ext: "toml".into() },
        )
        .unwrap();
        let canonical = codex_toml_to_canonical(&toml_str).unwrap();
        assert!(canonical.contains("line one"));
        assert!(canonical.contains("line three"));
    }

    #[test]
    fn generate_variant_refuses_md_native_tool() {
        let err = generate_variant_for_tool(
            SAMPLE_MD,
            "opencode",
            &AgentDeployKind::File { ext: "md".into() },
        )
        .unwrap_err();
        assert!(err.to_string().contains("md-native"));
    }

    #[test]
    fn generate_skill_md_carries_frontmatter_and_body() {
        let skill = generate_variant_for_tool(
            SAMPLE_MD,
            "workbuddy",
            &AgentDeployKind::SkillWrapped,
        )
        .unwrap();
        assert!(skill.starts_with("---\n"));
        assert!(skill.contains("name: implementer"));
        assert!(skill.contains("You are an implementer."));
        // And it must round-trip back through the SKILL.md importer.
        let canonical = skill_md_to_canonical(&skill).unwrap();
        assert!(canonical.contains("description: 瀹炴柦浠ｇ爜淇敼"));
    }

    // ── ids ──

    #[test]
    fn agent_id_sanitizes_stem() {
        assert_eq!(agent_id_from_source("my-agent", "").unwrap(), "my-agent");
        assert_eq!(agent_id_from_source("../evil", "").unwrap(), "evil");
    }
}
