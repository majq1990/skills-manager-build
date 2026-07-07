// 轮次追踪 + 卡住/报错启发式 + 防抖。
// 状态落 per-agent 目录的 JSON；多 agent 并发用 .lock 目录做最小互斥（mkdir 原子）。
import { readFileSync, writeFileSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { STATE_DIR, STUCK_ROUNDS, STUCK_DEBOUNCE_MS, ERROR_DEBOUNCE_MS, DAILY_CAP } from './config.mjs';

const STATE = join(STATE_DIR, 'tracker.json');
const NEG = ['还是不对', '又错了', '重新来', '不行', '没解决', '还是不行', 'still', 'again', "doesn't work", 'not working'];
const POS = ['解决了', '可以了', '好了', 'works now', 'fixed', 'resolved'];

function withLock(fn) {
  const lock = STATE + '.lock';
  for (let i = 0; i < 50; i++) {
    try { mkdirSync(lock); break; } catch { if (i === 49) return fn(); Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 20); }
  }
  try { return fn(); } finally { try { rmSync(lock, { recursive: true, force: true }); } catch {} }
}
function load() { try { return JSON.parse(readFileSync(STATE, 'utf8')); } catch { return { sessions: {}, reports: [] }; }
}
function save(s) { if (!existsSync(STATE_DIR)) mkdirSync(STATE_DIR, { recursive: true }); writeFileSync(STATE, JSON.stringify(s)); }

const dayStart = () => { const d = new Date(); d.setHours(0, 0, 0, 0); return d.getTime(); };
function underCap(s, now) { s.reports = (s.reports || []).filter(t => t >= dayStart()); return s.reports.length < DAILY_CAP; }

// 记录一次工具调用，返回该 session 的连续同工具计数
export function recordTool(sessionId, tool) {
  return withLock(() => {
    const s = load(); const cur = s.sessions[sessionId] || { tool: null, streak: 0, neg: 0, lastStuck: 0, errs: {} };
    cur.streak = cur.tool === tool ? cur.streak + 1 : 1;
    cur.tool = tool;
    s.sessions[sessionId] = cur; save(s);
    return cur.streak;
  });
}

// 用户消息里的情绪信号：消极 +2 轮权重（最多 +5），积极清零
export function noteUserMessage(sessionId, text = '') {
  return withLock(() => {
    const s = load(); const cur = s.sessions[sessionId] || { tool: null, streak: 0, neg: 0, lastStuck: 0, errs: {} };
    if (POS.some(k => text.includes(k))) { cur.neg = 0; cur.streak = 0; }
    else if (NEG.some(k => text.includes(k))) cur.neg = Math.min((cur.neg || 0) + 2, 5);
    s.sessions[sessionId] = cur; save(s);
  });
}

// 是否判定卡住（连续同工具 + 消极权重 >= 阈值），含防抖
export function checkStuck(sessionId) {
  return withLock(() => {
    const now = Date.now(); const s = load(); const cur = s.sessions[sessionId];
    if (!cur) return null;
    const score = (cur.streak || 0) + (cur.neg || 0);
    if (score < STUCK_ROUNDS) return null;
    if (now - (cur.lastStuck || 0) < STUCK_DEBOUNCE_MS) return null;
    if (!underCap(s, now)) return null;
    cur.lastStuck = now; s.reports.push(now); save(s);
    return { rounds: cur.streak, tool: cur.tool, score };
  });
}

// 报错防抖（同 tool+错误类型 1h 内至多 1 次）
export function shouldReportError(sessionId, tool, errKey = '') {
  return withLock(() => {
    const now = Date.now(); const s = load(); const cur = s.sessions[sessionId] || { errs: {} };
    const k = `${tool}:${errKey}`.slice(0, 80);
    if (now - (cur.errs?.[k] || 0) < ERROR_DEBOUNCE_MS) return false;
    if (!underCap(s, now)) return false;
    cur.errs = cur.errs || {}; cur.errs[k] = now; s.reports.push(now);
    s.sessions[sessionId] = cur; save(s);
    return true;
  });
}

export function resetSession(sessionId) {
  withLock(() => { const s = load(); delete s.sessions[sessionId]; save(s); });
}
