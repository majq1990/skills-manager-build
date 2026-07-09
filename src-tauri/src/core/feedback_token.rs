//! feedback-monitor 反馈插件的 token 投递。
//!
//! 企业登录成功后，把 JWT 写到各 agent 的 `~/.feedback-monitor/<agent>/token`，
//! 供部署到该 agent 的 feedback-monitor 插件（submit.mjs）读取上报鉴权。
//! 用 token 文件 + 属主 ACL（Windows icacls / unix 0600），不用 OS keyring
//! （2026-07-06 spike：keyring v3 默认进程内 mock 不持久化 + 跨语言互通成本高）。

use std::fs;
use std::path::{Path, PathBuf};

/// 需要投递 token 的 agent（与插件 FEEDBACK_AGENT / feedback-monitor 适配器目录对齐）。
/// 目前 = 已写适配器的三个；Codex 待 spike 确认后加，OpenClaw/Hermes 后续。
const AGENTS: &[&str] = &["opencode", "workbuddy", "claude_code"];

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn state_dir(agent: &str) -> Option<PathBuf> {
    home_dir().map(|h| h.join(".feedback-monitor").join(agent))
}

/// 把 token 写到所有目标 agent 的 token 文件（登录成功后调用）。失败只记日志、不影响登录。
pub fn write_token(token: &str) {
    if token.trim().is_empty() {
        return;
    }
    for agent in AGENTS {
        match state_dir(agent) {
            Some(dir) => {
                if let Err(e) = write_one(&dir, token) {
                    log::warn!("feedback-monitor token 写入失败 ({agent}): {e}");
                }
            }
            None => log::warn!("feedback-monitor: 无法解析主目录，跳过 {agent}"),
        }
    }
}

fn write_one(dir: &Path, token: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join("token");
    fs::write(&path, token.as_bytes())?;
    restrict(&path);
    Ok(())
}

/// 限属主可读，去掉继承的其他 ACL。
fn restrict(path: &Path) {
    #[cfg(windows)]
    {
        if let Some(user) = std::env::var_os("USERNAME") {
            let user = user.to_string_lossy().to_string();
            let _ = std::process::Command::new("icacls")
                .arg(path)
                .arg("/inheritance:r")
                .arg("/grant:r")
                .arg(format!("{user}:R"))
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

/// 写企业 skill 名单到 ~/.feedback-monitor/enterprise-skills.json，
/// 供插件判定"当前会话是否在跑企业 skill"（只对名单内 skill 上报自动反馈）。
pub fn write_enterprise_list(skills: &[String]) {
    let Some(home) = home_dir() else { return };
    let dir = home.join(".feedback-monitor");
    if let Err(e) = fs::create_dir_all(&dir) {
        log::warn!("feedback-monitor: 建目录失败 {e}");
        return;
    }
    match serde_json::to_string(skills) {
        Ok(json) => {
            if let Err(e) = fs::write(dir.join("enterprise-skills.json"), json) {
                log::warn!("feedback-monitor: 写企业名单失败 {e}");
            }
        }
        Err(e) => log::warn!("feedback-monitor: 序列化企业名单失败 {e}"),
    }
}

/// 登出/退出时清除各 agent 的 token 文件。
pub fn clear_token() {
    for agent in AGENTS {
        if let Some(dir) = state_dir(agent) {
            let _ = fs::remove_file(dir.join("token"));
        }
    }
}
