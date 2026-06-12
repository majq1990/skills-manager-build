use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// MCP Server metadata from MCP Registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub website_url: Option<String>,
    pub repository: Option<McpRepository>,
    pub remotes: Vec<McpRemote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRepository {
    pub url: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRemote {
    #[serde(rename = "type")]
    pub remote_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpServerWrapper {
    server: McpServer,
    #[serde(rename = "_meta")]
    meta: McpMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpMeta {
    #[serde(rename = "io.modelcontextprotocol.registry/official")]
    pub official: McpOfficialMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpOfficialMeta {
    pub status: String,
    pub is_latest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpServersResponse {
    pub servers: Vec<McpServerWrapper>,
}

/// MCP Registry API client
pub struct McpRegistryApi {
    client: reqwest::blocking::Client,
}

impl McpRegistryApi {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// List MCP servers from registry
    pub fn list_servers(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<McpServer>> {
        let url = format!(
            "https://registry.modelcontextprotocol.io/v0/servers?page={}&per_page={}",
            page, per_page
        );

        let response = self
            .client
            .get(&url)
            .send()
            .context("Failed to fetch MCP servers from registry")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!(
                "MCP Registry returned HTTP {}: {}",
                status,
                body
            ));
        }

        let resp_data: McpServersResponse = response
            .json()
            .context("Failed to parse MCP servers response")?;

        // Filter to only return latest versions
        let servers: Vec<McpServer> = resp_data
            .servers
            .into_iter()
            .filter(|wrapper| wrapper.meta.official.is_latest)
            .map(|wrapper| wrapper.server)
            .collect();

        Ok(servers)
    }

    /// Search MCP servers by keyword
    pub fn search_servers(&self, query: &str, limit: u32) -> Result<Vec<McpServer>> {
        // MCP Registry doesn't have a direct search endpoint, so we fetch and filter
        let servers = self.list_servers(1, limit.max(100))?;

        let query_lower = query.to_lowercase();
        let filtered: Vec<McpServer> = servers
            .into_iter()
            .filter(|server| {
                let name_match = server.name.to_lowercase().contains(&query_lower);
                let desc_match = server
                    .description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);
                name_match || desc_match
            })
            .collect();

        Ok(filtered)
    }
}

impl Default for McpRegistryApi {
    fn default() -> Self {
        Self::new()
    }
}
