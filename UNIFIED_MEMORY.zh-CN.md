# 跨 Agent 统一记忆

Skills Manager 使用 `~/.agent-memory/` 作为个人记忆的唯一事实源。目标是让同一位用户在 Claude Code、Codex、OpenCode、WorkBuddy、Cursor 等 Agent 中写入的记忆汇总到一起，并能被其他 Agent 按需调用。

## 工作方式

1. 每次同步前扫描各 Agent 的原生 `memory` 目录。
2. 按内容 SHA-256 精确去重；同名内容冲突时采用修改时间较新的版本。
3. 替换旧版本前，将旧文件备份到 `~/.agent-memory/.skills-manager/backups/`。
4. 把统一仓中的每条 Markdown 记忆物化为 `memory-*` skill，分发给所有已安装 Agent。
5. 给每个 Agent 安装 `memory-unified-bridge`，指导 Agent 直接向统一仓写入；受沙箱限制时可写自己的原生目录，随后由 Skills Manager 汇入。
6. GUI 启动时立即同步一次，运行期间每 60 秒自动对账一次。

## GUI

侧边栏打开“统一记忆”，可以查看统一仓路径、记忆数量、来源目录和 Agent 分发状态，执行汇总同步、新建记忆和搜索统一记忆清单。

## CLI

```powershell
skills-manager-cli memory status
skills-manager-cli memory migrate --dry-run
skills-manager-cli memory migrate
skills-manager-cli memory sync
skills-manager-cli memory remember --title "标题" --description "召回说明" --content "Markdown 内容" --memory-type reference --source-agent codex
```

`memory status` 和 `migrate --dry-run` 不写文件。`memory sync` 会先汇总原生目录，再分发记忆副本。

## 安全与冲突

- 统一仓是源，Agent skills 目录中的 `memory-*` 是副本，不应直接修改。
- 同内容不同文件名会精确去重。
- 同名不同内容按 `mtime` 取新版本，旧版本会先备份，不直接丢失。
- 每次正式对账记录在 `~/.agent-memory/.skills-manager/reconcile.jsonl`。
- 不自动把凭证写入记忆；只有用户明确要求时才应保存敏感信息。
