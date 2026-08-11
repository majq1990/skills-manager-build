use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmMcpPackage {
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub homepage: Option<String>,
    pub repository_url: Option<String>,
    pub npm_url: String,
    pub weekly_downloads: Option<u64>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmSearchObject>,
}

#[derive(Debug, Clone, Deserialize)]
struct NpmSearchObject {
    package: NpmSearchPackage,
    downloads: Option<NpmDownloads>,
}

#[derive(Debug, Clone, Deserialize)]
struct NpmDownloads {
    weekly: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct NpmSearchPackage {
    name: String,
    version: String,
    description: Option<String>,
    keywords: Option<Vec<String>>,
    links: Option<NpmLinks>,
}

#[derive(Debug, Clone, Deserialize)]
struct NpmLinks {
    npm: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NpmPackageMetadata {
    name: String,
    version: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    keywords: Option<Vec<String>>,
    repository: Option<NpmRepository>,
    #[serde(rename = "dist-tags")]
    dist_tags: Option<NpmDistTags>,
}

#[derive(Debug, Clone, Deserialize)]
struct NpmDistTags {
    latest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum NpmRepository {
    Url(String),
    Object { url: Option<String> },
}

impl NpmRepository {
    fn url(&self) -> Option<String> {
        match self {
            Self::Url(url) => Some(url.clone()),
            Self::Object { url } => url.clone(),
        }
    }
}

pub struct NpmMcpApi {
    client: reqwest::blocking::Client,
}

impl NpmMcpApi {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub fn search_packages(&self, query: &str, limit: usize) -> Result<Vec<NpmMcpPackage>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let mut packages = Vec::new();
        let mut seen = HashSet::new();

        if is_likely_package_name(query) {
            if let Ok(pkg) = self.get_package(query) {
                seen.insert(pkg.name.clone());
                packages.push(pkg);
            }
        }

        let search_text = if query.to_ascii_lowercase().contains("mcp") {
            query.to_string()
        } else {
            format!("keywords:mcp {query}")
        };
        let size = limit.clamp(1, 100).to_string();
        let response = self
            .client
            .get("https://registry.npmjs.org/-/v1/search")
            .query(&[("text", search_text.as_str()), ("size", size.as_str())])
            .send()
            .context("Failed to search npm MCP packages")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!(
                "npm registry returned HTTP {status}: {body}"
            ));
        }

        let search: NpmSearchResponse = response
            .json()
            .context("Failed to parse npm search response")?;
        for item in search.objects {
            let package = item.into_package();
            if !looks_like_mcp_package(&package) {
                continue;
            }
            if seen.insert(package.name.clone()) {
                packages.push(package);
            }
            if packages.len() >= limit {
                break;
            }
        }

        Ok(packages)
    }

    pub fn get_package(&self, package_name: &str) -> Result<NpmMcpPackage> {
        let metadata = self
            .fetch_package_metadata("https://registry.npmjs.org", package_name)
            .or_else(|_| {
                self.fetch_package_metadata("https://registry.npmmirror.com", package_name)
            })?;
        Ok(metadata.into_package())
    }

    fn fetch_package_metadata(
        &self,
        registry_base: &str,
        package_name: &str,
    ) -> Result<NpmPackageMetadata> {
        let url = format!("{}/{}", registry_base, encode_package_name(package_name));
        let response = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to fetch npm package metadata: {package_name}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!(
                "npm registry returned HTTP {status} for {package_name}: {body}"
            ));
        }

        response
            .json()
            .with_context(|| format!("Failed to parse npm package metadata: {package_name}"))
    }
}

impl Default for NpmMcpApi {
    fn default() -> Self {
        Self::new()
    }
}

impl NpmSearchObject {
    fn into_package(self) -> NpmMcpPackage {
        let links = self.package.links;
        NpmMcpPackage {
            npm_url: links
                .as_ref()
                .and_then(|l| l.npm.clone())
                .unwrap_or_else(|| npm_package_url(&self.package.name)),
            homepage: links.as_ref().and_then(|l| l.homepage.clone()),
            repository_url: links.as_ref().and_then(|l| l.repository.clone()),
            name: self.package.name,
            description: self.package.description,
            version: self.package.version,
            weekly_downloads: self.downloads.and_then(|d| d.weekly),
            keywords: self.package.keywords.unwrap_or_default(),
        }
    }
}

impl NpmPackageMetadata {
    fn into_package(self) -> NpmMcpPackage {
        let version = self
            .version
            .or_else(|| self.dist_tags.and_then(|tags| tags.latest))
            .unwrap_or_default();
        NpmMcpPackage {
            npm_url: npm_package_url(&self.name),
            repository_url: self.repository.and_then(|r| r.url()),
            name: self.name,
            description: self.description,
            version,
            homepage: self.homepage,
            weekly_downloads: None,
            keywords: self.keywords.unwrap_or_default(),
        }
    }
}

fn looks_like_mcp_package(package: &NpmMcpPackage) -> bool {
    let haystack = format!(
        "{} {} {}",
        package.name,
        package.description.as_deref().unwrap_or_default(),
        package.keywords.join(" ")
    )
    .to_ascii_lowercase();
    haystack.contains("mcp") || haystack.contains("model context protocol")
}

pub fn package_server_key(package_name: &str) -> String {
    package_name
        .trim()
        .trim_start_matches('@')
        .replace('/', "-")
        .replace('@', "")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn npm_package_url(package_name: &str) -> String {
    format!("https://www.npmjs.com/package/{package_name}")
}

fn encode_package_name(package_name: &str) -> String {
    package_name.replace('@', "%40").replace('/', "%2F")
}

fn is_likely_package_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && !value.contains(char::is_whitespace)
        && (value.starts_with('@') || value.contains('/') || value.contains('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_server_key_normalizes_scoped_packages() {
        assert_eq!(package_server_key("@ntruth/dbhub"), "ntruth-dbhub");
        assert_eq!(
            package_server_key("@modelcontextprotocol/server-filesystem"),
            "modelcontextprotocol-server-filesystem"
        );
    }

    #[test]
    fn encode_package_name_handles_scopes() {
        assert_eq!(encode_package_name("@ntruth/dbhub"), "%40ntruth%2Fdbhub");
    }
}
