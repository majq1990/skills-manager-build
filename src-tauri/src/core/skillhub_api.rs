use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const SKILLCN_BASE: &str = "https://www.skill-cn.com";

/// Skill from SkillHub.cn (实际数据源：skill-cn.com 公共 API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillHubSkill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub tags: Vec<String>,
    pub installs: u64,
    pub source_url: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CnItem {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    heat_score: f64,
    #[serde(default)]
    practice_count: u64,
    #[serde(default)]
    repo_owner_name: Option<String>,
    #[serde(default)]
    supports_download_zip: bool,
}

#[derive(Debug, Deserialize)]
struct CnPage {
    #[serde(default)]
    data: Vec<CnItem>,
}

#[derive(Debug, Deserialize)]
struct CnDetail {
    data: CnItem,
}

fn map_item(item: CnItem) -> SkillHubSkill {
    let tags = item
        .tag
        .clone()
        .filter(|t| !t.is_empty())
        .map(|t| vec![t])
        .unwrap_or_default();
    SkillHubSkill {
        id: item.id.to_string(),
        name: item.name,
        description: item.description,
        author: item.repo_owner_name,
        version: None,
        tags,
        installs: item.download_count,
        source_url: item.source_url,
        download_url: if item.supports_download_zip {
            Some(format!("{}/api/skills/{}/download", SKILLCN_BASE, item.id))
        } else {
            None
        },
    }
}

/// SkillHub API client — 实际指向 skill-cn.com 的公共接口
pub struct SkillHubApi {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl SkillHubApi {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: SKILLCN_BASE.to_string(),
        }
    }

    fn fetch_page(&self, page: u32, size: u32) -> Result<Vec<CnItem>> {
        let url = format!("{}/api/skills?page={}&size={}", self.base_url, page, size);
        let response = self
            .client
            .get(&url)
            .send()
            .context("Failed to connect to skill-cn.com")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("HTTP {}: {}", status, body));
        }
        let page: CnPage = response
            .json()
            .context("Failed to parse skill-cn.com response")?;
        Ok(page.data)
    }

    fn fetch_upto(&self, max_items: usize) -> Result<Vec<CnItem>> {
        let size: u32 = 50;
        let mut all: Vec<CnItem> = Vec::new();
        let mut page: u32 = 1;
        loop {
            let batch = self.fetch_page(page, size)?;
            let got = batch.len();
            all.extend(batch);
            if got < size as usize || all.len() >= max_items {
                break;
            }
            page += 1;
            if page > 20 {
                break;
            }
        }
        all.truncate(max_items);
        Ok(all)
    }

    /// Search skills — 拉取列表后本地过滤（skill-cn.com 无搜索接口）
    pub fn search_skills(&self, query: &str, limit: usize) -> Result<Vec<SkillHubSkill>> {
        let items = self.fetch_upto(500)?;
        let q = query.trim().to_lowercase();
        let filtered: Vec<SkillHubSkill> = items
            .into_iter()
            .filter(|item| {
                if q.is_empty() {
                    return true;
                }
                item.name.to_lowercase().contains(&q)
                    || item
                        .description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&q))
                        .unwrap_or(false)
                    || item
                        .tag
                        .as_deref()
                        .map(|t| t.to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .take(limit)
            .map(map_item)
            .collect();
        Ok(filtered)
    }

    /// List trending skills — skill-cn.com 默认按 heat_score 降序
    pub fn list_trending(&self, limit: usize) -> Result<Vec<SkillHubSkill>> {
        let items = self.fetch_upto(limit.max(30))?;
        Ok(items.into_iter().take(limit).map(map_item).collect())
    }

    /// Get skill details by ID (skill-cn 的数字 id)
    pub fn get_skill(&self, skill_id: &str) -> Result<SkillHubSkill> {
        let url = format!("{}/api/skills/{}", self.base_url, skill_id);
        let response = self
            .client
            .get(&url)
            .send()
            .context("Failed to get skill from skill-cn.com")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("HTTP {}: {}", status, body));
        }
        let detail: CnDetail = response.json().context("Failed to parse skill detail")?;
        Ok(map_item(detail.data))
    }

    /// Download skill package (returns bytes)
    pub fn download_skill(&self, skill_id: &str) -> Result<Vec<u8>> {
        let url = format!("{}/api/skills/{}/download", self.base_url, skill_id);
        let response = self
            .client
            .get(&url)
            .send()
            .context("Failed to download skill from skill-cn.com")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!(
                "skill-cn.com returned HTTP {}: {}",
                status,
                body
            ));
        }
        let bytes = response.bytes().context("Failed to read skill package")?;
        Ok(bytes.to_vec())
    }
}

impl Default for SkillHubApi {
    fn default() -> Self {
        Self::new()
    }
}
