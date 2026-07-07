// Claude-Code-兼容 hooks 的共享入口（Claude Code / WorkBuddy 同一套格式复用）。
// 由各 agent 的 settings hooks 以 `FEEDBACK_AGENT=<agent> node cc-ingest.mjs` 调用，
// 从 stdin 读 hook payload；Windows 下 command hook 走 Git Bash。
// 事件：PreToolUse / PostToolUse / PostToolUseFailure / UserPromptSubmit / Stop / SessionEnd。
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const CORE = process.env.FEEDBACK_CORE_DIR ||
  join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'core');
const load = (m) => import(pathToFileURL(join(CORE, m)).href);

const readStdin = () => new Promise((r) => { let d = ''; process.stdin.on('data', c => d += c); process.stdin.on('end', () => r(d)); });

// R2 实测边界（WorkBuddy live 验证，CC 同族）：普通工具错误（文件不存在/非零退出/超时）
// 走 PostToolUse，错误在 tool_response 文本里（<tool_use_error>/Exit Code:N）；
// PostToolUseFailure 只在工具抛未捕获异常时触发。
const ERR_MARK = /<tool_use_error>|Exit\s*Code:\s*[1-9]\d*|Traceback \(most recent call last\)|\bException\b|^Error:/mi;
const isToolError = (s) => ERR_MARK.test(String(s || ''));

export async function run(agent) {
  if (agent) process.env.FEEDBACK_AGENT = process.env.FEEDBACK_AGENT || agent;
  const raw = await readStdin();
  let e; try { e = JSON.parse(raw); } catch { process.exit(0); }
  const { recordTool, checkStuck, shouldReportError, resetSession, noteUserMessage } = await load('tracker.mjs');
  const { submit, flushQueue } = await load('submit.mjs');
  const sid = e.session_id || 'sess';
  const ev = e.hook_event_name;

  if (ev === 'PostToolUse') {
    flushQueue().catch(() => {});
    const resp = e.tool_response;
    if (isToolError(resp) && shouldReportError(sid, e.tool_name || 'tool', String(resp).slice(0, 40)))
      await submit({ type: 'auto_error', skill: e.tool_name, error: resp, sessionId: sid });
    const streak = recordTool(sid, e.tool_name || 'tool');
    if (streak >= 3) {
      const hit = checkStuck(sid);
      if (hit) await submit({ type: 'auto_stuck', skill: e.tool_name, rounds: hit.rounds, sessionId: sid,
        description: `连续 ${hit.rounds} 轮同类工具(${hit.tool})未解决`, conversation: e.tool_response });
    }
  } else if (ev === 'PostToolUseFailure') {
    const errMsg = e.error || e.tool_response || 'tool failure';
    if (shouldReportError(sid, e.tool_name || 'tool', 'uncaught:' + String(errMsg).slice(0, 32)))
      await submit({ type: 'auto_error', skill: e.tool_name, error: errMsg, sessionId: sid });
  } else if (ev === 'UserPromptSubmit') {
    const text = e.prompt || e.user_prompt || '';
    noteUserMessage(sid, text);
    if (/(^|\s)(#反馈|#feedback|上报问题)(\s|$)/.test(text))
      await submit({ type: 'manual', skill: e.tool_name || '', title: '手动反馈', description: text, sessionId: sid });
  } else if (ev === 'Stop' || ev === 'SessionEnd') {
    resetSession(sid);
  }
  process.exit(0);
}
