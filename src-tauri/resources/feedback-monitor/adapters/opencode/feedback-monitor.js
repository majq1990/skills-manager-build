// OpenCode 适配层 —— 实测机制（agent live 验证 v1.17.13）：
//   插件是 async 函数返回 Hooks 对象；用 tool.execute.before 计数、event 总线抓错误。
//   ⚠️ 不是 hooks.json PreToolUse（v2 假设对 opencode 为 FALSE）。
//   安装：置于 ~/.config/opencode/plugins/  或 <project>/.opencode/plugins/
//   （Windows 是 .config 非 %APPDATA%）。CORE 路径由 skills-manager 装机时注入。
import { pathToFileURL } from 'node:url';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const CORE = process.env.FEEDBACK_CORE_DIR ||
  join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'core');
const load = (m) => import(pathToFileURL(join(CORE, m)).href);

export const FeedbackMonitor = async () => {
  process.env.FEEDBACK_AGENT ||= 'opencode';
  const { recordTool, checkStuck, shouldReportError, resetSession, noteUserMessage } = await load('tracker.mjs');
  const { submit, flushQueue } = await load('submit.mjs');
  flushQueue().catch(() => {}); // 启动补发离线队列

  return {
    // R1：每次工具调用计数；达阈值上报卡住
    'tool.execute.before': async ({ tool, sessionID }) => {
      const streak = recordTool(sessionID, tool);
      if (streak >= 3) {
        const hit = checkStuck(sessionID);
        if (hit) await submit({ type: 'auto_stuck', skill: tool, rounds: hit.rounds, sessionId: sessionID,
          description: `连续 ${hit.rounds} 轮同类工具(${hit.tool})未解决` });
      }
    },
    // R2（工具错误）走 event 总线的 message.part.updated（tool.execute.after 无 error 字段）
    event: async ({ event }) => {
      const t = event?.type, p = event?.properties;
      if (t === 'message.part.updated' && p?.part?.type === 'tool' && p.part?.state?.status === 'error') {
        const err = p.part.state.error || 'tool error';
        if (shouldReportError(p.part.callID?.split('_')[0] || 'sess', p.part.tool, String(err).slice(0, 40)))
          await submit({ type: 'auto_error', skill: p.part.tool, error: err });
      } else if (t === 'session.error') {
        await submit({ type: 'auto_error', skill: 'session', error: JSON.stringify(p || {}) });
      } else if (t === 'session.idle' && p?.sessionID) {
        resetSession(p.sessionID); // 一轮结束重置计数
      } else if (t === 'message.updated' && p?.info?.role === 'user') {
        noteUserMessage(p.info.sessionID || '', p.info?.text || '');
      }
    },
  };
};
