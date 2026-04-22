use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsShSkill {
    pub id: String,
    pub skill_id: String,
    pub name: String,
    pub source: String,
    pub installs: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum LeaderboardType {
    AllTime,
    Trending,
    Hot,
}

impl LeaderboardType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "trending" => Self::Trending,
            "hot" => Self::Hot,
            _ => Self::AllTime,
        }
    }
}

pub fn build_http_client(proxy_url: Option<&str>, timeout_secs: u64) -> reqwest::blocking::Client {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent("skills-manager")
        .timeout(std::time::Duration::from_secs(timeout_secs));
    if let Some(proxy) = proxy_url.filter(|s| !s.is_empty()) {
        if let Ok(p) = reqwest::Proxy::all(proxy) {
            builder = builder.proxy(p);
        }
    }
    builder.build().unwrap_or_default()
}

pub fn fetch_leaderboard(
    board: LeaderboardType,
    proxy_url: Option<&str>,
) -> Result<Vec<SkillsShSkill>> {
    // 数据源切换：skills.sh 国内不可达，统一走 skill-cn.com 公共 API
    let items = fetch_skillcn_items(proxy_url, 60)?;
    let mut ranked: Vec<(u64, serde_json::Value)> = items
        .into_iter()
        .map(|item| {
            let metric: u64 = match board {
                LeaderboardType::AllTime => item
                    .get("download_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                LeaderboardType::Trending => item
                    .get("heat_score")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as u64,
                LeaderboardType::Hot => item
                    .get("practice_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
            };
            (metric, item)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    ranked.truncate(30);

    Ok(ranked
        .into_iter()
        .map(|(metric, item)| map_skillcn_to_skillssh(&item, metric))
        .collect())
}

fn fetch_skillcn_items(
    proxy_url: Option<&str>,
    max_items: usize,
) -> Result<Vec<serde_json::Value>> {
    let client = build_http_client(proxy_url, 15);
    let size: u32 = 50;
    let mut all: Vec<serde_json::Value> = Vec::new();
    let mut page: u32 = 1;
    loop {
        let url = format!(
            "https://www.skill-cn.com/api/skills?page={}&size={}",
            page, size
        );
        let resp: serde_json::Value = client
            .get(&url)
            .send()
            .context("Failed to fetch skill-cn.com")?
            .json()
            .context("Failed to parse skill-cn.com response")?;
        let batch = resp
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
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

fn map_skillcn_to_skillssh(item: &serde_json::Value, installs: u64) -> SkillsShSkill {
    let id_str = item
        .get("id")
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string())
        .unwrap_or_default();
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&id_str)
        .to_string();
    SkillsShSkill {
        id: format!("skillhub/{}", id_str),
        skill_id: id_str,
        name,
        source: "skillhub".to_string(),
        installs,
    }
}

pub fn search_skills(
    query: &str,
    limit: usize,
    proxy_url: Option<&str>,
) -> Result<Vec<SkillsShSkill>> {
    // 数据源切换：skills.sh 搜索无法访问，走 skill-cn.com 拉全量后本地过滤
    let items = fetch_skillcn_items(proxy_url, 500)?;
    let q = query.trim().to_lowercase();
    let filtered: Vec<SkillsShSkill> = items
        .into_iter()
        .filter(|item| {
            if q.is_empty() {
                return true;
            }
            let name_hit = item
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase().contains(&q))
                .unwrap_or(false);
            let desc_hit = item
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase().contains(&q))
                .unwrap_or(false);
            let tag_hit = item
                .get("tag")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase().contains(&q))
                .unwrap_or(false);
            name_hit || desc_hit || tag_hit
        })
        .take(limit)
        .map(|item| {
            let installs = item
                .get("download_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            map_skillcn_to_skillssh(&item, installs)
        })
        .collect();
    Ok(filtered)
}

