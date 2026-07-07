// 上报：读 token（keyring）→ 脱敏 → POST /api/v1/feedback → 失败入离线队列。
import { readFileSync, writeFileSync, appendFileSync, existsSync, mkdirSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { summarize, sanitize } from './sanitize.mjs';
import { FEEDBACK_ENDPOINT, STATE_DIR, AGENT, CRED_SERVICE, CRED_ACCOUNT } from './config.mjs';

const QUEUE = join(STATE_DIR, 'queue.jsonl');
const DEAD = join(STATE_DIR, 'dead.jsonl');

// token 读取优先级：环境变量 → token 文件（skills-manager 写）→ OS 凭据库。
// 注意：Windows Credential Manager 用 cmdkey 读不出密码，需 CredRead（PowerShell）。
export function readToken() {
  if (process.env.FEEDBACK_TOKEN) return process.env.FEEDBACK_TOKEN.trim();
  const tf = join(STATE_DIR, 'token');
  if (existsSync(tf)) { const t = readFileSync(tf, 'utf8').trim(); if (t) return t; }
  try {
    if (process.platform === 'darwin')
      return execFileSync('security', ['find-generic-password', '-a', CRED_ACCOUNT, '-s', CRED_SERVICE, '-w'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    if (process.platform === 'linux')
      return execFileSync('secret-tool', ['lookup', 'service', CRED_SERVICE, 'account', CRED_ACCOUNT], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    if (process.platform === 'win32') {
      // cmdkey 不回显密码；用 PowerShell + CredentialManager 读取（需 skills-manager 装机时用同一 target 写入）
      const ps = `[void][Windows.Security.Credentials.PasswordVault,Windows.Security.Credentials,ContentType=WindowsRuntime];` +
        `(New-Object Windows.Security.Credentials.PasswordVault).Retrieve('${CRED_SERVICE}','${CRED_ACCOUNT}').Password`;
      return execFileSync('powershell', ['-NoProfile', '-Command', ps], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    }
  } catch { /* 读取失败 → 无 token */ }
  return null;
}

function enqueue(file, record) {
  if (!existsSync(STATE_DIR)) mkdirSync(STATE_DIR, { recursive: true });
  appendFileSync(file, JSON.stringify(record) + '\n');
}

// 组装并脱敏 payload；自动触发默认 summary（不传全文）
function buildPayload({ type, skill, title, description, conversation, error, rounds, sessionId }) {
  const isAuto = type === 'auto_stuck' || type === 'auto_error';
  const conv = conversation ? summarize(conversation) : null;
  const err = error ? sanitize(error) : null;
  return {
    payload: {
      type, skill: skill || '',
      title: title ? sanitize(title).text : '',
      description: description ? sanitize(description).text : '',
      source: { trigger: type, agent: AGENT, platform: process.platform },
      conversation: conv ? { rounds: rounds || null, session_id: sessionId || null, summary: conv.text } : undefined,
      error: err ? { message: err.text } : undefined,
    },
    needsReview: (conv?.needsReview || err?.needsReview) === true,
    isAuto,
  };
}

export async function submit(input, opts = {}) {
  const { payload } = buildPayload(input);
  const q = (reason) => { if (!opts.noQueue) enqueue(QUEUE, { input, reason }); return { ok: false, reason }; };
  const token = readToken();
  if (!token) return q('no-token');
  try {
    const res = await fetch(FEEDBACK_ENDPOINT, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify(payload),
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) return q(`http-${res.status}`);
    return { ok: true };
  } catch (e) {
    return q(String(e?.message || e));
  }
}

// 重试离线队列：成功删除，>=5 次失败移入 dead
export async function flushQueue() {
  if (!existsSync(QUEUE)) return;
  const lines = readFileSync(QUEUE, 'utf8').split('\n').filter(Boolean);
  const remain = [];
  for (const line of lines) {
    let rec; try { rec = JSON.parse(line); } catch { continue; }
    const r = await submit(rec.input, { noQueue: true });
    if (!r.ok) { rec.tries = (rec.tries || 0) + 1; (rec.tries >= 5 ? () => enqueue(DEAD, rec) : () => remain.push(rec))(); }
  }
  if (remain.length) writeFileSync(QUEUE, remain.map(r => JSON.stringify(r)).join('\n') + '\n');
  else try { unlinkSync(QUEUE); } catch {}
}
