# 交接文档：SkillHub 搜索源修复（skillhub.cn）

> 交接对象：codex。本文件自包含，无需上文对话上下文即可接手。
> 仓库：`D:\git\skills-manager`（Tauri 2 + React 19 + Rust）。HEAD = `e4a3f92`，未提交。

---

## 1. 问题（用户报告）

用户引入 `https://skillhub.cn/` 作为搜索源。在 skillhub.cn **网站**搜 `anysearch` 能搜到 `AnySearch` 这个 skill，但在 **Skills Manager 工具的「排行榜」(SkillHub) tab** 搜 `anysearch` 返回「未找到匹配的市场 Skill」。本质：搜什么都搜不到，不只是 anysearch。

## 2. 根因（已彻底定位，无需重新排查）

工具连的根本不是 skillhub.cn 的真实后端：

| | skillhub.cn 网站用的真实后端 | 修复前代码请求的 |
|---|---|---|
| 域名 | `https://api.skillhub.cn` | `https://www.skill-cn.com`（旧 `skillhub_api.rs` 写死） |
| 技能量 | **71,005 个**（含 AnySearch） | 只有 **51 个**（一个精选 Top-50 镜像） |
| 搜索 | 服务端 `keyword=` 真搜索 | 无搜索接口，拉前 N 条**本地过滤**（总共才 51 条） |
| 返回结构 | `{code,data:{skills:[…],total}}` | `{data:[…],total,totalPages}` |

AnySearch 在 7 万个里、不在那 51 个精选里，且 skill-cn.com 直接忽略 `keyword` 参数 → 永远搜不到。

## 3. 已验证的 skillhub.cn 真实 API 契约（用浏览器实抓 + curl 验证，可信）

Base：`https://api.skillhub.cn`

| 用途 | 请求 | 返回要点 |
|---|---|---|
| 列表/搜索 | `GET /api/skills?page=1&pageSize=N&sortBy=score&order=desc&keyword=<q>` | `{"code":0,"data":{"skills":[{name,slug,description,description_zh,downloads,version,ownerName,homepage,category,…}],"total":71005},"message":"success"}`；`keyword=anysearch` → total=1 命中 AnySearch ✅ |
| 详情 | `GET /api/v1/skills/{slug}` | `{"latestVersion":{"version":"1.0.2"},"owner":{"displayName","handle"},"skill":{"slug","displayName","summary","summary_zh","category","stats":{downloads,stars}}}` |
| 文件清单 | `GET /api/v1/skills/{slug}/files?version={v}` | `{"count":5,"files":[{"path","sha256","size"}],"version"}` |
| 单文件 | `GET /api/v1/skills/{slug}/file?path={p}&version={v}` | **302** → 跳对象存储 → 原始文件字节（reqwest 默认跟随重定向）。SKILL.md 返回 text/markdown |
| ⚠️ 打包下载 | **不存在**。`/download`、`/zip`、`/archive` GET/POST 全 405 | → 安装必须**逐文件抓取后在本地组装目录**再装 |

中文 UI 描述优先级：`description_zh` / `summary_zh` 非空优先，否则回退 `description` / `summary`。

## 4. 已完成的改动（2 个文件，`cargo check` 已过 exit 0）

`git status`：
```
 M src-tauri/src/commands/browse.rs
 M src-tauri/src/core/skillhub_api.rs
```

- **`src-tauri/src/core/skillhub_api.rs`** —— 整体重写：
  - 常量改 `SKILLHUB_BASE = "https://api.skillhub.cn"`（原 `SKILLCN_BASE`）
  - 新增按真实 JSON 结构的 `ListResp/DetailResp/FilesResp` 解析结构体
  - `search_skills()` 用服务端 `keyword` 真搜索（核心修复）；`list_trending()` 不带 keyword
  - `get_skill(slug)` 解析详情，拿到 `version`
  - 新增 `materialize_skill(slug, version, dest)`：列文件 → 逐个 `download_file` → 写入目录；含 `is_safe_relative()` 目录穿越防护
  - 删除原 `download_skill()`（zip 端点不存在）
  - **`SkillHubSkill` 结构体字段保持不变**，前端 `src/lib/tauri.ts` 的 `SkillHubSkill` 接口无需改动。注意：`id` 字段语义从「数字 id」变为 **slug**
- **`src-tauri/src/commands/browse.rs`** —— `install_skillhub_skill`：
  - 改为 `get_skill` 拿 version → `tempfile::tempdir()` → `api.materialize_skill(...)` → `installer::install_from_local(temp_dir.path(), Some(&name))`（installer 的 `PreparedSource` 已支持目录安装）

依赖均已在 `src-tauri/Cargo.toml`（`reqwest` blocking+json、`urlencoding`、`tempfile`），未引入新 crate。

## 5. 剩余任务（codex 按序执行）

1. **端到端 API 实测**（上次因 Bash classifier 临时不可用未跑完）。复刻代码实际请求，确认链路通：
   ```bash
   curl -s "https://api.skillhub.cn/api/skills?page=1&pageSize=60&sortBy=score&order=desc&keyword=anysearch"
   curl -s "https://api.skillhub.cn/api/v1/skills/anysearch"
   # 取 latestVersion.version 后：
   curl -s "https://api.skillhub.cn/api/v1/skills/anysearch/files?version=<v>"
   curl -sL "https://api.skillhub.cn/api/v1/skills/anysearch/file?path=SKILL.md&version=<v>"
   ```
   预期：第一条 total=1 含 AnySearch；文件清单 5 个文件；SKILL.md 200 且非空。
2. **重新编译验证**：`cd src-tauri && cargo check`（已知约 3 分钟，上次 exit 0）。建议补 `cargo clippy`。
3. **构建 Windows 安装包并自测安装**：`pwsh scripts/build-windows.ps1`（按 memory「三平台发布架构」，Win 在本机 MSVC 出 NSIS）。装新包后在「排行榜/SkillHub」tab 搜 `anysearch` 应出现，点安装应能逐文件落地到 `~/.claude/skills/anysearch`。
4. **（可选）发布 OTA**：聚合 `pwsh scripts/release-all.ps1` + `release.sh` 拼 `latest.json` scp 到 demo.egova.com.cn。注意 pubkey 已轮换 `924A5728A6A5EDD0`，签名密码在 `~/.tauri-signing-password`。是否发版需先问用户。
5. **提交**：用户确认后再 commit/push（仓库约定：用户要求才提交；当前在 `main`，建议先开分支）。提交信息结尾加：
   `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`

## 6. 已知关联问题（本次报告范围外，建议向用户确认是否一并修）

`src-tauri/src/core/skillssh_api.rs:46,87,93,95,140` —— **SkillSSH** 是「排行榜」里**另一个独立源**，仍指向 `https://www.skill-cn.com/api/skills?page=&size=` 拉全量后本地过滤，**犯的是同一个病**（51 条精选镜像）。本次只修了 SkillHub 源（用户明确说引入的是 skillhub.cn）。若用户希望 SkillSSH 也走真实后端，可参照本次 `skillhub_api.rs` 同样改造。

## 7. 验证标准（完成定义）

- [ ] `cargo check`/`clippy` 通过
- [ ] 端到端 curl 实测：keyword 搜索命中 AnySearch、逐文件下载 200
- [ ] 新 Windows 包安装后，SkillHub tab 搜 `anysearch` 能显示，且能成功安装到本地 skills 目录
- [ ] 用户确认后再 commit/push（除删除外可自动通过，发版/提交需告知用户）
