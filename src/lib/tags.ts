/** 单个标签上限（与服务端 normalizeTags / Rust normalize_tag 的 MAX_TAG_LEN 一致） */
export const MAX_TAG_LEN = 20;

/** 最多几个标签 */
export const MAX_TAG_COUNT = 5;

/**
 * 服务端的 parseValue 不识别引号转义与带引号的逗号分隔串，
 * 这些字符会让标签被静默改写/损坏，所以三端（客户端 / Rust / 服务端）
 * 都必须按同一套规则先清洗。改动这里要同步 Rust `normalize_tag`
 * 与服务端 `normalizeTags`，并由 `normalize_tag_matches_server_sanitization` 测试兜底。
 */
const HOSTILE_CHARS = /[,[\]"'\n\r\t]/g;

/** 危险字符转空格 → 压缩连续空白 → 去首尾空白 → 截断到 MAX_TAG_LEN。 */
export function sanitizeTag(raw: string): string {
  return raw
    .replace(HOSTILE_CHARS, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, MAX_TAG_LEN);
}
