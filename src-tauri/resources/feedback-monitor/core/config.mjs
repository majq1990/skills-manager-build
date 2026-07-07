// 反馈监控共享常量。可被环境变量覆盖（skills-manager 安装时可写入 agent 的环境）。
import { homedir } from 'node:os';
import { join } from 'node:path';

// base_url 已含 /skill-api，nginx 把 /skill-api/ 映射到 8091 的 /api/v1/，
// 所以这里只加 /feedback（不能再加 /api/v1，否则双前缀 404）。
export const SERVER_URL = process.env.FEEDBACK_SERVER_URL || 'https://demo.egova.com.cn/skill-api';
export const FEEDBACK_ENDPOINT = SERVER_URL.replace(/\/$/, '') + '/feedback';

// 卡住阈值：连续同工具 + 消极权重 >= 该值（v3 决策：先保守 10）
export const STUCK_ROUNDS = Number(process.env.FEEDBACK_STUCK_ROUNDS || 10);
export const STUCK_DEBOUNCE_MS = 24 * 3600 * 1000; // 同 tool 24h 至多 1 次
export const ERROR_DEBOUNCE_MS = 3600 * 1000;      // 同 tool+错误 1h 至多 1 次
export const DAILY_CAP = Number(process.env.FEEDBACK_DAILY_CAP || 5); // 全局日限（客户端侧；服务端仍须限流）

// 每 agent 独立状态目录，避免串扰
export const AGENT = process.env.FEEDBACK_AGENT || 'unknown';
export const STATE_DIR = process.env.FEEDBACK_STATE_DIR || join(homedir(), '.feedback-monitor', AGENT);

// keyring 里的凭据标识（由 skills-manager 装机时写入）
export const CRED_SERVICE = 'feedback-monitor';
export const CRED_ACCOUNT = process.env.FEEDBACK_CRED_ACCOUNT || 'skill-server';
