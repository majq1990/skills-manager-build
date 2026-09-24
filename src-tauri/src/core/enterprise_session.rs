//! 企业账号登录态的持久化与恢复。
//!
//! token 原本只存在 `commands::enterprise::ENTERPRISE_API` 这个进程内静态变量里，
//! 进程一退出就丢，用户每次重开应用都要重新登录。这里把登录态加密落库
//! （`settings` 表 + `.secret.key`，见 `skill_store::SENSITIVE_KEYS` 与
//! `core::crypto`），启动时读回并本地校验 JWT 的 `exp`——过期就不恢复，
//! 不必联网就能判断，用户直接看到登录框而不是先撞一串 401。

use crate::core::skill_store::SkillStore;

const TOKEN_KEY: &str = "enterprise_token";
const USERNAME_KEY: &str = "enterprise_username";

/// 恢复出来的登录态。
pub struct Session {
    pub token: String,
    pub username: String,
}

/// 登录成功后调用：加密落库。
pub fn save(store: &SkillStore, username: &str, token: &str) {
    if let Err(e) = store.set_setting(USERNAME_KEY, username) {
        log::warn!("企业登录态: 保存用户名失败 {e}");
    }
    if let Err(e) = store.set_setting(TOKEN_KEY, token) {
        log::warn!("企业登录态: 保存 token 失败 {e}");
    }
}

/// 登出时调用：清掉落盘凭据，下次启动保持未登录。
pub fn clear(store: &SkillStore) {
    for key in [TOKEN_KEY, USERNAME_KEY] {
        if let Err(e) = store.remove_setting(key) {
            log::warn!("企业登录态: 清理 {key} 失败 {e}");
        }
    }
}

/// 启动时调用：读回 token。返回 `None` 表示没有可用登录态（从未登录 / 已登出 /
/// 已过期 / 密文解不开），调用方应保持未登录。
pub fn restore(store: &SkillStore) -> Option<Session> {
    let token = match store.get_setting(TOKEN_KEY) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        // `.secret.key` 被删或换过 → 旧密文解不开，按未登录处理并清掉残留，
        // 否则每次启动都会重复报同一个解密错误。
        Err(e) => {
            log::warn!("企业登录态: 读取 token 失败（密钥可能已变更），清除残留 {e}");
            let _ = store.remove_setting(TOKEN_KEY);
            let _ = store.remove_setting(USERNAME_KEY);
            return None;
        }
    };

    if token.trim().is_empty() {
        let _ = store.remove_setting(TOKEN_KEY);
        return None;
    }

    if is_expired(&token) {
        log::info!("企业登录态: 本地 token 已过期，清除并保持未登录");
        clear(store);
        return None;
    }

    let username = store
        .get_setting(USERNAME_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();
    Some(Session { token, username })
}

/// 本地解析 JWT 的 `exp` 判断是否过期。
///
/// 解析不出来（非 JWT、没有 `exp`、不是 JSON）时返回 `false` 即当作仍有效：
/// 宁可让服务端回 401 再处理，也不要因为解析失败把用户踢出登录态。
fn is_expired(token: &str) -> bool {
    let Some(payload) = decode_jwt_payload(token) else {
        return false;
    };
    let Ok(claims) = serde_json::from_str::<serde_json::Value>(&payload) else {
        return false;
    };
    let Some(exp) = claims.get("exp").and_then(|e| e.as_i64()) else {
        return false;
    };
    // 30 秒宽限，吸收客户端与服务端的时钟偏差。
    exp + 30 < chrono::Utc::now().timestamp()
}

fn decode_jwt_payload(token: &str) -> Option<String> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill_store::SkillStore;
    use tempfile::tempdir;

    fn jwt_with_exp(exp: i64) -> String {
        use base64::Engine;
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header = enc.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = enc.encode(format!(r#"{{"exp":{exp}}}"#));
        format!("{header}.{payload}.signature")
    }

    fn live_token(username: &str) -> String {
        use base64::Engine;
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let exp = chrono::Utc::now().timestamp() + 3600;
        let header = enc.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = enc.encode(format!(r#"{{"exp":{exp},"sub":"{username}"}}"#));
        format!("{header}.{payload}.signature")
    }

    #[test]
    fn expired_token_is_detected() {
        let past = chrono::Utc::now().timestamp() - 3600;
        assert!(is_expired(&jwt_with_exp(past)));
    }

    #[test]
    fn future_token_is_kept() {
        let future = chrono::Utc::now().timestamp() + 3600;
        assert!(!is_expired(&jwt_with_exp(future)));
    }

    #[test]
    fn unparsable_token_is_treated_as_valid() {
        // 解析不了就交给服务端判断，不在这里把用户踢出登录态。
        assert!(!is_expired("not-a-jwt"));
        assert!(!is_expired("a.b.c"));
        // 结构合法但没有 exp 字段
        use base64::Engine;
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let token = format!(
            "{}.{}.sig",
            enc.encode(br#"{"alg":"HS256"}"#),
            enc.encode(br#"{"sub":"majianquan"}"#)
        );
        assert!(!is_expired(&token));
    }

    /// 登录态落库再恢复的完整回路：token 加密存储（DB 里读不到明文），
    /// 过期不恢复，clear 之后恢复不到。
    #[test]
    fn save_restore_clear_round_trip() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        let token = live_token("majianquan");

        assert!(restore(&store).is_none());

        save(&store, "majianquan", &token);

        let session = restore(&store).expect("登录后应能恢复");
        assert_eq!(session.token, token);
        assert_eq!(session.username, "majianquan");

        // 落库的是密文，不是明文 JWT
        let raw = store
            .raw_setting_for_test(TOKEN_KEY)
            .expect("读取原始值")
            .expect("token 行应存在");
        assert!(!raw.contains(&token));
        assert!(raw.starts_with(crate::core::crypto::ENC_PREFIX));

        clear(&store);
        assert!(restore(&store).is_none());
    }

    #[test]
    fn expired_saved_token_is_not_restored() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        let expired = jwt_with_exp(chrono::Utc::now().timestamp() - 60);

        save(&store, "majianquan", &expired);
        assert!(restore(&store).is_none());
        // 过期清除后，落盘也不该再留着
        assert!(store.raw_setting_for_test(TOKEN_KEY).unwrap().is_none());
    }
}
