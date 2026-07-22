use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::central_repo;

pub const DEFAULT_PROVIDERS_URL: &str =
    "https://demo.egova.com.cn/skill-server/memory-tool/llm-providers.json";
pub const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
pub const FETCH_TIMEOUT_SECS: u64 = 15;

const EMBEDDED_FALLBACK: &str = include_str!("providers_fallback.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistry {
    pub schema_version: u32,
    pub updated_at: String,
    #[serde(default)]
    pub next_review: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub providers: Vec<Provider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub display_name: String,
    pub vendor: String,
    pub endpoint: String,
    pub chat_completions_path: String,
    pub model: String,
    #[serde(default)]
    pub openai_compat: bool,
    pub tier: String,
    #[serde(default)]
    pub domestic: bool,
    #[serde(default)]
    pub requires_vpn: bool,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub signup_url: Option<String>,
    #[serde(default)]
    pub canary_env: Option<String>,
    #[serde(default)]
    pub last_verified: Option<String>,
    #[serde(default = "default_status")]
    pub status: String,
    pub recommended_rank: u32,
}

fn default_status() -> String {
    "unverified".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadSource {
    Cache,
    Remote,
    EmbeddedFallback,
}

impl LoadSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoadSource::Cache => "cache",
            LoadSource::Remote => "remote",
            LoadSource::EmbeddedFallback => "embedded_fallback",
        }
    }
}

pub struct LoadResult {
    pub registry: ProviderRegistry,
    pub source: LoadSource,
    pub cache_path: PathBuf,
    pub cached_at: Option<SystemTime>,
    pub fetch_error: Option<String>,
}

pub fn providers_url() -> String {
    std::env::var("SKILLS_MANAGER_MEMORY_PROVIDERS_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PROVIDERS_URL.to_string())
}

pub fn cache_file_path() -> PathBuf {
    central_repo::cache_dir().join("memory-providers.json")
}

/// Load the provider registry via the standard waterfall:
/// 1. If `force_refresh` is false and the on-disk cache is <24h old, return it.
/// 2. Otherwise fetch from the configured URL, write cache, return.
/// 3. If fetch fails, try any-age stale cache.
/// 4. If no cache at all, return embedded fallback (bundled with binary).
pub fn load(force_refresh: bool) -> Result<LoadResult> {
    let cache_path = cache_file_path();

    if !force_refresh {
        if let Some(res) = try_load_fresh_cache(&cache_path) {
            return Ok(res);
        }
    }

    match fetch_remote() {
        Ok(registry) => {
            let cached_at = write_cache(&cache_path, &registry).ok();
            Ok(LoadResult {
                registry,
                source: LoadSource::Remote,
                cache_path,
                cached_at,
                fetch_error: None,
            })
        }
        Err(err) => {
            if let Some(mut stale) = try_load_any_cache(&cache_path) {
                stale.fetch_error = Some(format!("{err:#}"));
                return Ok(stale);
            }
            let registry: ProviderRegistry = serde_json::from_str(EMBEDDED_FALLBACK)
                .context("embedded fallback providers JSON is invalid")?;
            Ok(LoadResult {
                registry,
                source: LoadSource::EmbeddedFallback,
                cache_path,
                cached_at: None,
                fetch_error: Some(format!("{err:#}")),
            })
        }
    }
}

fn try_load_fresh_cache(path: &PathBuf) -> Option<LoadResult> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;
    if age > Duration::from_secs(CACHE_TTL_SECS) {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    let registry: ProviderRegistry = serde_json::from_str(&raw).ok()?;
    Some(LoadResult {
        registry,
        source: LoadSource::Cache,
        cache_path: path.clone(),
        cached_at: Some(modified),
        fetch_error: None,
    })
}

fn try_load_any_cache(path: &PathBuf) -> Option<LoadResult> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok();
    let raw = fs::read_to_string(path).ok()?;
    let registry: ProviderRegistry = serde_json::from_str(&raw).ok()?;
    Some(LoadResult {
        registry,
        source: LoadSource::Cache,
        cache_path: path.clone(),
        cached_at: modified,
        fetch_error: None,
    })
}

fn fetch_remote() -> Result<ProviderRegistry> {
    let url = providers_url();
    let client = reqwest::blocking::Client::builder()
        .user_agent("skills-manager/memory")
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()
        .context("building HTTP client")?;
    let resp = client
        .get(&url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx response from {url}"))?;
    let text = resp.text().context("reading response body")?;
    let registry: ProviderRegistry =
        serde_json::from_str(&text).context("parsing llm-providers.json")?;
    Ok(registry)
}

fn write_cache(path: &PathBuf, registry: &ProviderRegistry) -> Result<SystemTime> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("creating cache dir")?;
    }
    let raw = serde_json::to_vec_pretty(registry).context("serializing registry")?;
    fs::write(path, &raw).context("writing cache")?;
    Ok(fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or_else(SystemTime::now))
}

/// Providers sorted ascending by `recommended_rank` (1 = try first).
pub fn ranked(registry: &ProviderRegistry) -> Vec<&Provider> {
    let mut v: Vec<&Provider> = registry.providers.iter().collect();
    v.sort_by_key(|p| p.recommended_rank);
    v
}
