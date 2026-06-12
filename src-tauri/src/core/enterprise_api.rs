use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseSkill {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(alias = "category", default)]
    pub visibility: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(alias = "download_count", default)]
    pub download_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub success: bool,
    pub token: String,
    pub user: ServerUserInfo,
    #[serde(default)]
    pub permissions: Option<serde_json::Value>,
    #[serde(rename = "expiresIn", default)]
    pub expires_in: Option<String>,
}

/// Server returns user with `id`, `firstname`, `lastname` (not `userId`/`isAdmin`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerUserInfo {
    pub id: u64,
    pub login: String,
    #[serde(default)]
    pub firstname: Option<String>,
    #[serde(default)]
    pub lastname: Option<String>,
    pub email: String,
}

/// Frontend-facing user info (mapped from ServerUserInfo)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(rename = "userId")]
    pub user_id: u64,
    pub login: String,
    pub email: String,
    #[serde(rename = "isAdmin")]
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendLoginResponse {
    pub success: bool,
    pub token: String,
    pub user: UserInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsResponse {
    pub success: bool,
    pub count: usize,
    pub skills: Vec<EnterpriseSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagsResponse {
    pub success: bool,
    pub count: usize,
    pub tags: Vec<String>,
}

pub struct EnterpriseApi {
    base_url: String,
    token: Option<String>,
}

impl EnterpriseApi {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: None,
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    pub fn get_token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    fn build_client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default()
    }

    pub fn login(&mut self, username: &str, password: &str) -> Result<LoginResponse> {
        let client = Self::build_client();
        let url = format!("{}/auth/login", self.base_url);

        let mut body = serde_json::Map::new();
        body.insert(
            "username".to_string(),
            serde_json::Value::String(username.to_string()),
        );
        body.insert(
            "password".to_string(),
            serde_json::Value::String(password.to_string()),
        );

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .context("Failed to connect to enterprise server")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Login failed ({}): {}", status, text);
        }

        let login_resp: LoginResponse = resp.json().context("Failed to parse login response")?;

        if login_resp.success {
            self.token = Some(login_resp.token.clone());
        }

        Ok(login_resp)
    }

    /// Map server LoginResponse to frontend-compatible format
    pub fn map_login_response(resp: LoginResponse) -> FrontendLoginResponse {
        let is_admin = resp
            .permissions
            .as_ref()
            .and_then(|p| p.get("isAdmin"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        FrontendLoginResponse {
            success: resp.success,
            token: resp.token,
            user: UserInfo {
                user_id: resp.user.id,
                login: resp.user.login,
                email: resp.user.email,
                is_admin,
            },
        }
    }

    fn auth_header(&self) -> Result<String> {
        let token = self.token.as_deref().context("Not authenticated")?;
        Ok(format!("Bearer {}", token))
    }

    pub fn list_skills(&self) -> Result<Vec<EnterpriseSkill>> {
        let client = Self::build_client();
        let url = format!("{}/skills", self.base_url);

        let resp = client
            .get(&url)
            .header("Authorization", self.auth_header()?)
            .send()
            .context("Failed to fetch skills")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Failed to list skills ({}): {}", status, text);
        }

        let skills_resp: SkillsResponse =
            resp.json().context("Failed to parse skills response")?;

        Ok(skills_resp.skills)
    }

    pub fn get_tags(&self) -> Result<Vec<String>> {
        let client = Self::build_client();
        let url = format!("{}/skills/tags", self.base_url);

        let resp = client
            .get(&url)
            .header("Authorization", self.auth_header()?)
            .send()
            .context("Failed to fetch tags")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Failed to get tags ({}): {}", status, text);
        }

        let tags_resp: TagsResponse = resp.json().context("Failed to parse tags response")?;

        Ok(tags_resp.tags)
    }

    pub fn search_by_tag(&self, tag: &str) -> Result<Vec<EnterpriseSkill>> {
        let client = Self::build_client();
        let url = format!(
            "{}/skills/search?tag={}",
            self.base_url,
            urlencoding::encode(tag)
        );

        let resp = client
            .get(&url)
            .header("Authorization", self.auth_header()?)
            .send()
            .context("Failed to search by tag")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Failed to search by tag ({}): {}", status, text);
        }

        let skills_resp: SkillsResponse =
            resp.json().context("Failed to parse search response")?;

        Ok(skills_resp.skills)
    }

    pub fn search_by_query(&self, query: &str) -> Result<Vec<EnterpriseSkill>> {
        let client = Self::build_client();
        let url = format!(
            "{}/skills/search?q={}",
            self.base_url,
            urlencoding::encode(query)
        );

        let resp = client
            .get(&url)
            .header("Authorization", self.auth_header()?)
            .send()
            .context("Failed to search skills")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Failed to search skills ({}): {}", status, text);
        }

        let skills_resp: SkillsResponse =
            resp.json().context("Failed to parse search response")?;

        Ok(skills_resp.skills)
    }

    /// 下载指定技能指定版本的 zip 包字节。
    /// name 可能含中文（如「定制图标生成器」），必须 URL 编码；服务端 express :name 自动 decode。
    /// 配合服务端 RFC 5987 Content-Disposition，中文名技能下载完整可用。
    pub fn download_skill(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        let client = Self::build_client();
        let url = format!(
            "{}/skills/{}/{}/download",
            self.base_url,
            urlencoding::encode(name),
            urlencoding::encode(version)
        );

        let resp = client
            .get(&url)
            .header("Authorization", self.auth_header()?)
            .send()
            .context("Failed to download skill")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Failed to download skill ({}): {}", status, text);
        }

        let bytes = resp.bytes().context("Failed to read skill package")?;
        Ok(bytes.to_vec())
    }
}
