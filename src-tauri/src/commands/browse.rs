use std::sync::Arc;
use tauri::State;

use crate::core::{
    domestic_mcp_api::{DomesticMcpMarket, DomesticMcpServer, McpProvider},
    error::AppError,
    gitee_api::{gitee_repo_to_domestic_mcp, GiteeApi},
    mcp_registry_api::{McpRegistryApi, McpServer},
    skill_store::SkillStore,
    skillsmp_api,
    skillssh_api::{self, LeaderboardType, SkillsShSkill},
    skillhub_api::{SkillHubApi, SkillHubSkill},
};

const LEADERBOARD_CACHE_TTL: i64 = 300; // 5 minutes

// ── Existing SkillSSH and SkillsMP APIs ──

#[tauri::command]
pub async fn fetch_leaderboard(
    board: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<SkillsShSkill>, AppError> {
    let cache_key = format!("leaderboard_{}", board);

    // Check cache
    if let Ok(Some(cached)) = store.get_cache(&cache_key, LEADERBOARD_CACHE_TTL) {
        if let Ok(skills) = serde_json::from_str::<Vec<SkillsShSkill>>(&cached) {
            return Ok(skills);
        }
    }

    let proxy_url = store.proxy_url();
    let board_type = LeaderboardType::from_str(&board);
    let skills = tauri::async_runtime::spawn_blocking(move || {
        skillssh_api::fetch_leaderboard(board_type, proxy_url.as_deref()).map_err(AppError::network)
    })
    .await??;

    // Update cache
    if let Ok(json) = serde_json::to_string(&skills) {
        store.set_cache(&cache_key, &json).ok();
    }

    Ok(skills)
}

#[tauri::command]
pub async fn search_skillssh(
    query: String,
    limit: Option<usize>,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<SkillsShSkill>, AppError> {
    let proxy_url = store.proxy_url();
    let requested = limit.unwrap_or(60);
    let bounded = requested.clamp(1, 300);
    tauri::async_runtime::spawn_blocking(move || {
        skillssh_api::search_skills(&query, bounded, proxy_url.as_deref())
            .map_err(AppError::network)
    })
    .await?
}

#[tauri::command]
pub async fn search_skillsmp(
    query: String,
    ai: Option<bool>,
    page: Option<u32>,
    limit: Option<u32>,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<SkillsShSkill>, AppError> {
    let api_key = store
        .get_setting("skillsmp_api_key")
        .map_err(AppError::db)?
        .filter(|k| !k.is_empty())
        .ok_or_else(|| AppError::network(anyhow::anyhow!("SkillsMP API key not configured")))?;
    let proxy_url = store.proxy_url();
    let mode = if ai.unwrap_or(false) {
        skillsmp_api::SearchMode::Ai
    } else {
        skillsmp_api::SearchMode::Keyword
    };
    tauri::async_runtime::spawn_blocking(move || {
        skillsmp_api::search(&api_key, &query, mode, page, limit, proxy_url.as_deref())
            .map_err(AppError::network)
    })
    .await?
}

// ── MCP Registry (Global) ──

#[tauri::command]
pub async fn search_mcp_registry(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<McpServer>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = McpRegistryApi::new();
        api.search_servers(&query, limit.unwrap_or(20))
            .map_err(|e| AppError::network(e.to_string()))
    })
    .await?
}

#[tauri::command]
pub async fn list_mcp_registry(
    page: Option<u32>,
    per_page: Option<u32>,
) -> Result<Vec<McpServer>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = McpRegistryApi::new();
        api.list_servers(page.unwrap_or(1), per_page.unwrap_or(20))
            .map_err(|e| AppError::network(e.to_string()))
    })
    .await?
}

// ── Domestic MCP Market (China) ──

#[tauri::command]
pub async fn search_domestic_mcp(query: String) -> Result<Vec<DomesticMcpServer>, AppError> {
    Ok(DomesticMcpMarket::search(&query))
}

#[tauri::command]
pub async fn list_domestic_mcp(
    provider: Option<String>,
) -> Result<Vec<DomesticMcpServer>, AppError> {
    match provider {
        Some(p) => {
            let provider_enum = match p.as_str() {
                "aliyun" => McpProvider::Aliyun,
                "bytedance" => McpProvider::Bytedance,
                "tencent" => McpProvider::Tencent,
                "dingtalk" => McpProvider::Dingtalk,
                _ => return Ok(DomesticMcpMarket::list_all()),
            };
            Ok(DomesticMcpMarket::filter_by_provider(provider_enum))
        }
        None => Ok(DomesticMcpMarket::list_all()),
    }
}

#[tauri::command]
pub async fn list_domestic_mcp_providers() -> Result<Vec<serde_json::Value>, AppError> {
    let providers = vec![
        // 国内 5 大 MCP 市场（按访问量排序，默认显示 Top）
        serde_json::json!({ "id": "mcpso",      "name": "MCP.so",      "icon": McpProvider::McpSo.icon_url() }),
        serde_json::json!({ "id": "modelscope", "name": "魔搭社区",     "icon": McpProvider::ModelScope.icon_url() }),
        serde_json::json!({ "id": "baidu",      "name": "百度MCP广场",  "icon": McpProvider::BaiduMcp.icon_url() }),
        serde_json::json!({ "id": "higress",    "name": "Higress MCP", "icon": McpProvider::Higress.icon_url() }),
        serde_json::json!({ "id": "pulsemcp",   "name": "PulseMCP",    "icon": McpProvider::PulseMcp.icon_url() }),
        // 厂商云
        serde_json::json!({ "id": "aliyun",     "name": "阿里云",       "icon": McpProvider::Aliyun.icon_url() }),
        serde_json::json!({ "id": "bytedance",  "name": "字节跳动",     "icon": McpProvider::Bytedance.icon_url() }),
        serde_json::json!({ "id": "tencent",    "name": "腾讯云",       "icon": McpProvider::Tencent.icon_url() }),
        serde_json::json!({ "id": "dingtalk",   "name": "钉钉",         "icon": McpProvider::Dingtalk.icon_url() }),
        serde_json::json!({ "id": "gitee",      "name": "Gitee",       "icon": McpProvider::Gitee.icon_url() }),
    ];
    Ok(providers)
}

/// 跨市场搜索：在所有静态聚合数据中按关键词过滤
#[tauri::command]
pub async fn search_top_mcp(query: String, limit: Option<usize>) -> Result<Vec<DomesticMcpServer>, AppError> {
    let take = limit.unwrap_or(50);
    let q = query.trim().to_lowercase();
    let mut all = if q.is_empty() {
        DomesticMcpMarket::list_all()
    } else {
        DomesticMcpMarket::search(&q)
    };
    all.truncate(take);
    Ok(all)
}

// ── Gitee Search (China) ──

#[tauri::command]
pub async fn search_gitee_skills(query: String, limit: Option<u32>) -> Result<Vec<DomesticMcpServer>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = GiteeApi::new();
        let repos = if query.trim().is_empty() {
            api.search_skills(limit.unwrap_or(30))
        } else {
            api.search_repos(&query, limit.unwrap_or(30))
        };

        repos
            .map(|repos| repos.iter().map(|r| gitee_repo_to_domestic_mcp(r)).collect())
            .map_err(|e| AppError::network(e.to_string()))
    })
    .await?
}

#[tauri::command]
pub async fn fetch_gitee_trending(limit: Option<u32>) -> Result<Vec<DomesticMcpServer>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = GiteeApi::new();
        api.get_trending(limit.unwrap_or(20))
            .map(|repos| repos.iter().map(|r| gitee_repo_to_domestic_mcp(r)).collect())
            .map_err(|e| AppError::network(e.to_string()))
    })
    .await?
}

// ── Domestic MCP Install ──

#[derive(Debug, Clone, serde::Serialize)]
pub struct DomesticMcpInstallResult {
    pub server_name: String,
    pub written_targets: Vec<String>,
    pub skipped_targets: Vec<String>,
    pub config_snippet: String,
    pub server_key: String,
}

fn agent_config_candidates() -> Vec<(String, std::path::PathBuf)> {
    let mut out = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let p = std::path::PathBuf::from(appdata)
            .join("Claude")
            .join("claude_desktop_config.json");
        out.push(("Claude Desktop".to_string(), p));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = std::path::PathBuf::from(home);
        out.push(("Cursor".to_string(), home.join(".cursor").join("mcp.json")));
        out.push(("Claude Code".to_string(), home.join(".claude.json")));
        out.push(("WorkBuddy".to_string(), home.join(".workbuddy").join("mcp.json")));
    }
    out
}

fn merge_mcp_entry(
    path: &std::path::Path,
    server_key: &str,
    entry: &serde_json::Value,
) -> std::io::Result<()> {
    let mut root: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        if content.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        }
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        serde_json::json!({})
    };

    if !root.is_object() {
        root = serde_json::json!({});
    }
    let obj = root.as_object_mut().unwrap();
    let servers = obj
        .entry("mcpServers".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !servers.is_object() {
        *servers = serde_json::json!({});
    }
    servers
        .as_object_mut()
        .unwrap()
        .insert(server_key.to_string(), entry.clone());

    let pretty = serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, pretty)?;
    Ok(())
}

#[tauri::command]
pub async fn install_domestic_mcp_direct(
    server_id: String,
) -> Result<DomesticMcpInstallResult, AppError> {
    let server = DomesticMcpMarket::list_all()
        .into_iter()
        .find(|s| s.id == server_id)
        .ok_or_else(|| AppError::internal(format!("Unknown server id: {}", server_id)))?;

    let entry = serde_json::json!({ "url": server.url });
    let server_key = server.id.clone();
    let snippet_root = serde_json::json!({
        "mcpServers": { server_key.clone(): entry.clone() }
    });
    let config_snippet =
        serde_json::to_string_pretty(&snippet_root).unwrap_or_else(|_| "{}".to_string());

    let candidates = agent_config_candidates();
    let entry_clone = entry.clone();
    let key_clone = server_key.clone();
    let (written, skipped) = tauri::async_runtime::spawn_blocking(move || {
        let mut written = Vec::new();
        let mut skipped = Vec::new();
        for (label, path) in candidates {
            let should_write = path.exists()
                || label == "Claude Desktop"
                || label == "Cursor"
                || label == "Claude Code"
                || label == "WorkBuddy";
            if !should_write {
                skipped.push(label);
                continue;
            }
            match merge_mcp_entry(&path, &key_clone, &entry_clone) {
                Ok(_) => written.push(label),
                Err(e) => {
                    log::warn!("Failed to write MCP config to {:?}: {}", path, e);
                    skipped.push(label);
                }
            }
        }
        (written, skipped)
    })
    .await
    .map_err(|e| AppError::internal(format!("Task join failed: {}", e)))?;

    Ok(DomesticMcpInstallResult {
        server_name: server.name,
        written_targets: written,
        skipped_targets: skipped,
        config_snippet,
        server_key,
    })
}

#[tauri::command]
pub async fn install_domestic_mcp(
    server_id: String,
    provider: String,
) -> Result<(), AppError> {
    // For domestic MCP servers, we open the URL in browser for manual installation
    // since they require specific setup per provider
    // 通用规则：先尝试在静态聚合表中找到对应 server，直接打开它的 url；
    // 找不到时，再走旧的 provider 兜底。
    let url = if let Some(found) = DomesticMcpMarket::list_all()
        .into_iter()
        .find(|s| s.id == server_id)
    {
        found.url
    } else {
        match provider.as_str() {
            "aliyun" => format!("https://bailian.console.aliyun.com/?spm=skill-{}", server_id),
            "bytedance" => format!("https://console.volcengine.com/mcp/{}?ref=skills-manager", server_id),
            "tencent" => format!("https://console.cloud.tencent.com/mcp/{}?ref=skills-manager", server_id),
            "dingtalk" => format!("https://open.dingtalk.com/mcp/{}?ref=skills-manager", server_id),
            "mcpso" => "https://mcp.so/".to_string(),
            "modelscope" => "https://modelscope.cn/mcp".to_string(),
            "baidu" => "https://mcp.bce.baidu.com/".to_string(),
            "higress" => "https://mcp.higress.ai/".to_string(),
            "pulsemcp" => "https://www.pulsemcp.com/servers".to_string(),
            _ => return Err(AppError::internal(format!("Unknown provider: {}", provider))),
        }
    };

    // Open URL in default browser
    if let Err(e) = open::that(&url) {
        return Err(AppError::internal(format!("Failed to open browser: {}", e)));
    }

    Ok(())
}

// ── Registry MCP Install ──

#[tauri::command]
pub async fn install_registry_mcp(
    server_name: String,
) -> Result<String, AppError> {
    // For registry MCP servers, we return the installation URL/command
    // which can be used by the client (Claude/Cursor/etc.)
    let install_config = format!(
        r#"{{
  "mcpServers": {{
    "{}": {{
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-{}"]
    }}
  }}
}}"#,
        server_name, server_name
    );

    Ok(install_config)
}

#[tauri::command]
pub async fn search_skillhub(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SkillHubSkill>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = SkillHubApi::new();
        api.search_skills(&query, limit.unwrap_or(60))
            .map_err(|e| AppError::network(e.to_string()))
    })
    .await?
}

#[tauri::command]
pub async fn list_skillhub_trending(
    limit: Option<usize>,
) -> Result<Vec<SkillHubSkill>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = SkillHubApi::new();
        api.list_trending(limit.unwrap_or(30))
            .map_err(|e| AppError::network(e.to_string()))
    })
    .await?
}

#[tauri::command]
pub async fn get_skillhub_skill(
    skill_id: String,
) -> Result<SkillHubSkill, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = SkillHubApi::new();
        api.get_skill(&skill_id)
            .map_err(|e| AppError::network(e.to_string()))
    })
    .await?
}

#[tauri::command]
pub async fn install_skillhub_skill(
    skill_id: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<String, AppError> {
    use crate::commands::presets::sync_scenario_skills;
    use crate::commands::skills::{store_installed_skill_unlocked, InstallSourceMetadata};
    use crate::core::installer;

    let store = store.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let api = SkillHubApi::new();

        let skill_info = api
            .get_skill(&skill_id)
            .map_err(|e| AppError::network(format!("Failed to get skill info: {}", e)))?;

        let version = skill_info
            .version
            .clone()
            .ok_or_else(|| AppError::network("SkillHub skill has no resolvable version".to_string()))?;

        // skillhub.cn 无 zip 打包接口，逐文件物化到临时目录后按目录安装
        let temp_dir = tempfile::tempdir().map_err(AppError::io)?;
        api.materialize_skill(&skill_id, &version, temp_dir.path())
            .map_err(|e| AppError::network(format!("Download failed: {}", e)))?;

        let result = installer::install_from_local(temp_dir.path(), Some(&skill_info.name))
            .map_err(|e| AppError::internal(format!("Install failed: {}", e)))?;

        let metadata = InstallSourceMetadata {
            source_type: "skillhub".to_string(),
            source_ref: Some(format!("skillhub/{}", skill_id)),
            source_ref_resolved: skill_info.source_url.clone(),
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            update_status: "up_to_date".to_string(),
        };

        let active = store.get_active_scenario_id().ok().flatten();
        store_installed_skill_unlocked(&store, &result, &metadata, active.as_deref())?;

        if let Some(scenario_id) = active.as_deref() {
            sync_scenario_skills(&store, scenario_id)?;
        }

        Ok(result.name)
    })
    .await?
}
