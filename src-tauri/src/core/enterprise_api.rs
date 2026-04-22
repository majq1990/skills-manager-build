use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Skill metadata from enterprise server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseSkill {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    #[serde(default)]
    pub category: Option<String>, // "global" or "support-dept"
    // visibility for frontend compatibility - maps from category
    #[serde(default)]
    pub visibility: Option<String>,
    // Whether this skill is installed locally
    #[serde(default)]
    pub installed: bool,
}

/// Skill detail with download URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseSkillDetail {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    // Allow additional fields for flexibility
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Enterprise API client for skill operations.
pub struct EnterpriseApi {
    client: Client,
}

impl EnterpriseApi {
    /// Create a new API client with async HTTP client.
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// List all available skills from enterprise server.
    pub async fn list_skills(
        &self,
        server_url: &str,
        token: &str,
    ) -> Result<Vec<EnterpriseSkill>> {
        let url = format!("{}/skills", server_url.trim_end_matches('/'));

        println!("[EnterpriseApi] Fetching skills from: {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context(format!("Failed to connect to enterprise server at {}", url))?;

        let status = response.status();
        println!("[EnterpriseApi] Response status: {}", status);

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Server returned HTTP {}: {}",
                status,
                body
            ));
        }

        // Parse JSON
        let text = response.text().await
            .context("Failed to read response body")?;

        println!("[EnterpriseApi] Response body: {}", text.chars().take(200).collect::<String>());

        // API returns: { "success": true, "count": N, "skills": [...] }
        #[derive(Deserialize)]
        struct SkillsResponse {
            success: bool,
            count: Option<i32>,
            skills: Vec<EnterpriseSkill>,
        }

        let resp_data: SkillsResponse = serde_json::from_str(&text)
            .context(format!("Failed to parse skills response. Body: {}", text.chars().take(500).collect::<String>()))?;

        if !resp_data.success {
            return Err(anyhow::anyhow!("Server returned success=false"));
        }

        // Map category to visibility for each skill
        let skills: Vec<EnterpriseSkill> = resp_data.skills.into_iter()
            .map(|mut skill| {
                skill.visibility = skill.category.clone();
                skill
            })
            .collect();

        Ok(skills)
    }

    /// Get skill details by name.
    pub async fn get_skill(
        &self,
        server_url: &str,
        token: &str,
        name: &str,
    ) -> Result<EnterpriseSkillDetail> {
        let url = format!("{}/skills/{}", server_url.trim_end_matches('/'), name);

        log::info!("[EnterpriseApi] Getting skill details from: {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to get skill details")?;

        let status = response.status();
        log::info!("[EnterpriseApi] Skill detail response status: {}", status);

        if !status.is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            log::error!("[EnterpriseApi] Failed to get skill: HTTP {} - {}", status, body);
            return Err(anyhow::anyhow!(
                "Failed to get skill: HTTP {} - {}",
                status,
                body
            ));
        }

        // Get raw response text for debugging
        let text = response.text().await
            .context("Failed to read response body")?;

        log::info!("[EnterpriseApi] Skill detail response: {}", text.chars().take(500).collect::<String>());

        // Try to parse as standard format first
        match serde_json::from_str::<EnterpriseSkillDetail>(&text) {
            Ok(skill) => {
                log::info!("[EnterpriseApi] Parsed skill detail successfully: {}", skill.name);
                Ok(skill)
            }
            Err(e) => {
                log::warn!("[EnterpriseApi] Failed to parse standard format: {}", e);

                // Try to parse as wrapped response { "skill": { ... } }
                #[derive(Deserialize)]
                    struct SkillWrapper {
                        #[serde(rename = "skill")]
                        skill: Option<EnterpriseSkillDetail>,
                        #[serde(rename = "data")]
                        data: Option<EnterpriseSkillDetail>,
                    }

                if let Ok(wrapper) = serde_json::from_str::<SkillWrapper>(&text) {
                    if let Some(skill) = wrapper.skill.or(wrapper.data) {
                        log::info!("[EnterpriseApi] Parsed skill from wrapper format");
                        return Ok(skill);
                    }
                }

                // Return original error with context
                Err(anyhow::anyhow!(
                    "Failed to parse skill detail: {}. Response preview: {}",
                    e,
                    text.chars().take(200).collect::<String>()
                ))
            }
        }
    }

    /// Download skill package (returns zip bytes).
    pub async fn download_skill(
        &self,
        server_url: &str,
        token: &str,
        name: &str,
    ) -> Result<Vec<u8>> {
        // First get skill details to find the latest version
        log::info!("[EnterpriseApi] Getting skill details for: {}", name);
        let skill_detail = self.get_skill(server_url, token, name).await?;

        // Use skill name from response if available, otherwise use the input name
        let skill_name = if skill_detail.name.is_empty() {
            name
        } else {
            &skill_detail.name
        };

        // Use version from response, or default to "latest"
        let version = if skill_detail.version.is_empty() {
            "latest"
        } else {
            &skill_detail.version
        };

        log::info!("[EnterpriseApi] Skill name: {}, version: {}", skill_name, version);

        let url = format!(
            "{}/skills/{}/{}/download",
            server_url.trim_end_matches('/'),
            skill_name,
            version
        );

        log::info!("[EnterpriseApi] Downloading skill from: {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to download skill")?;

        let status = response.status();
        log::info!("[EnterpriseApi] Download response status: {}", status);

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log::error!("[EnterpriseApi] Download failed: HTTP {} - {}", status, body);
            return Err(anyhow::anyhow!(
                "Failed to download skill: HTTP {} - {}",
                status,
                body
            ));
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read skill package")?;

        log::info!("[EnterpriseApi] Downloaded {} bytes", bytes.len());

        Ok(bytes.to_vec())
    }
}

// ── Upload / Scan / History types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub scan_status: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanData {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_scan_at: Option<String>,
    #[serde(default)]
    pub scan_report: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStatusResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub data: Option<ScanData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanTriggerResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub scan_result: Option<serde_json::Value>,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub uploaded_at: String,
}

// ── Upload / Scan / History methods ──

impl EnterpriseApi {
    pub async fn upload_skill(
        &self,
        server_url: &str,
        token: &str,
        name: &str,
        zip_bytes: Vec<u8>,
        version: Option<&str>,
        _category: Option<&str>,
    ) -> Result<UploadResponse> {
        let url = format!("{}/skills/{}/upload", server_url.trim_end_matches('/'), name);
        log::info!("[EnterpriseApi] Uploading skill to: {}", url);

        let file_part = reqwest::multipart::Part::bytes(zip_bytes)
            .file_name(format!("{}.zip", name))
            .mime_str("application/zip")?;

        let mut form = reqwest::multipart::Form::new().part("package", file_part);
        if let Some(v) = version.filter(|s| !s.is_empty()) {
            form = form.text("version", v.to_string());
        }

        let response = self
            .client
            .post(&url)
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .context("Failed to upload skill")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Upload failed: HTTP {} - {}", status, body));
        }

        let resp: UploadResponse = response.json().await.context("Failed to parse upload response")?;
        Ok(resp)
    }

    pub async fn get_scan_status(
        &self,
        server_url: &str,
        token: &str,
        name: &str,
        version: &str,
    ) -> Result<ScanStatusResponse> {
        let url = format!(
            "{}/skills/{}/{}/scan",
            server_url.trim_end_matches('/'),
            name,
            version
        );
        let response = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to get scan status")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Scan status failed: HTTP {} - {}", status, body));
        }

        let resp: ScanStatusResponse = response.json().await.context("Failed to parse scan status")?;
        Ok(resp)
    }

    pub async fn trigger_scan(
        &self,
        server_url: &str,
        token: &str,
        name: &str,
        version: &str,
    ) -> Result<ScanTriggerResponse> {
        let url = format!(
            "{}/skills/{}/{}/scan",
            server_url.trim_end_matches('/'),
            name,
            version
        );
        let response = self
            .client
            .post(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to trigger scan")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Trigger scan failed: HTTP {} - {}", status, body));
        }

        let resp: ScanTriggerResponse = response.json().await.context("Failed to parse scan trigger response")?;
        Ok(resp)
    }

    pub async fn get_upload_history(
        &self,
        server_url: &str,
        token: &str,
        name: &str,
    ) -> Result<Vec<VersionInfo>> {
        let url = format!(
            "{}/skills/{}/versions",
            server_url.trim_end_matches('/'),
            name
        );
        let response = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to get upload history")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Upload history failed: HTTP {} - {}", status, body));
        }

        #[derive(Deserialize)]
        struct VersionsResponse {
            #[serde(default)]
            data: Vec<VersionInfo>,
        }

        let resp: VersionsResponse = response.json().await.context("Failed to parse versions response")?;
        Ok(resp.data)
    }
}

impl Default for EnterpriseApi {
    fn default() -> Self {
        Self::new()
    }
}