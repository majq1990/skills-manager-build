// 反馈监控共享常量。可被环境变量覆盖（skills-manager 安装时可写入 agent 的环境）。
import { homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { readFileSync, readdirSync, statSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';

// base_url 已含 /skill-api，nginx 把 /skill-api/ 映射到 8091 的 /api/v1/，
// 所以这里只加 /feedback（不能再加 /api/v1，否则双前缀 404）。
export const SERVER_URL = process.env.FEEDBACK_SERVER_URL || 'https://demo.egova.com.cn/skill-api';
export const FEEDBACK_ENDPOINT = SERVER_URL.replace(/\/$/, '') + '/feedback';

// 卡住阈值：连续同工具 + 消极权重 >= 该值（v3 决策：先保守 10）
export const STUCK_ROUNDS = Number(process.env.FEEDBACK_STUCK_ROUNDS || 3);
export const STUCK_DEBOUNCE_MS = 24 * 3600 * 1000; // 同 tool 24h 至多 1 次
export const ERROR_DEBOUNCE_MS = 3600 * 1000;      // 同 tool+错误 1h 至多 1 次
export const DAILY_CAP = Number(process.env.FEEDBACK_DAILY_CAP || 5); // 全局日限（客户端侧；服务端仍须限流）

// 每 agent 独立状态目录，避免串扰
export const AGENT = process.env.FEEDBACK_AGENT || 'unknown';
export const STATE_DIR = process.env.FEEDBACK_STATE_DIR || join(homedir(), '.feedback-monitor', AGENT);

// 企业 skill 名单（skills-manager 登录时写入 ~/.feedback-monitor/enterprise-skills.json，存的是服务端 slug）。
// 自动反馈只在"当前会话正在跑名单内的企业 skill"时上报，避免对普通工具（read/playwright 等）误报。
//
// 难点：各 agent 的 Skill 工具传的是**本地 SKILL.md 的 name**（很多是中文显示名，或去重漂移的 `xxx-2`），
// 与名单里的服务端 slug 对不上 → 识别不到 skill。这里在客户端自建「显示名/目录名 → 名单 slug」映射来兜住：
//   1) 扫技能根目录（目录名=slug，frontmatter name=显示名）建 name→dir 映射，带缓存（按根目录 mtime 失效）；
//   2) canonical() 归一：候选(name 或 dir) → 剥 `plugin:` 命名空间 → 查映射 → 对去重后缀 `-N` 兜底，最终落到名单 slug。

const ALIAS_CACHE = join(homedir(), '.feedback-monitor', 'skill-alias.json');

// 技能安装根（目录名=slug）。默认覆盖 CC 与 opencode 两处；FEEDBACK_SKILL_ROOTS 以 `;` 追加。
function skillRoots() {
  const roots = [];
  const add = (p) => { try { if (p && existsSync(p)) roots.push(p); } catch {} };
  add(join(homedir(), '.claude', 'skills'));
  add('D:/opencode/config/skills');
  const extra = process.env.FEEDBACK_SKILL_ROOTS;
  if (extra) for (const r of extra.split(';').map(s => s.trim()).filter(Boolean)) add(r);
  return roots;
}

// 读 SKILL.md frontmatter 的 name（显示名）
function readSkillName(skillMd) {
  try {
    const head = readFileSync(skillMd, 'utf8').slice(0, 2000);
    const m = head.match(/^---\s*[\r\n]([\s\S]*?)[\r\n]---/);
    const fm = m ? m[1] : head;
    const nm = fm.match(/^name:\s*["']?(.+?)["']?\s*$/m);
    return nm ? nm[1].trim() : null;
  } catch { return null; }
}

// 建「name/dir → dir-slug」映射，缓存到 skill-alias.json（技能根 mtime 未变则复用，避免每个 hook 都全扫）
function buildAliasMap() {
  const roots = skillRoots();
  const sig = roots.map(r => { try { return r + ':' + statSync(r).mtimeMs; } catch { return r + ':0'; } }).join('|');
  try { const c = JSON.parse(readFileSync(ALIAS_CACHE, 'utf8')); if (c && c.sig === sig && c.map) return c.map; } catch {}
  const map = {};
  for (const root of roots) {
    let ents = [];
    try { ents = readdirSync(root, { withFileTypes: true }); } catch { continue; }
    for (const d of ents) {
      if (!d.isDirectory()) continue;
      const slug = d.name;
      map[slug] = slug;
      const nm = readSkillName(join(root, slug, 'SKILL.md'));
      if (nm && !(nm in map)) map[nm] = slug;   // 首个占位，避免重名互覆盖
    }
  }
  try {
    if (!existsSync(dirname(ALIAS_CACHE))) mkdirSync(dirname(ALIAS_CACHE), { recursive: true });
    writeFileSync(ALIAS_CACHE, JSON.stringify({ sig, map }));
  } catch {}
  return map;
}

export function loadEnterpriseSkills() {
  let slugs = [];
  try {
    const p = process.env.FEEDBACK_ENTERPRISE_LIST ||
      join(homedir(), '.feedback-monitor', 'enterprise-skills.json');
    const arr = JSON.parse(readFileSync(p, 'utf8'));
    if (Array.isArray(arr)) slugs = arr;
  } catch {}
  const slugSet = new Set(slugs);
  let alias = {};
  try { alias = buildAliasMap(); } catch {}
  const dedup = (s) => s.replace(/-\d+$/, '');            // 去重漂移后缀 `-2`
  const bareNs = (s) => { const i = s.lastIndexOf(':'); return i > 0 ? s.slice(i + 1) : s; }; // 剥 `plugin:`

  // 把候选（Skill 工具传来的 name 或 dir-slug）归一到企业名单里的 slug；不是企业 skill 返回 null。
  function canonical(name) {
    if (!name) return null;
    const cands = [];
    const n = String(name).trim();
    for (const x of [n, bareNs(n)]) if (x && !cands.includes(x)) cands.push(x);
    for (const x of cands) {
      if (slugSet.has(x)) return x;                         // 直接是名单 slug
      const s = alias[x];                                   // 显示名/目录名 → dir-slug
      if (s) { if (slugSet.has(s)) return s; const b = dedup(s); if (b !== s && slugSet.has(b)) return b; }
      const b2 = dedup(x);                                  // 候选本身是 `xxx-2` 形态
      if (b2 !== x && slugSet.has(b2)) return b2;
    }
    return null;
  }

  return {
    canonical,
    has: (name) => canonical(name) !== null,   // 兼容旧调用
    get size() { return slugSet.size; },
  };
}

// keyring 里的凭据标识（由 skills-manager 装机时写入）
export const CRED_SERVICE = 'feedback-monitor';
export const CRED_ACCOUNT = process.env.FEEDBACK_CRED_ACCOUNT || 'skill-server';
