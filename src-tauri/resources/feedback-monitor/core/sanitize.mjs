// 脱敏：上报前对会话/输出做敏感信息清洗。
// 审计 R4 结论：现有 log_sanitize 太窄且没接到反馈链路 —— 此处重建并强制在 submit 前调用。
// 原则：宁可多杀（needs_review 人审），自动触发默认只传摘要不传全文。

const RULES = [
  // JWT（含企业 token 本身）
  [/eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}/g, '***REDACTED-JWT***'],
  // Bearer / OpenAI / GitHub / Slack 前缀 token
  [/\b(Bearer\s+|sk-|ghp_|gho_|ghu_|ghr_|ghs_|xox[abprs]-)[A-Za-z0-9_-]{10,}/g, '***REDACTED-TOKEN***'],
  // AWS Access Key
  [/\bAKIA[0-9A-Z]{16}\b/g, '***REDACTED-AWS***'],
  // 私钥 PEM 块
  [/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g, '***REDACTED-PRIVATE-KEY***'],
  // 密码/口令 赋值
  [/\b(password|passwd|pwd|secret|token|api[_-]?key)\s*[=:]\s*['"]?([^\s'"&]{4,})/gi, '$1=***'],
  // DB / 服务连接串里的账号密码
  [/\b([a-z]+):\/\/([^:@/\s]+):([^@/\s]+)@/gi, '$1://$2:***@'],
  // URL 内联凭证 ?key=/?token=（保留域名与参数名）
  [/([?&](?:key|token|access_token|apikey)=)[^\s&"']+/gi, '$1***'],
  // 手机号
  [/\b1[3-9]\d{9}\b/g, m => m.slice(0, 3) + '****' + m.slice(7)],
  // 内网 IPv4
  [/\b(?:10|127|169\.254|172\.(?:1[6-9]|2\d|3[01])|192\.168)(?:\.\d{1,3}){2,3}\b/g, 'xxx.xxx.xxx.xxx'],
  // 用户主目录路径（只留末级）
  [/(?:[A-Za-z]:\\Users\\|\/home\/|\/Users\/)[^\s'"]*[\\/]([^\\/\s'"]+)/g, '<path>/$1'],
];

// 高熵兜底：连续 >=32 位 hex 或长 base64 串，疑似密钥 → 打标 needs_review（不直接删，避免误杀正文）
const HIGH_ENTROPY = /\b(?:[A-Fa-f0-9]{32,}|[A-Za-z0-9+/]{40,}={0,2})\b/g;

export function sanitize(input) {
  if (input == null) return { text: '', needsReview: false };
  let text = typeof input === 'string' ? input : JSON.stringify(input);
  for (const [re, rep] of RULES) text = text.replace(re, rep);
  const needsReview = HIGH_ENTROPY.test(text);
  HIGH_ENTROPY.lastIndex = 0;
  return { text, needsReview };
}

// 自动触发默认只传摘要：截断 + 脱敏
export function summarize(input, max = 800) {
  const { text, needsReview } = sanitize(input);
  return { text: text.length > max ? text.slice(0, max) + `…[+${text.length - max}]` : text, needsReview };
}

// node core/sanitize.mjs --selftest
if (process.argv.includes('--selftest')) {
  const cases = [
    'Authorization: Bearer sk-ABCDEFGHIJKLMNOP1234',
    'jwt=eyJhbGciOi.eyJzdWIiOiIx.SflKxwRJSMeKKF2QT4',
    'postgres://admin:s3cr3tPass@10.0.0.5:5432/db',
    '手机 15928716057 路径 C:\\Users\\majq1\\.ssh\\id_ed25519',
    'https://mcp-gw.dingtalk.com/server/abc?key=ef527a7e5dbb5d1fd81efa433c1997a9',
  ];
  for (const c of cases) console.log(JSON.stringify(sanitize(c)));
}
