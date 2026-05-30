use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::skill_store::SkillStore;

const MAX_IDLE_DAYS: i64 = 7;
const MAX_IDLE_MILLIS: i64 = MAX_IDLE_DAYS * 24 * 60 * 60 * 1000;

/// Request body for enterprise login API.
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// User information returned from enterprise login API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub department: String,
    #[serde(default)]
    pub is_support_dept: bool,
}

/// Response from enterprise login API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    pub user: UserInfo,
    // Flattened fields for frontend compatibility
    #[serde(rename = "username")]
    pub username: String,
    #[serde(rename = "department")]
    pub department: String,
    #[serde(rename = "is_support_dept")]
    pub is_support_dept: bool,
}

/// Stored authentication data (persisted in settings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuth {
    pub token: String,
    pub expires_at: i64,
    pub user: UserInfo,
    pub server_url: String,
    #[serde(default)]
    pub last_activity: Option<i64>,
}

/// Enterprise authentication client.
pub struct EnterpriseAuth {
    client: reqwest::Client,
}

impl EnterpriseAuth {
    /// Create a new authentication client with async HTTP client.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("skills-manager")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Perform login against enterprise server.
    /// Async method using reqwest async client.
    pub async fn login(
        &self,
        store: &Arc<SkillStore>,
        server_url: &str,
        username: &str,
        password: &str,
    ) -> Result<LoginResponse> {
        let base_url = if server_url.ends_with('/') {
            server_url.to_string()
        } else {
            format!("{}/", server_url)
        };

        let login_url = format!("{}auth/login", base_url);
        let request = LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };

        let response = self
            .client
            .post(&login_url)
            .json(&request)
            .send()
            .await
            .context("Failed to connect to enterprise server")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Login failed with status {}: {}",
                status,
                body
            ));
        }

        let server_response: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse login response")?;

        // Parse response according to actual API format
        // API returns: { "success": true, "token": "...", "user": {...}, "permissions": {...}, "expiresIn": "30d" }
        let token = server_response["token"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        // Parse expiresIn (e.g., "30d" -> timestamp)
        let expires_in = server_response["expiresIn"]
            .as_str()
            .unwrap_or("30d");
        let expires_at = if expires_in.ends_with("d") {
            let days: i64 = expires_in[..expires_in.len()-1].parse().unwrap_or(30);
            Utc::now().timestamp_millis() + days * 24 * 60 * 60 * 1000
        } else {
            Utc::now().timestamp_millis() + 30 * 24 * 60 * 60 * 1000 // default 30 days
        };

        // Parse user info - API uses "login" instead of "username"
        let user_obj = server_response["user"].as_object().cloned().unwrap_or_default();
        let permissions_obj = server_response["permissions"].as_object().cloned().unwrap_or_default();
        let groups = permissions_obj["groups"].as_array().cloned().unwrap_or_default();

        // Extract roles from groups
        let roles: Vec<String> = groups
            .iter()
            .filter_map(|g| g["name"].as_str().map(String::from))
            .collect();

        // Check if user has support-dept role
        let is_support_dept = roles.iter().any(|r| r.contains("支持部"));

        // Get department from lastname or groups
        let lastname = user_obj["lastname"].as_str().unwrap_or_default();
        let _firstname = user_obj["firstname"].as_str().unwrap_or_default();
        let department = if !lastname.is_empty() {
            lastname.to_string()
        } else {
            groups.first()
                .and_then(|g| g["name"].as_str())
                .unwrap_or("支持部")
                .to_string()
        };

        let user = UserInfo {
            id: user_obj["id"].as_i64().unwrap_or(0).to_string(),
            username: user_obj["login"].as_str().unwrap_or_default().to_string(),
            roles,
            department,
            is_support_dept,
        };

        // Build response with flattened fields for frontend
        let login_response = LoginResponse {
            token: token.clone(),
            expires_at,
            user: user.clone(),
            username: user.username.clone(),
            department: user.department.clone(),
            is_support_dept: user.is_support_dept,
        };

        let now = Utc::now().timestamp_millis();
        let stored_auth = StoredAuth {
            token: login_response.token.clone(),
            expires_at: login_response.expires_at,
            user: login_response.user.clone(),
            server_url: base_url.clone(),
            last_activity: Some(now),
        };

        Self::persist_auth(store, &stored_auth)?;
        store
            .set_setting("enterprise_server_url", &base_url)
            .context("Failed to store server URL")?;

        Ok(login_response)
    }

    /// Logout - clear stored authentication.
    pub fn logout(store: &Arc<SkillStore>) -> Result<()> {
        Self::clear_auth(store)
    }

    fn clear_auth(store: &Arc<SkillStore>) -> Result<()> {
        store
            .set_setting("enterprise_auth_token", "")
            .context("Failed to clear auth token")?;
        Ok(())
    }

    fn persist_auth(store: &Arc<SkillStore>, auth: &StoredAuth) -> Result<()> {
        let auth_json = serde_json::to_string(auth).context("Failed to serialize auth data")?;
        store
            .set_setting("enterprise_auth_token", &auth_json)
            .context("Failed to store auth token")?;
        Ok(())
    }

    /// Get stored authentication data.
    pub fn get_auth(store: &Arc<SkillStore>) -> Result<Option<StoredAuth>> {
        let auth_json = store
            .get_setting("enterprise_auth_token")
            .context("Failed to read auth token")?;

        match auth_json {
            None => Ok(None),
            Some(json) if json.is_empty() => Ok(None),
            Some(json) => {
                let auth: StoredAuth = serde_json::from_str(&json)
                    .context("Failed to parse stored auth")?;
                Ok(Some(auth))
            }
        }
    }

    /// Get stored auth and apply Tauri-side session rules:
    /// - token absolute expiry still follows the server-issued expires_at
    /// - if the app has been idle for more than 7 days, require re-login
    /// - any successful use updates last_activity to keep the session alive
    pub fn get_valid_auth(
        store: &Arc<SkillStore>,
        touch_activity: bool,
    ) -> Result<Option<StoredAuth>> {
        let Some(mut auth) = Self::get_auth(store)? else {
            return Ok(None);
        };

        let now = Utc::now().timestamp_millis();
        let last_activity = auth.last_activity.unwrap_or(now);
        let expired = auth.expires_at <= now;
        let idle_too_long = now.saturating_sub(last_activity) > MAX_IDLE_MILLIS;

        if expired || idle_too_long {
            Self::clear_auth(store)?;
            return Ok(None);
        }

        if touch_activity {
            auth.last_activity = Some(now);
            Self::persist_auth(store, &auth)?;
        }

        Ok(Some(auth))
    }

    /// Check if user is currently authenticated (token exists and not expired).
    pub fn is_authenticated(store: &Arc<SkillStore>) -> Result<bool> {
        Ok(Self::get_valid_auth(store, true)?.is_some())
    }

    /// Check if authenticated user has support-dept role.
    pub fn is_support_dept(store: &Arc<SkillStore>) -> Result<bool> {
        match Self::get_valid_auth(store, true)? {
            None => Ok(false),
            Some(auth_data) => {
                Ok(auth_data.user.is_support_dept || auth_data.user.roles.iter().any(|r| r.contains("支持部")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_store() -> Arc<SkillStore> {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("skills-manager.db");
        // Keep tempdir alive for the lifetime of the store in this test scope.
        let leaked = Box::leak(Box::new(tmp));
        let db_path = PathBuf::from(leaked.path().join("skills-manager.db"));
        Arc::new(SkillStore::new(&db_path).unwrap())
    }

    fn save_auth(store: &Arc<SkillStore>, auth: &StoredAuth) {
        let json = serde_json::to_string(auth).unwrap();
        store.set_setting("enterprise_auth_token", &json).unwrap();
    }

    fn sample_auth(last_activity: Option<i64>) -> StoredAuth {
        let now = Utc::now().timestamp_millis();
        StoredAuth {
            token: "token".into(),
            expires_at: now + 30 * 24 * 60 * 60 * 1000,
            user: UserInfo {
                id: "1".into(),
                username: "majq1".into(),
                roles: vec!["支持部".into()],
                department: "支持部".into(),
                is_support_dept: true,
            },
            server_url: "https://demo.egova.com.cn/skill-api/".into(),
            last_activity,
        }
    }

    #[test]
    fn valid_auth_updates_last_activity() {
        let store = make_store();
        let old = Utc::now().timestamp_millis() - 60_000;
        save_auth(&store, &sample_auth(Some(old)));

        let auth = EnterpriseAuth::get_valid_auth(&store, true).unwrap().unwrap();
        assert!(auth.last_activity.unwrap() >= old);
    }

    #[test]
    fn idle_auth_requires_relogin_after_seven_days() {
        let store = make_store();
        let stale = Utc::now().timestamp_millis() - (MAX_IDLE_MILLIS + 1_000);
        save_auth(&store, &sample_auth(Some(stale)));

        let auth = EnterpriseAuth::get_valid_auth(&store, true).unwrap();
        assert!(auth.is_none());
    }
}
