use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Gitee repository info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiteeRepo {
    pub id: i64,
    pub full_name: String,
    pub name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub stargazers_count: i32,
    pub forks_count: i32,
    pub language: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub owner: GiteeOwner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiteeOwner {
    pub login: String,
    pub avatar_url: String,
}

/// Gitee API client for searching skill repositories
pub struct GiteeApi {
    client: reqwest::blocking::Client,
}

impl GiteeApi {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Search repositories on Gitee
    pub fn search_repos(&self, query: &str, per_page: u32) -> Result<Vec<GiteeRepo>> {
        // Gitee search API endpoint
        let url = format!(
            "https://gitee.com/api/v5/search/repositories?q={}&page=1&per_page={}",
            urlencoding::encode(query),
            per_page.min(100)
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .context(format!("Failed to search Gitee for '{}'", query))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Gitee API returned HTTP {}: {}",
                status,
                body
            ));
        }

        // Gitee returns array directly, not object with items
        let repos: Vec<GiteeRepo> = response
            .json()
            .context("Failed to parse Gitee search response")?;

        Ok(repos)
    }

    /// Search for skill-related repositories
    pub fn search_skills(&self, per_page: u32) -> Result<Vec<GiteeRepo>> {
        // Search for various skill-related keywords
        let keywords = [
            "claude-skill",
            "claude skill",
            "ai-skill",
            "mcp-server",
            "mcp server",
        ];

        let mut all_repos = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for keyword in keywords {
            match self.search_repos(keyword, per_page / keywords.len() as u32 + 5) {
                Ok(repos) => {
                    for repo in repos {
                        if seen_ids.insert(repo.id) {
                            all_repos.push(repo);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to search Gitee for '{}': {}", keyword, e);
                }
            }
        }

        // Sort by stars (descending)
        all_repos.sort_by(|a, b| b.stargazers_count.cmp(&a.stargazers_count));

        Ok(all_repos)
    }

    /// Get trending repositories (most starred recently)
    pub fn get_trending(&self, per_page: u32) -> Result<Vec<GiteeRepo>> {
        // Search for AI/LLM related repos with recent updates
        let query = "AI OR LLM OR 大模型 OR claude OR openai language:markdown";
        self.search_repos(query, per_page)
    }
}

impl Default for GiteeApi {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert Gitee repo to domestic MCP server format
pub fn gitee_repo_to_domestic_mcp(repo: &GiteeRepo) -> super::domestic_mcp_api::DomesticMcpServer {
    super::domestic_mcp_api::DomesticMcpServer {
        id: format!("gitee-{}", repo.id),
        name: repo.name.clone(),
        description: repo
            .description
            .clone()
            .unwrap_or_else(|| "No description".to_string()),
        provider: super::domestic_mcp_api::McpProvider::Gitee,
        url: repo.html_url.clone(),
        category: repo.language.clone().unwrap_or_else(|| "Skill".to_string()),
        installs: repo.stargazers_count as u64,
    }
}
