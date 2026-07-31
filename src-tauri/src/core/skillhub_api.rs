use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

/// SkillHub.cn 真实后端（与展示站 skillhub.cn 同源；旧的 skill-cn.com 只是 51 条精选镜像，已弃用）
pub const SKILLHUB_BASE: &str = "https://api.skillhub.cn";

/// Skill from SkillHub.cn —— 字段与前端 `SkillHubSkill` 接口保持一致，勿改动
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillHubSkill {
    /// slug（skillhub.cn 用 slug 而非数字 id；安装/详情都以此为键）
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

// ── /api/skills 列表/搜索响应 ──

#[derive(Debug, Deserialize, Default)]
struct ListItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    description_zh: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, rename = "ownerName")]
    owner_name: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ListData {
    #[serde(default)]
    skills: Vec<ListItem>,
    #[serde(default)]
    #[allow(dead_code)]
    total: u64,
}

#[derive(Debug, Deserialize)]
struct ListResp {
    #[serde(default)]
    data: ListData,
}

// ── /api/v1/skills/{slug} 详情响应 ──

#[derive(Debug, Deserialize, Default)]
struct DetailVersion {
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize, Default)]
struct DetailOwner {
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
    #[serde(default)]
    handle: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct DetailSkill {
    #[serde(default)]
    slug: String,
    #[serde(default, rename = "displayName")]
    display_name: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    summary_zh: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct DetailResp {
    #[serde(default, rename = "latestVersion")]
    latest_version: Option<DetailVersion>,
    #[serde(default)]
    owner: Option<DetailOwner>,
    #[serde(default)]
    skill: DetailSkill,
}

// ── /api/v1/skills/{slug}/files 文件清单响应 ──

#[derive(Debug, Deserialize, Default)]
struct FileEntry {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize, Default)]
struct FilesResp {
    #[serde(default)]
    files: Vec<FileEntry>,
}

/// 中文 UI 优先取中文描述，空则回退英文
fn pick_desc(zh: Option<String>, en: Option<String>) -> Option<String> {
    zh.filter(|s| !s.trim().is_empty())
        .or(en)
        .filter(|s| !s.trim().is_empty())
}

fn map_list_item(it: ListItem) -> SkillHubSkill {
    let tags = it
        .category
        .filter(|c| !c.trim().is_empty())
        .map(|c| vec![c])
        .unwrap_or_default();
    SkillHubSkill {
        id: it.slug,
        name: it.name,
        description: pick_desc(it.description_zh, it.description),
        author: it.owner_name,
        version: it.version,
        tags,
        installs: it.downloads,
        source_url: it.homepage,
        download_url: None,
    }
}

/// 拒绝绝对路径 / `..` / 根路径分量，防目录穿越
fn is_safe_relative(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let p = Path::new(path);
    p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// SkillHub API client —— 指向 api.skillhub.cn 真实公共接口
pub struct SkillHubApi {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl SkillHubApi {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: SKILLHUB_BASE.to_string(),
        }
    }

    fn list(&self, keyword: Option<&str>, limit: usize) -> Result<Vec<SkillHubSkill>> {
        let url = format!("{}/api/skills", self.base_url);
        let page_size = limit.clamp(1, 100).to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("page", "1"),
            ("pageSize", &page_size),
            ("sortBy", "score"),
            ("order", "desc"),
        ];
        let kw = keyword.map(|k| k.trim()).filter(|k| !k.is_empty());
        if let Some(k) = kw {
            params.push(("keyword", k));
        }
        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .context("Failed to connect to api.skillhub.cn")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("HTTP {}: {}", status, body));
        }
        let resp: ListResp = response
            .json()
            .context("Failed to parse api.skillhub.cn list response")?;
        Ok(resp
            .data
            .skills
            .into_iter()
            .take(limit)
            .map(map_list_item)
            .collect())
    }

    /// Search skills —— 服务端 keyword 真搜索（全量 7 万+ 技能可检索）
    pub fn search_skills(&self, query: &str, limit: usize) -> Result<Vec<SkillHubSkill>> {
        self.list(Some(query), limit)
    }

    /// List trending skills —— 按 score 降序
    pub fn list_trending(&self, limit: usize) -> Result<Vec<SkillHubSkill>> {
        self.list(None, limit)
    }

    /// Get skill details by slug
    pub fn get_skill(&self, slug: &str) -> Result<SkillHubSkill> {
        let url = format!("{}/api/v1/skills/{}", self.base_url, slug);
        let response = self
            .client
            .get(&url)
            .send()
            .context("Failed to get skill from api.skillhub.cn")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("HTTP {}: {}", status, body));
        }
        let detail: DetailResp = response.json().context("Failed to parse skill detail")?;
        let version = detail
            .latest_version
            .map(|v| v.version)
            .filter(|v| !v.is_empty());
        let author = detail.owner.and_then(|o| o.display_name.or(o.handle));
        let tags = detail
            .skill
            .category
            .clone()
            .filter(|c| !c.trim().is_empty())
            .map(|c| vec![c])
            .unwrap_or_default();
        let slug_resolved = if detail.skill.slug.is_empty() {
            slug.to_string()
        } else {
            detail.skill.slug
        };
        Ok(SkillHubSkill {
            id: slug_resolved,
            name: detail.skill.display_name,
            description: pick_desc(detail.skill.summary_zh, detail.skill.summary),
            author,
            version,
            tags,
            installs: 0,
            source_url: Some(format!("https://skillhub.cn/skills/{}", slug)),
            download_url: None,
        })
    }

    /// 列出指定版本的所有文件相对路径
    fn list_files(&self, slug: &str, version: &str) -> Result<Vec<String>> {
        let url = format!("{}/api/v1/skills/{}/files", self.base_url, slug);
        let response = self
            .client
            .get(&url)
            .query(&[("version", version)])
            .send()
            .context("Failed to list skill files from api.skillhub.cn")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("HTTP {}: {}", status, body));
        }
        let resp: FilesResp = response.json().context("Failed to parse file list")?;
        Ok(resp.files.into_iter().map(|f| f.path).collect())
    }

    /// 下载单个文件内容（接口会 302 跳到对象存储，reqwest 默认跟随重定向）
    fn download_file(&self, slug: &str, version: &str, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}/api/v1/skills/{}/file", self.base_url, slug);
        let response = self
            .client
            .get(&url)
            .query(&[("path", path), ("version", version)])
            .send()
            .with_context(|| format!("Failed to download file '{}'", path))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!(
                "api.skillhub.cn returned HTTP {} for '{}': {}",
                status,
                path,
                body
            ));
        }
        let bytes = response
            .bytes()
            .with_context(|| format!("Failed to read file '{}'", path))?;
        Ok(bytes.to_vec())
    }

    /// 把指定版本所有文件物化到 `dest` 目录（skillhub.cn 无 zip 打包接口，需逐文件组装）
    pub fn materialize_skill(&self, slug: &str, version: &str, dest: &Path) -> Result<()> {
        let files = self.list_files(slug, version)?;
        if files.is_empty() {
            return Err(anyhow::anyhow!(
                "Skill '{}' (v{}) has no files",
                slug,
                version
            ));
        }
        for rel in &files {
            if !is_safe_relative(rel) {
                return Err(anyhow::anyhow!("Unsafe file path in package: '{}'", rel));
            }
            let bytes = self.download_file(slug, version, rel)?;
            let target = dest.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create dir {:?}", parent))?;
            }
            std::fs::write(&target, &bytes)
                .with_context(|| format!("Failed to write {:?}", target))?;
        }
        Ok(())
    }
}

impl Default for SkillHubApi {
    fn default() -> Self {
        Self::new()
    }
}
