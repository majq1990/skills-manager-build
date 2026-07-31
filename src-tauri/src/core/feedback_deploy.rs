//! feedback-monitor 插件的联动部署。
//!
//! 把插件 bundle（core + adapters）拷到安装目录，再对**已检测到的** agent 落配置：
//! - opencode：生成 `~/.config/opencode/plugins/feedback-monitor.js` 加载器（re-export 已装适配器）
//! - Claude Code / WorkBuddy：幂等把 hooks 合并进各自 `settings.json`
//!
//! 只覆盖已 live 验证的三个 agent；Codex（仅回合级 notify + 与现有 notify 冲突）、
//! OpenClaw/Hermes 后续。幂等：重复部署不产生重复 hook 条目。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

enum Method {
    /// opencode：往 plugins 目录（相对 home）写加载器 .js
    Opencode { plugins_rel: &'static str },
    /// CC-兼容：合并 hooks 进 settings.json（相对 home 的路径 + adapters 子目录名）
    CcSettings {
        settings_rel: &'static str,
        adapter: &'static str,
    },
}

struct AgentDeploy {
    key: &'static str,
    detect_rel: &'static str, // 相对 home 的探测目录，存在=该 agent 已安装
    method: Method,
}

fn agents() -> Vec<AgentDeploy> {
    vec![
        AgentDeploy {
            key: "opencode",
            detect_rel: ".config/opencode",
            method: Method::Opencode {
                plugins_rel: ".config/opencode/plugins",
            },
        },
        AgentDeploy {
            key: "claude_code",
            detect_rel: ".claude",
            method: Method::CcSettings {
                settings_rel: ".claude/settings.json",
                adapter: "claude-code",
            },
        },
        AgentDeploy {
            key: "workbuddy",
            detect_rel: ".workbuddy",
            method: Method::CcSettings {
                settings_rel: ".workbuddy/settings.json",
                adapter: "workbuddy",
            },
        },
    ]
}

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

/// import 用的 file:// URL（正斜杠）。
fn file_url(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

/// 递归拷贝，跳过 node_modules/.git/state/*.log。
fn copy_bundle(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let n = name.to_string_lossy();
        if n == "node_modules" || n == ".git" || n == "state" || n.ends_with(".log") {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            fs::create_dir_all(&to)?;
            copy_bundle(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 一个 agent 的 CC hooks 配置（与 adapters/*/settings.hooks.json 对齐），command 指向已装 ingest。
fn cc_hooks(install_dir: &Path, adapter: &str) -> Value {
    let ingest = install_dir
        .join("adapters")
        .join(adapter)
        .join("ingest.mjs");
    let cmd = format!("node \"{}\"", ingest.to_string_lossy());
    let one = |timeout: i64| json!([{ "matcher": "*", "hooks": [{ "type": "command", "timeout": timeout, "command": cmd }] }]);
    let one_nomatch = |timeout: i64| json!([{ "hooks": [{ "type": "command", "timeout": timeout, "command": cmd }] }]);
    json!({
        "PostToolUse": one(15),
        "PostToolUseFailure": one(15),
        "UserPromptSubmit": one_nomatch(15),
        "Stop": one_nomatch(10),
    })
}

/// 幂等合并 hooks：逐事件把我们的条目 append 进已有数组；若已存在相同 command 则跳过。
fn merge_hooks(settings: &mut Value, our: &Value) {
    if !settings.is_object() {
        *settings = json!({});
    }
    let obj = settings.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();

    for (event, arr) in our.as_object().unwrap() {
        let our_cmd = our_command(arr);
        let existing = hooks.entry(event.clone()).or_insert_with(|| json!([]));
        if !existing.is_array() {
            *existing = json!([]);
        }
        let list = existing.as_array_mut().unwrap();
        let dup = list.iter().any(|e| entry_command(e) == our_cmd);
        if !dup {
            for e in arr.as_array().unwrap() {
                list.push(e.clone());
            }
        }
    }
}

/// 取一组事件条目里第一个 hook 的 command（用于去重）。
fn our_command(arr: &Value) -> Option<String> {
    arr.as_array()?.first().and_then(entry_command)
}
fn entry_command(entry: &Value) -> Option<String> {
    entry
        .get("hooks")?
        .as_array()?
        .first()?
        .get("command")?
        .as_str()
        .map(|s| s.to_string())
}

/// 部署：把 bundle 拷到 install_dir，再对已检测到的 agent 落配置。返回已部署 agent key 列表。
pub fn deploy_from(
    bundle_src: &Path,
    home: &Path,
    install_dir: &Path,
) -> std::io::Result<Vec<String>> {
    fs::create_dir_all(install_dir)?;
    copy_bundle(bundle_src, install_dir)?;

    let mut deployed = Vec::new();
    for a in agents() {
        if !home.join(a.detect_rel).exists() {
            continue; // 该 agent 未安装，跳过
        }
        match a.method {
            Method::Opencode { plugins_rel } => {
                let dir = home.join(plugins_rel);
                fs::create_dir_all(&dir)?;
                let adapter = install_dir
                    .join("adapters")
                    .join("opencode")
                    .join("feedback-monitor.js");
                let loader = format!(
                    "// auto-generated by skills-manager — feedback-monitor loader\nexport * from \"{}\";\n",
                    file_url(&adapter)
                );
                fs::write(dir.join("feedback-monitor.js"), loader)?;
            }
            Method::CcSettings {
                settings_rel,
                adapter,
            } => {
                let path = home.join(settings_rel);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut settings: Value = fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_else(|| json!({}));
                merge_hooks(&mut settings, &cc_hooks(install_dir, adapter));
                fs::write(
                    &path,
                    serde_json::to_string_pretty(&settings).unwrap_or_default(),
                )?;
            }
        }
        deployed.push(a.key.to_string());
    }
    Ok(deployed)
}

/// 从 app 资源目录解析 feedback-monitor bundle 路径（tauri.conf resources 声明的位置）。
pub fn resource_bundle(resource_dir: &Path) -> PathBuf {
    resource_dir.join("resources").join("feedback-monitor")
}

/// 便捷入口：install_dir = ~/.feedback-monitor/plugin。
pub fn deploy(bundle_src: &Path) -> std::io::Result<Vec<String>> {
    let home = home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
    let install_dir = home.join(".feedback-monitor").join("plugin");
    deploy_from(bundle_src, &home, &install_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path, content: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn deploy_writes_loader_and_merges_hooks_idempotently() {
        let tmp = std::env::temp_dir().join(format!("fmdeploy_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let bundle = tmp.join("bundle");
        let home = tmp.join("home");
        let install = tmp.join("home/.feedback-monitor/plugin");

        // 假 bundle
        touch(&bundle.join("core/config.mjs"), "//core");
        touch(
            &bundle.join("adapters/opencode/feedback-monitor.js"),
            "export const FeedbackMonitor=async()=>({});",
        );
        touch(&bundle.join("adapters/claude-code/ingest.mjs"), "//cc");
        touch(&bundle.join("adapters/workbuddy/ingest.mjs"), "//wb");
        // node_modules 应被跳过
        touch(&bundle.join("node_modules/x/y.js"), "skip");

        // 只装了 opencode + claude_code（不装 workbuddy）
        fs::create_dir_all(home.join(".config/opencode")).unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        // 预置一个已有的用户 hooks + 其它设置，验证不被清掉
        touch(
            &home.join(".claude/settings.json"),
            r#"{"model":"x","hooks":{"PostToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        );

        let d1 = deploy_from(&bundle, &home, &install).unwrap();
        assert!(d1.contains(&"opencode".to_string()));
        assert!(d1.contains(&"claude_code".to_string()));
        assert!(!d1.contains(&"workbuddy".to_string())); // 未安装 → 跳过

        // node_modules 被跳过
        assert!(!install.join("node_modules").exists());
        // opencode 加载器
        let loader =
            fs::read_to_string(home.join(".config/opencode/plugins/feedback-monitor.js")).unwrap();
        assert!(loader.contains("export * from \"file://"));
        assert!(loader.contains("adapters/opencode/feedback-monitor.js"));
        // CC settings：保留原 model + 原 hook，且加了我们的
        let s: Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap())
                .unwrap();
        assert_eq!(s["model"], "x");
        let post = s["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post.len(), 2); // 原 echo hi + 我们的
        assert!(s["hooks"]["PostToolUseFailure"].is_array());

        // 幂等：再部署一次，PostToolUse 不应再增加
        deploy_from(&bundle, &home, &install).unwrap();
        let s2: Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap())
                .unwrap();
        assert_eq!(s2["hooks"]["PostToolUse"].as_array().unwrap().len(), 2);

        let _ = fs::remove_dir_all(&tmp);
    }
}
