//! Registry Mirror Manager
//!
//! Manages multiple skill registry mirrors including:
//! - Official sources (skills.sh, GitHub)
//! - Domestic mirrors (Tencent, Alibaba, ByteDance)
//! - Enterprise private registries

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Registry mirror configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryMirror {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub base_url: String,
    pub api_endpoint: String,
    pub region: MirrorRegion,
    pub enabled: bool,
    pub priority: u32, // Lower = higher priority
    pub timeout_secs: u64,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MirrorRegion {
    Global,
    ChinaMainland,
    ChinaHongKong,
    Singapore,
    Europe,
    US,
}

impl MirrorRegion {
    pub fn as_str(&self) -> &'static str {
        match self {
            MirrorRegion::Global => "global",
            MirrorRegion::ChinaMainland => "cn-mainland",
            MirrorRegion::ChinaHongKong => "cn-hk",
            MirrorRegion::Singapore => "sg",
            MirrorRegion::Europe => "eu",
            MirrorRegion::US => "us",
        }
    }
}

impl Default for RegistryMirror {
    fn default() -> Self {
        Self {
            name: "official".to_string(),
            display_name: "Official".to_string(),
            description: "Official skill registry".to_string(),
            base_url: "https://skills.sh".to_string(),
            api_endpoint: "https://skills.sh/api".to_string(),
            region: MirrorRegion::Global,
            enabled: true,
            priority: 100,
            timeout_secs: 30,
            headers: HashMap::new(),
        }
    }
}

/// Mirror manager that handles multiple registries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorManager {
    pub mirrors: Vec<RegistryMirror>,
    pub selected_mirror: String,
    pub auto_select: bool, // Auto select best mirror based on latency
    pub fallback_enabled: bool,
}

impl Default for MirrorManager {
    fn default() -> Self {
        Self {
            mirrors: Self::default_mirrors(),
            selected_mirror: "tencent".to_string(), // Default to Tencent for China users
            auto_select: true,
            fallback_enabled: true,
        }
    }
}

impl MirrorManager {
    /// Get default set of mirrors including domestic options
    pub fn default_mirrors() -> Vec<RegistryMirror> {
        vec![
            // Tencent Cloud Mirror (China)
            RegistryMirror {
                name: "tencent".to_string(),
                display_name: "腾讯云镜像".to_string(),
                description: "腾讯云国内加速镜像".to_string(),
                base_url: "https://skills-mirror.tencentcloud.com".to_string(),
                api_endpoint: "https://skills-mirror.tencentcloud.com/api".to_string(),
                region: MirrorRegion::ChinaMainland,
                enabled: true,
                priority: 10,
                timeout_secs: 30,
                headers: {
                    let mut h = HashMap::new();
                    h.insert("X-Mirror-Source".to_string(), "tencent".to_string());
                    h
                },
            },
            // Alibaba Cloud Mirror (China)
            RegistryMirror {
                name: "aliyun".to_string(),
                display_name: "阿里云镜像".to_string(),
                description: "阿里云国内加速镜像".to_string(),
                base_url: "https://skills-mirror.aliyuncs.com".to_string(),
                api_endpoint: "https://skills-mirror.aliyuncs.com/api".to_string(),
                region: MirrorRegion::ChinaMainland,
                enabled: true,
                priority: 20,
                timeout_secs: 30,
                headers: {
                    let mut h = HashMap::new();
                    h.insert("X-Mirror-Source".to_string(), "aliyun".to_string());
                    h
                },
            },
            // ByteDance Mirror (China)
            RegistryMirror {
                name: "bytedance".to_string(),
                display_name: "字节镜像".to_string(),
                description: "字节跳动国内加速镜像".to_string(),
                base_url: "https://skills-mirror.bytedance.com".to_string(),
                api_endpoint: "https://skills-mirror.bytedance.com/api".to_string(),
                region: MirrorRegion::ChinaMainland,
                enabled: true,
                priority: 30,
                timeout_secs: 30,
                headers: {
                    let mut h = HashMap::new();
                    h.insert("X-Mirror-Source".to_string(), "bytedance".to_string());
                    h
                },
            },
            // Official Global (fallback)
            RegistryMirror {
                name: "official".to_string(),
                display_name: "官方源".to_string(),
                description: "官方全球源（可能较慢）".to_string(),
                base_url: "https://skills.sh".to_string(),
                api_endpoint: "https://skills.sh/api".to_string(),
                region: MirrorRegion::Global,
                enabled: true,
                priority: 100,
                timeout_secs: 60,
                headers: HashMap::new(),
            },
            // GitHub Mirror (for Git-based skills)
            RegistryMirror {
                name: "github".to_string(),
                display_name: "GitHub".to_string(),
                description: "GitHub 官方源".to_string(),
                base_url: "https://github.com".to_string(),
                api_endpoint: "https://api.github.com".to_string(),
                region: MirrorRegion::Global,
                enabled: true,
                priority: 90,
                timeout_secs: 60,
                headers: HashMap::new(),
            },
            // Gitee Mirror (China alternative to GitHub)
            RegistryMirror {
                name: "gitee".to_string(),
                display_name: "Gitee".to_string(),
                description: "Gitee 国内代码托管".to_string(),
                base_url: "https://gitee.com".to_string(),
                api_endpoint: "https://gitee.com/api/v5".to_string(),
                region: MirrorRegion::ChinaMainland,
                enabled: true,
                priority: 15,
                timeout_secs: 30,
                headers: HashMap::new(),
            },
        ]
    }

    /// Get currently selected mirror
    pub fn get_active_mirror(&self) -> Option<&RegistryMirror> {
        self.mirrors.iter().find(|m| m.name == self.selected_mirror && m.enabled)
    }

    /// Get mirror by name
    pub fn get_mirror(&self, name: &str) -> Option<&RegistryMirror> {
        self.mirrors.iter().find(|m| m.name == name)
    }

    /// Switch to a different mirror
    pub fn switch_mirror(&mut self, name: &str) -> Result<()> {
        if self.mirrors.iter().any(|m| m.name == name && m.enabled) {
            self.selected_mirror = name.to_string();
            Ok(())
        } else {
            Err(anyhow::anyhow!("Mirror '{}' not found or disabled", name))
        }
    }

    /// Enable/disable a mirror
    pub fn set_mirror_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        if let Some(mirror) = self.mirrors.iter_mut().find(|m| m.name == name) {
            mirror.enabled = enabled;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Mirror '{}' not found", name))
        }
    }

    /// Add a custom mirror
    pub fn add_mirror(&mut self, mirror: RegistryMirror) {
        // Remove existing mirror with same name if exists
        self.mirrors.retain(|m| m.name != mirror.name);
        self.mirrors.push(mirror);
        // Sort by priority
        self.mirrors.sort_by_key(|m| m.priority);
    }

    /// Remove a custom mirror
    pub fn remove_mirror(&mut self, name: &str) -> Result<()> {
        let is_builtin = ["tencent", "aliyun", "bytedance", "official", "github", "gitee"].contains(&name);
        if is_builtin {
            return Err(anyhow::anyhow!("Cannot remove built-in mirror '{}'", name));
        }
        self.mirrors.retain(|m| m.name != name);
        if self.selected_mirror == name {
            self.selected_mirror = "tencent".to_string();
        }
        Ok(())
    }

    /// Test mirror connectivity and return sorted list by latency
    pub async fn test_mirrors(&self) -> Vec<(String, Result<u64, String>)> {
        let mut results = Vec::new();

        for mirror in &self.mirrors {
            if !mirror.enabled {
                continue;
            }

            let start = std::time::Instant::now();
            let result = match test_mirror_connectivity(mirror).await {
                Ok(_) => Ok(start.elapsed().as_millis() as u64),
                Err(e) => Err(e.to_string()),
            };
            results.push((mirror.name.clone(), result));
        }

        // Sort by latency (successful first, then by time)
        results.sort_by(|a, b| {
            match (&a.1, &b.1) {
                (Ok(a_time), Ok(b_time)) => a_time.cmp(b_time),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                (Err(_), Err(_)) => std::cmp::Ordering::Equal,
            }
        });

        results
    }

    /// Get leaderboard URL for current mirror
    pub fn get_leaderboard_url(&self, board_type: &str) -> String {
        let mirror = self.get_active_mirror();
        match mirror {
            Some(m) => format!("{}/{}", m.base_url, board_type),
            None => format!("https://skills.sh/{}", board_type),
        }
    }

    /// Get search URL for current mirror
    pub fn get_search_url(&self, query: &str, limit: usize) -> String {
        let mirror = self.get_active_mirror();
        match mirror {
            Some(m) => format!("{}/search?q={}&limit={}", m.base_url, query, limit),
            None => format!("https://skills.sh/api/search?q={}&limit={}", query, limit),
        }
    }

    /// Get GitHub mirror URL (for Git operations)
    pub fn get_github_mirror_url(&self, original_url: &str) -> String {
        // If using domestic mirror, try to convert GitHub URL to mirror
        if self.selected_mirror == "gitee" && original_url.contains("github.com") {
            // Try to use Gitee mirror if the repo is mirrored there
            // Format: https://github.com/user/repo -> https://gitee.com/user/repo
            original_url.replace("github.com", "gitee.com")
        } else {
            original_url.to_string()
        }
    }
}

/// Test if a mirror is accessible
async fn test_mirror_connectivity(mirror: &RegistryMirror) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = client
        .head(&mirror.base_url)
        .send()
        .await
        .context(format!("Failed to connect to {}", mirror.name))?;

    if response.status().is_success() || response.status().as_u16() == 404 {
        // 404 is OK - means server is up but endpoint might be different
        Ok(())
    } else {
        Err(anyhow::anyhow!("Server returned status {}", response.status()))
    }
}

/// Convert skills.sh skill to mirror URL
pub fn convert_to_mirror_url(original_source: &str, mirror_name: &str) -> String {
    match mirror_name {
        "tencent" => original_source.replace("skills.sh", "skills-mirror.tencentcloud.com"),
        "aliyun" => original_source.replace("skills.sh", "skills-mirror.aliyuncs.com"),
        "bytedance" => original_source.replace("skills.sh", "skills-mirror.bytedance.com"),
        "gitee" => convert_github_to_gitee(original_source),
        _ => original_source.to_string(),
    }
}

/// Convert GitHub URL to Gitee equivalent
fn convert_github_to_gitee(github_url: &str) -> String {
    // Extract user/repo from GitHub URL
    if let Some(caps) = regex::Regex::new(r"github\.com/([^/]+)/([^/]+?)(?:\.git)?")
        .ok()
        .and_then(|re| re.captures(github_url)) {
        let user = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let repo = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        return format!("https://gitee.com/{}/{}.git", user, repo);
    }
    github_url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mirrors() {
        let manager = MirrorManager::default();
        assert!(!manager.mirrors.is_empty());
        assert!(manager.get_active_mirror().is_some());
    }

    #[test]
    fn test_switch_mirror() {
        let mut manager = MirrorManager::default();
        assert!(manager.switch_mirror("aliyun").is_ok());
        assert_eq!(manager.selected_mirror, "aliyun");
    }

    #[test]
    fn test_github_to_gitee() {
        let github = "https://github.com/user/repo.git";
        let gitee = convert_github_to_gitee(github);
        assert_eq!(gitee, "https://gitee.com/user/repo.git");
    }
}
