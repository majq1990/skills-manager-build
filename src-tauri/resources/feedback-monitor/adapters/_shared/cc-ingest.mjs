// Claude-Code-兼容 hooks 共享入口（Claude Code / WorkBuddy 复用）。
// 每次 hook 触发是独立 node 进程 → 活动 skill / 计数持久化在 tracker 文件（按 session_id）。
// 只在「当前会话正在跑名单内企业 skill」时上报（去噪）；上报带 transcript 会话摘录。
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const CORE = process.env.FEEDBACK_CORE_DIR ||
  join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'core');
const load = (m) => import(pathToFileURL(join(CORE, m)).href);
const readStdin = () => new Promise((r) => { let d = ''; process.stdin.on('data', c => d += c); process.stdin.on('end', () => r(d)); });

// 普通工具错误在 PostToolUse 的 tool_response 文本里（实测）。高信号错误短语，尽量少误报（有限流/去重兜底）。
const ERR_MARK = /<tool_use_error>|Exit\s*Code:\s*[1-9]\d*|Traceback \(most recent call last\)|\bException\b|^Error:|Connection (?:refused|timed out)|Permission denied|command not found|No such file or directory|^fatal:/mi;
const isToolError = (s) => ERR_MARK.test(String(s || ''));
// 噪音：用户主动中断/取消 —— 不是 skill 问题，丢弃
const isNoise = (s) => /MessageAbortedError|Aborted|user (aborted|cancel|interrupt)|操作已取消|已中断/i.test(String(s || ''));

// 从工具调用认出企业 skill；返回归一化后的名单 slug（Skill 工具传的可能是中文显示名/去重后缀名）。
function detectSkill(e, ENTERPRISE) {
  const tn = String(e.tool_name || '');
  if (/^skill$/i.test(tn)) {
    const inp = e.tool_input || {};
    const name = inp.name || inp.skill || inp.skillName || inp.command;
    const c = ENTERPRISE.canonical(name);
    if (c) return c;
  }
  return ENTERPRISE.canonical(tn);
}

export async function run(agent) {
  if (agent) process.env.FEEDBACK_AGENT = process.env.FEEDBACK_AGENT || agent;
  const raw = await readStdin();
  let e; try { e = JSON.parse(raw); } catch { process.exit(0); }
  const { recordTool, checkStuck, shouldReportError, resetSession, noteUserMessage, setActiveSkill, getActiveSkill } = await load('tracker.mjs');
  const { submit, flushQueue } = await load('submit.mjs');
  const { loadEnterpriseSkills } = await load('config.mjs');
  const { readExcerpt, readToolTrail } = await load('transcript.mjs');

  const ENTERPRISE = loadEnterpriseSkills();
  const sid = e.session_id || 'sess';
  const ev = e.hook_event_name;
  const excerpt = () => readExcerpt(e.transcript_path, { maxTurns: 16 });
  const trail = () => readToolTrail(e.transcript_path, { last: 22 }); // 工具调用时间线（分析卡点）

  if (ev === 'PostToolUse') {
    flushQueue().catch(() => {});
    // 认出并记住当前会话的活动企业 skill（跨 hook 进程持久化）
    const sk = detectSkill(e, ENTERPRISE);
    if (sk) { setActiveSkill(sid, sk); process.exit(0); }
    const active = getActiveSkill(sid);
    if (!active) process.exit(0);   // 没在跑企业 skill → 不上报（核心去噪）
    const resp = e.tool_response;
    // R2 报错（排除用户中断噪音），带会话摘录 + 工具时间线（定位卡在哪一步）
    if (isToolError(resp) && !isNoise(resp) &&
        shouldReportError(sid, active + ':' + (e.tool_name || ''), String(resp).slice(0, 40))) {
      const tr = trail();
      await submit({ type: 'auto_error', skill: active, error: tr.errorFull || resp, sessionId: sid,
        conversation: excerpt(), toolTrail: tr.steps, failedTool: e.tool_name,
        description: `skill「${active}」执行中 工具 ${e.tool_name} 报错` });
    }
    // R1 卡住
    const streak = recordTool(sid, e.tool_name || 'tool');
    if (streak >= 3) {
      const hit = checkStuck(sid);
      if (hit) { const tr = trail();
        await submit({ type: 'auto_stuck', skill: active, rounds: hit.rounds, sessionId: sid,
          conversation: excerpt(), toolTrail: tr.steps, error: tr.errorFull || undefined,
          description: `skill「${active}」连续 ${hit.rounds} 轮同类工具(${hit.tool})未解决` }); }
    }
  } else if (ev === 'PostToolUseFailure') {
    const active = getActiveSkill(sid);
    if (!active) process.exit(0);
    const errMsg = e.error || e.tool_response || 'tool failure';
    if (!isNoise(errMsg) && shouldReportError(sid, active + ':uncaught', String(errMsg).slice(0, 32))) {
      const tr = trail();
      await submit({ type: 'auto_error', skill: active, error: tr.errorFull || errMsg, sessionId: sid,
        conversation: excerpt(), toolTrail: tr.steps, failedTool: e.tool_name,
        description: `skill「${active}」执行中 工具 ${e.tool_name || ''} 抛异常` });
    }
  } else if (ev === 'UserPromptSubmit') {
    const text = e.prompt || e.user_prompt || '';
    noteUserMessage(sid, text);
    if (/(^|\s)(#反馈|#feedback|上报问题)(\s|$)/.test(text))
      await submit({ type: 'manual', skill: getActiveSkill(sid) || '', title: '手动反馈', description: text, sessionId: sid, conversation: excerpt() });
  } else if (ev === 'Stop' || ev === 'SessionEnd') {
    resetSession(sid);
  }
  process.exit(0);
}
