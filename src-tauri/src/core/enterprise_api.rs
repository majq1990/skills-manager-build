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
    /// 当前用户可上传的可见性级别（"public"/"tech-manager"/"support" 的子集，可能为空 = 无上传权限）。
    #[serde(rename = "uploadVisibilities", default)]
    pub upload_visibilities: Vec<String>,
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

/// 上传/发布技能后服务端返回的扁平化结果（供前端展示）。
/// 服务端实际把版本/状态包在 `skill` 子对象里，由 upload_skill 解析后映射到这里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub status: String,
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

        let upload_visibilities = resp
            .permissions
            .as_ref()
            .and_then(|p| p.get("uploadVisibilities"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        FrontendLoginResponse {
            success: resp.success,
            token: resp.token,
            user: UserInfo {
                user_id: resp.user.id,
                login: resp.user.login,
                email: resp.user.email,
                is_admin,
            },
            upload_visibilities,
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

    /// 上传/发布本地技能 zip 到企业服务器并触发安全扫描。
    /// zip 根目录必须直接含 SKILL.md（不带顶层目录），且 SKILL.md 里的 name 须等于 `name`。
    /// 字段名固定 `package`；可选 `version` text 字段（留空则服务端自动递增）。
    /// 同步函数（reqwest::blocking），必须在 spawn_blocking 内调用。
    /// 非 2xx 时把服务端 message/error 文本带出来（尤其安全扫描失败 HTTP 400）。
    pub fn upload_skill(
        &self,
        name: &str,
        zip_bytes: Vec<u8>,
        version: Option<&str>,
        visibility: Option<&str>,
    ) -> Result<UploadResponse> {
        use reqwest::blocking::multipart::{Form, Part};

        let client = Self::build_client();
        let url = format!(
            "{}/skills/{}/upload",
            self.base_url,
            urlencoding::encode(name)
        );

        let part = Part::bytes(zip_bytes)
            .file_name("skill.zip")
            .mime_str("application/zip")
            .context("Failed to build upload part")?;
        let mut form = Form::new().part("package", part);
        if let Some(v) = version {
            if !v.trim().is_empty() {
                form = form.text("version", v.to_string());
            }
        }
        if let Some(vis) = visibility {
            if !vis.trim().is_empty() {
                form = form.text("visibility", vis.to_string());
            }
        }

        let resp = client
            .post(&url)
            .header("Authorization", self.auth_header()?)
            .multipart(form)
            .send()
            .context("Failed to upload skill")?;

        let status = resp.status();
        let text = resp.text().unwrap_or_default();

        if !status.is_success() {
            // 服务端 JSON 形如 {success:false, error:"Security scan failed", message:"...", skill:{status:"unsafe"...}}
            // 尽量提取人类可读信息（error/message），让用户看清安全扫描失败原因。
            let detail = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .map(|v| {
                    let error = v.get("error").and_then(|x| x.as_str()).unwrap_or("");
                    let message = v.get("message").and_then(|x| x.as_str()).unwrap_or("");
                    match (error.is_empty(), message.is_empty()) {
                        (false, false) => format!("{}: {}", error, message),
                        (false, true) => error.to_string(),
                        (true, false) => message.to_string(),
                        (true, true) => String::new(),
                    }
                })
                .filter(|s| !s.is_empty())
                .unwrap_or(text);
            anyhow::bail!("Upload failed ({}): {}", status, detail);
        }

        // 成功：{success:true, message, skill:{name,version,status,...}, uploadedBy}
        let body: serde_json::Value =
            serde_json::from_str(&text).context("Failed to parse upload response")?;
        let skill = body.get("skill");
        Ok(UploadResponse {
            success: body
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            version: skill
                .and_then(|s| s.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            message: body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: skill
                .and_then(|s| s.get("status"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    /// 提交问题/建议反馈到企业服务器（服务端写钉钉电子表格一行）。
    /// type/skill/title/description；提交人由服务端从登录 JWT 自动带，无需前端传。
    pub fn submit_feedback(
        &self,
        feedback_type: &str,
        skill: &str,
        title: &str,
        description: &str,
    ) -> Result<()> {
        let client = Self::build_client();
        // base_url 已含 /skill-api（nginx 映射到 /api/v1/），只加 /feedback；
        // 加 /api/v1 会变成 /skill-api/api/v1/feedback → 404。
        let url = format!("{}/feedback", self.base_url);

        let mut body = serde_json::Map::new();
        body.insert(
            "type".to_string(),
            serde_json::Value::String(feedback_type.to_string()),
        );
        body.insert(
            "skill".to_string(),
            serde_json::Value::String(skill.to_string()),
        );
        body.insert(
            "title".to_string(),
            serde_json::Value::String(title.to_string()),
        );
        body.insert(
            "description".to_string(),
            serde_json::Value::String(description.to_string()),
        );

        let resp = client
            .post(&url)
            .header("Authorization", self.auth_header()?)
            .json(&body)
            .send()
            .context("Failed to submit feedback")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Failed to submit feedback ({}): {}", status, text);
        }

        Ok(())
    }
}
