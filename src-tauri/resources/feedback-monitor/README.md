# feedback-monitor

企业 skill 反馈监控插件。三通道：**R1 卡住**（连续同类工具 N 轮未解决）/ **R2 报错**（工具执行失败）/ **R3 手动**（触发词）。由 skills-manager 装机时随企业 skill 联动部署到各 agent，运行时把（脱敏后的）反馈 POST 到 `skill-server`。

> 状态：v0.1 脚手架。已实测可行的 **opencode**、**WorkBuddy** 两条适配 + 共享核心。Codex/OpenClaw/Hermes 适配、skills-manager 联动安装、服务端 schema 扩展待后续增量。

## 结构
```
core/            共享核心（agent 无关）
  config.mjs     常量：server url / 阈值 / 状态目录 / 凭据标识
  sanitize.mjs   脱敏（JWT/token/AWS/PEM/密码/连接串/URL key/手机/内网IP/家目录）+ 高熵打标
  tracker.mjs    轮次计数 + 卡住/报错启发式 + 防抖（24h/tool、错误1h、日限5）+ 文件锁
  submit.mjs     读 token(keyring) → 脱敏 → POST → 失败入离线队列 + flush 重试
adapters/
  _shared/cc-ingest.mjs          Claude-Code-兼容 hooks 共享入口（CC/WorkBuddy 复用）
  opencode/feedback-monitor.js   opencode JS 插件（tool.execute.before + event 总线）
  claude-code/ingest.mjs         CC 薄 wrapper → 共享入口（run('claude_code')）
  claude-code/settings.hooks.json 合并进 ~/.claude/settings.json 的 hooks
  workbuddy/ingest.mjs           WorkBuddy 薄 wrapper → 共享入口（run('workbuddy')）
  workbuddy/settings.hooks.json  合并进 ~/.workbuddy/settings.json 的 hooks
```

## Agent 覆盖
| Agent | 适配器 | 机制 | 验证 |
|---|---|---|---|
| opencode | ✅ | JS 插件 API | live |
| WorkBuddy | ✅ | CC-兼容 hooks（共享入口） | live |
| Claude Code | ✅ | CC 原生 hooks（共享入口） | 语法+冒烟（CC 是 CC-兼容基准） |
| Codex | 🔬 spike 中 | notify（待实测） | — |
| OpenClaw / Hermes | ⬜ 下一步 | 待 spike | — |

## 各 agent 机制（实测 2026-07-06）
- **opencode**（live 验证 v1.17.13）：JS 插件 API，**非** hooks.json。装 `~/.config/opencode/plugins/`（Windows 也是 `.config`）。R2 工具错误走 `event` 的 `message.part.updated`（`state.status=error`）；shell 非零退出码不算 error 需另 parse。
- **WorkBuddy = 腾讯 CodeBuddy 换皮**（live 验证 CLI v2.94.2）：Claude-Code-兼容 hooks，写 `~/.workbuddy/settings.json`；专用 `PostToolUseFailure` 事件。command hook 在 Windows 走 Git Bash。

> ⚠️ 关键：**没有"一份 hooks.json 通吃"**——每个 agent 各写适配层，只共享 core。

## 待办（下一增量）
- 服务端 `feedback.js` 扩展（`auto_*` 类型豁免必填 + trigger/conversation/error 字段 + **限流 N2**）+ 会话走文件 sink（R5）。
- skills-manager：装 skill 时联动下发本插件 + **写 JWT 到 per-agent token 文件**（`std::fs` + Windows `icacls` 限属主只读），与 submit.mjs 的 token 文件读取对齐。
  - token 投递决策（2026-07-06 spike）：**用 token 文件 + 属主 ACL，不用 OS keyring**。实测 Rust `keyring` v3 默认是进程内 mock（不持久化）、开 `windows-native` 踩依赖问题、且还需 Rust↔Node CredRead 互通——对零依赖跨 agent 插件不划算。keyring 留作后续硬化选项。
  - 已端到端验证（mock server）：无 token 入队 → 写 token 文件 flush → 带 Bearer POST 送达 + 密钥脱敏上链。
- Codex(notify)/OpenClaw/Hermes 适配 + Claude Code 参考实现。
- WorkBuddy `PostToolUseFailure` 的 live 造错验证（进行中）。

## 前置安全门禁（G0，开发不代表可上线）
- **N1**：skill-server 现网钉钉网关 key 泄露 —— 见 `D:\opencode\file\2026-07-06\N1-凭证泄露整改方案.md`。config 侧改造已在分支 `security/config-env-only`。
