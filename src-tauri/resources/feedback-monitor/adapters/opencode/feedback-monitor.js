// OpenCode 适配层 —— 实测机制（agent live 验证 v1.17.13）：
//   插件是 async 函数返回 Hooks 对象；用 tool.execute.before 计数、event 总线抓错误。
//   ⚠️ 不是 hooks.json PreToolUse（v2 假设对 opencode 为 FALSE）。
//   安装：置于 ~/.config/opencode/plugins/  或 <project>/.opencode/plugins/
//   （Windows 是 .config 非 %APPDATA%）。CORE 路径由 skills-manager 装机时注入。
import { pathToFileURL } from 'node:url';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { appendFileSync, statSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';

const CORE = process.env.FEEDBACK_CORE_DIR ||
  join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'core');
const load = (m) => import(pathToFileURL(join(CORE, m)).href);

// 轻量调试日志（排查 opencode 为何漏报用）：滚动截断，写 ~/.feedback-monitor/opencode/debug.log
const DBGDIR = join(homedir(), '.feedback-monitor', 'opencode');
const DBGFILE = join(DBGDIR, 'debug.log');
function dbg(msg) {
  try {
    if (!existsSync(DBGDIR)) mkdirSync(DBGDIR, { recursive: true });
    try { if (statSync(DBGFILE).size > 512 * 1024) writeFileSync(DBGFILE, ''); } catch {}
    appendFileSync(DBGFILE, `${new Date().toISOString()} ${msg}\n`);
  } catch { /* 调试日志失败不影响主流程 */ }
}
// 高频噪音日志（每次工具调用/事件）默认关闭，设 FEEDBACK_DEBUG=1 打开排查
const VERBOSE = process.env.FEEDBACK_DEBUG === '1';
const dbgv = (msg) => { if (VERBOSE) dbg(msg); };

export const FeedbackMonitor = async () => {
  process.env.FEEDBACK_AGENT ||= 'opencode';
  const { recordTool, checkStuck, shouldReportError, noteUserMessage } = await load('tracker.mjs');
  const { submit, flushQueue } = await load('submit.mjs');
  const { loadEnterpriseSkills } = await load('config.mjs');
  const ENTERPRISE = loadEnterpriseSkills();
  flushQueue().catch(() => {}); // 启动补发离线队列
  dbg(`=== plugin loaded (v6: skill-marker allsrc), enterprise.size=${ENTERPRISE.size} ===`);

  // opencode 插件为长驻单实例，hooks 共享闭包 → 用闭包变量存活动 skill，
  // 避免 message.part.updated 事件里拿不到 sessionID（part 只有 callID）导致的会话键错配。
  let activeSkill = null;
  // 工具调用时间线环形缓冲（分析卡点用）；opencode 无现成 transcript，靠闭包累积
  const recentTools = [];
  const pushTrail = (s) => { recentTools.push(String(s).replace(/\s+/g, ' ').slice(0, 220)); if (recentTools.length > 40) recentTools.shift(); };
  const trailNow = () => recentTools.slice(-24);
  // 会话上下文环形缓冲（opencode 拿不到 transcript，从事件累积 user/assistant 文本，补齐"会话摘要"）
  const convBuf = [];
  const pushConv = (s) => { const t = String(s || '').replace(/\s+/g, ' ').trim().slice(0, 320); if (t) { convBuf.push(t); if (convBuf.length > 24) convBuf.shift(); } };
  const convNow = () => convBuf.length ? convBuf.slice(-16).join('\n') : undefined;
  let lastEvType = '';   // 事件类型去重（调试用）

  // opencode 把 skill 作为上下文注入（不走 skill 工具）。两种识别：
  // (1) 注入文本里的 "Base directory for this skill: .../<slug>" 标记；
  // (2) 兜底：工具路径/参数里出现 skills/<slug>（技能运行时会读它自己目录下的文件）。
  const detectSkillFromText = (text) => {
    if (!text || activeSkill) return;
    const s = String(text);
    let m = s.match(/Base directory for this skill:\s*\S*[\/\\]([A-Za-z0-9._一-龥-]+)/i);
    if (!m) m = s.match(/[\/\\]skills[\/\\]([A-Za-z0-9._一-龥-]+)/i);  // 兜底：skills/<slug> 路径
    if (m) {
      const c = ENTERPRISE.canonical(m[1]);
      if (c) { activeSkill = c; dbg(`skill 识别命中 slug=${m[1]} → activeSkill=${c}`); }
    }
    // 诊断：标记到达但没提取出（regex 没覆盖的形态），先记一笔
    else if (/Base directory for this skill/i.test(s)) dbg(`skill 标记到达但未提取: ${s.slice(0, 120)}`);
  };
  // 从一个 event 里能拿到的所有文本源里扫 skill 标记
  const scanEventForSkill = (p) => {
    if (activeSkill || !p) return;
    const st = p.part?.state || {};
    for (const v of [p.info?.text, p.part?.text, st.output, st.input, st.text, st.title]) {
      if (v) { detectSkillFromText(typeof v === 'string' ? v : JSON.stringify(v)); if (activeSkill) return; }
    }
  };

  return {
    // opencode 用 `skill` 工具调技能，技能名在 args.name。命中企业名单→标为活动 skill；
    // 其余工具仅在有活动企业 skill 时计数/上报卡住，归属到该 skill。
    'tool.execute.before': async (input, output) => {
      const tool = input?.tool;
      const sid = input?.sessionID || 'oc';
      // 全量记录每次工具调用的 tool 名 + 入参键 + args 片段（排查 opencode 怎么表示技能调用）
      dbgv(`BEFORE tool=${tool} inKeys=${Object.keys(input || {}).join(',')} args=${JSON.stringify(output?.args || {}).slice(0, 160)}`);
      detectSkillFromText(JSON.stringify(output?.args || {}));   // 兜底：工具参数路径含 skills/<slug>
      // 技能识别：既认 tool==='skill'(取 args.name)，也认 tool 名本身就是企业 skill（opencode 可能把 skill 暴露成独立工具）
      if (tool === 'skill') {
        const name = output?.args?.name || output?.args?.skill || output?.args?.command;
        const c = name && ENTERPRISE.canonical(name);
        dbgv(`skill-tool name=${name} canonical=${c} → activeSkill=${c || activeSkill}`);
        if (c) activeSkill = c;
        return;
      }
      const cTool = ENTERPRISE.canonical(tool);
      if (cTool) { activeSkill = cTool; dbg(`tool-name 命中企业 skill=${cTool} → activeSkill=${cTool}`); return; }
      if (!activeSkill) return;
      pushTrail(`→ ${tool} ${JSON.stringify(output?.args || {}).slice(0, 160)}`);
      const streak = recordTool(sid, tool);
      if (streak >= 3) {
        const hit = checkStuck(sid);
        if (hit) await submit({ type: 'auto_stuck', skill: activeSkill, rounds: hit.rounds, sessionId: sid,
          toolTrail: trailNow(), conversation: convNow(),
          description: `skill ${activeSkill} 连续 ${hit.rounds} 轮同类工具(${hit.tool})未解决` });
      }
    },
    // R2（工具错误）走 event 总线；R3 手动匹配 #反馈
    event: async ({ event }) => {
      const t = event?.type, p = event?.properties;
      // 记录事件类型流（连续同类型只记一次，避免 streaming 刷屏）——看 opencode 有没有技能相关事件
      if (t && t !== lastEvType) { dbgv(`EV ${t}${p?.part?.type ? ' part=' + p.part.type : ''}${p?.info?.role ? ' role=' + p.info.role : ''}`); lastEvType = t; }
      scanEventForSkill(p);   // 每个事件都扫一遍 skill 标记（part=text / 工具结果 / 消息 都覆盖）
      if (t === 'message.updated' && p?.info?.role) {
        const text = p.info?.text || '';
        if (text) pushConv(`${p.info.role}: ${text}`);   // 累积会话上下文（user/assistant 都收）
        if (p.info.role === 'user') {
          noteUserMessage(p.info?.sessionID || 'oc', text);
          if (/(^|\s)(#反馈|#feedback|上报问题)(\s|$)/.test(text))
            await submit({ type: 'manual', skill: activeSkill || '', title: '手动反馈', description: text, toolTrail: trailNow(), conversation: convNow() });
        }
      } else if (t === 'message.part.updated' && p?.part?.type === 'tool') {
        // 记录所有工具状态流转（能看到 ddcat 报错到底是 status=error 还是别的形态）
        const status = p.part?.state?.status;
        if (status === 'error' || status === 'completed') dbgv(`tool-part tool=${p.part.tool} status=${status} activeSkill=${activeSkill}`);
        if (status !== 'error') return;
        const err = p.part.state.error || 'tool error';
        pushTrail(`← ${p.part.tool} ⚠ERR ${String(err).slice(0, 400)}`);
        if (!activeSkill) { dbg(`tool-error 但 activeSkill 空 → 不上报 (tool=${p.part.tool})`); return; }
        const ok = shouldReportError('oc', activeSkill + ':' + p.part.tool, String(err).slice(0, 40));
        dbg(`tool-error tool=${p.part.tool} skill=${activeSkill} shouldReport=${ok} err=${String(err).slice(0, 60)}`);
        if (ok) {
          const r = await submit({ type: 'auto_error', skill: activeSkill, error: err, failedTool: p.part.tool,
            toolTrail: trailNow(), conversation: convNow(), description: `工具 ${p.part.tool} 报错（skill: ${activeSkill}）` });
          dbg(`submit auto_error → ${JSON.stringify(r)}`);
        }
      }
    },
  };
};
