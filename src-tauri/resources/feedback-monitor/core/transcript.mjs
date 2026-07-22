// 会话 transcript 摘录 —— 兼容 WorkBuddy/CodeBuddy({type:"message",role,content[]})
// 与 Claude Code({type:"user|assistant",message:{role,content}})两种 .jsonl 格式。
// 抽最近若干条「用户/助手/工具调用/工具结果」压成紧凑上下文，供反馈上报（提交前由 submit 脱敏）。
import { readFileSync, statSync, openSync, readSync, closeSync } from 'node:fs';

// 只读文件尾部（大会话可能几 MB），避免读全量
function tailBytes(path, maxBytes) {
  const size = statSync(path).size;
  const start = Math.max(0, size - maxBytes);
  const fd = openSync(path, 'r');
  try {
    const buf = Buffer.alloc(size - start);
    readSync(fd, buf, 0, buf.length, start);
    return buf.toString('utf8');
  } finally { closeSync(fd); }
}

// content 可能是字符串或数组（{type,text|input_text|output_text|tool_use|tool_result}）
function contentToText(content) {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  const parts = [];
  for (const c of content) {
    if (!c || typeof c !== 'object') continue;
    if (c.type === 'text' || c.type === 'input_text' || c.type === 'output_text') parts.push(c.text || '');
    else if (c.type === 'tool_use') parts.push(`[调用工具 ${c.name || c.tool || '?'} ${JSON.stringify(c.input || {}).slice(0, 200)}]`);
    else if (c.type === 'tool_result') {
      const t = typeof c.content === 'string' ? c.content : contentToText(c.content);
      parts.push(`[工具结果] ${String(t).slice(0, 200)}`);
    }
  }
  return parts.join(' ').trim();
}

// 归一化一行 → {role, text}，非相关行返回 null。role: user|assistant|tool
function normalize(o) {
  if (!o || typeof o !== 'object') return null;
  const ty = o.type;
  // Claude Code: {type:"user|assistant", message:{role,content}}（tool_use/result 在 content 内）
  if (o.message && (ty === 'user' || ty === 'assistant')) {
    const t = contentToText(o.message.content);
    return t ? { role: o.message.role || ty, text: t } : null;
  }
  // WorkBuddy/CodeBuddy: 消息行 {type:"message", role, content}
  if (ty === 'message' && o.role) {
    const t = contentToText(o.content);
    return t ? { role: o.role, text: t } : null;
  }
  // WorkBuddy/CodeBuddy: 工具调用是独立行（OpenAI-responses 风格）
  if (ty === 'function_call' && o.name) {
    return { role: 'tool', text: `[调用 ${o.name}] ${String(o.arguments || '').slice(0, 240)}` };
  }
  if (ty === 'function_call_result' && o.name) {
    const st = o.status || '?';
    const out = typeof o.output === 'string' ? o.output : JSON.stringify(o.output || '');
    const flag = (st === 'error' || /error|failed|<tool_use_error>|Exit\s*Code:\s*[1-9]/i.test(out)) ? '⚠报错 ' : '';
    return { role: 'tool', text: `[${o.name} 结果 ${flag}status=${st}] ${out.slice(0, 300)}` };
  }
  return null;
}

/**
 * 读 transcript，返回最近 maxTurns 条消息压成的紧凑字符串。
 * @returns {string} 形如 "user: ...\nassistant: [调用工具 Bash]\n..."（未脱敏）
 */
export function readExcerpt(transcriptPath, { maxTurns = 14, perMsg = 320, totalMax = 3500 } = {}) {
  if (!transcriptPath) return '';
  let raw;
  try { raw = tailBytes(transcriptPath, 200 * 1024); } catch { return ''; }
  const lines = raw.split('\n');
  const msgs = [];
  for (const line of lines) {
    const s = line.trim();
    if (!s || s[0] !== '{') continue;
    let o; try { o = JSON.parse(s); } catch { continue; }
    const n = normalize(o);
    if (n) msgs.push(n);
  }
  const recent = msgs.slice(-maxTurns);
  const out = [];
  for (const m of recent) {
    let t = m.text.replace(/\s+/g, ' ').trim();
    // 去掉 system-reminder 噪音
    if (/^<system-reminder/.test(t)) continue;
    if (t.length > perMsg) t = t.slice(0, perMsg) + '…';
    out.push(`${m.role}: ${t}`);
  }
  // 保留最近（尾部）——失败发生在最后几轮，比开头更重要
  const joined = out.join('\n');
  return joined.length > totalMax ? '…' + joined.slice(-totalMax) : joined;
}

// 报错特征（用于给结果打 ⚠ERR 标记 + 提取最后一个失败的较全输出）
const TRAIL_ERR = /error|failed|<tool_use_error>|Exit\s*Code:\s*[1-9]|Traceback|Exception|denied|not found|timed?\s*out|拒绝|失败|无法|超时|不存在/i;

/**
 * 结构化「工具调用时间线」——分析 skill 卡点用。
 * 按出现顺序取最近 last 个工具事件（调用/结果），结果标 ok/⚠ERR，
 * 并单独返回最后一个报错的较全输出（errorFull），比 excerpt 更利于定位「卡在哪一步、报什么错」。
 * @returns {{steps:string[], errorFull:string|null}}
 */
export function readToolTrail(transcriptPath, { last = 22, briefIn = 180, briefOut = 300, errOut = 1800 } = {}) {
  if (!transcriptPath) return { steps: [], errorFull: null };
  let raw; try { raw = tailBytes(transcriptPath, 300 * 1024); } catch { return { steps: [], errorFull: null }; }
  const events = [];
  let errorFull = null;
  const brief = (s, n) => String(s == null ? '' : s).replace(/\s+/g, ' ').trim().slice(0, n);
  for (const line of raw.split('\n')) {
    const s = line.trim();
    if (!s || s[0] !== '{') continue;
    let o; try { o = JSON.parse(s); } catch { continue; }
    const ty = o.type;
    // Claude Code：tool_use / tool_result 嵌在 message.content 数组内
    if (o.message && (ty === 'assistant' || ty === 'user')) {
      const c = o.message.content;
      if (!Array.isArray(c)) continue;
      for (const b of c) {
        if (!b || typeof b !== 'object') continue;
        if (b.type === 'tool_use') {
          events.push({ k: 'call', tool: b.name || '?', text: brief(JSON.stringify(b.input || {}), briefIn) });
        } else if (b.type === 'tool_result') {
          const t = typeof b.content === 'string' ? b.content : contentToText(b.content);
          const err = b.is_error === true || TRAIL_ERR.test(t);
          if (err) errorFull = String(t).slice(0, errOut);
          events.push({ k: 'result', tool: '', err, text: brief(t, err ? 600 : briefOut) });
        }
      }
      continue;
    }
    // WorkBuddy/CodeBuddy：独立行 function_call / function_call_result
    if (ty === 'function_call' && o.name) {
      events.push({ k: 'call', tool: o.name, text: brief(o.arguments, briefIn) });
    } else if (ty === 'function_call_result' && o.name) {
      const out = typeof o.output === 'string' ? o.output : (o.output?.text ?? JSON.stringify(o.output || ''));
      const err = o.status === 'error' || TRAIL_ERR.test(out);
      if (err) errorFull = String(out).slice(0, errOut);
      events.push({ k: 'result', tool: o.name, err, text: brief(out, err ? 600 : briefOut) });
    }
  }
  const recent = events.slice(-last);
  const steps = recent.map(e => e.k === 'call'
    ? `→ ${e.tool} ${e.text}`.trim()
    : `← ${e.tool || ''} ${e.err ? '⚠ERR ' : ''}${e.text}`.replace(/\s+/g, ' ').trim());
  return { steps, errorFull };
}
