#!/usr/bin/env node

// Render CHANGELOG-zh.md into a standalone changelog.html page for the
// self-hosted download site (demo.egova.com.cn/MediaRoot/skill-manager/).
//
// Usage: node scripts/build-changelog-page.mjs [output.html]
//
// The page is intentionally static (no JS, no fetch): it must render even if
// latest.json is unavailable, and it is deployed by the same publish flow as
// download.html.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputPath = path.resolve(root, process.argv[2] ?? "changelog.html");

const escapeHtml = (text) =>
  text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

// Inline markdown subset used by the changelog: **bold**, `code`, [text](url).
const inline = (text) => {
  let out = escapeHtml(text);
  out = out.replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '<a href="$2">$1</a>');
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  out = out.replace(/`([^`]+)`/g, "<code>$1</code>");
  return out;
};

const parseEntries = (markdown) => {
  const entries = [];
  let current = null;
  let section = null;
  for (const rawLine of markdown.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    const versionMatch = line.match(/^## \[([^\]]+)\] - (\d{4}-\d{2}-\d{2})\s*$/);
    if (versionMatch) {
      current = { version: versionMatch[1], date: versionMatch[2], sections: [] };
      entries.push(current);
      section = null;
      continue;
    }
    if (!current) continue;
    const sectionMatch = line.match(/^### (.+)$/);
    if (sectionMatch) {
      section = { title: sectionMatch[1].trim(), items: [] };
      current.sections.push(section);
      continue;
    }
    const bulletMatch = line.match(/^- (.+)$/);
    if (bulletMatch && section) {
      section.items.push(bulletMatch[1].trim());
    }
  }
  return entries;
};

const readPackageVersion = () => {
  try {
    return JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8")).version;
  } catch {
    return "";
  }
};

const zh = fs.readFileSync(path.join(root, "CHANGELOG-zh.md"), "utf8");
const entries = parseEntries(zh);

if (entries.length === 0) {
  console.error("CHANGELOG-zh.md contained no `## [version] - date` entries");
  process.exit(1);
}

const appVersion = readPackageVersion();
const latest = entries[0];

const entryHtml = (entry, index) => {
  const isCurrent = entry.version === appVersion;
  const open = index === 0 || isCurrent ? " open" : "";
  const sections = entry.sections
    .map((section) => {
      const items = section.items
        .map((item) => `<li>${inline(item)}</li>`)
        .join("\n");
      if (!items) return "";
      return `        <h4>${escapeHtml(section.title)}</h4>\n        <ul>\n${items}\n        </ul>`;
    })
    .filter(Boolean)
    .join("\n");
  return `      <details class="entry"${open}>
        <summary>
          <span class="ver">v${escapeHtml(entry.version)}</span>
          <span class="date">${escapeHtml(entry.date)}</span>
          ${isCurrent ? '<span class="current">当前版本</span>' : ""}
        </summary>
${sections}
      </details>`;
};

const html = `<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Skills Manager - 更新历史</title>
  <style>
    :root {
      --bg: #0c0c0d;
      --card: #161618;
      --border: #2a2a2d;
      --text: #e4e4e7;
      --muted: #8b8b8b;
      --accent: #6366f1;
      --green: #22c55e;
    }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { background: var(--bg); color: var(--text); font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; min-height: 100vh; }
    header { text-align: center; padding: 60px 20px 40px; }
    header h1 { font-size: 2rem; font-weight: 700; }
    header p { color: var(--muted); margin-top: 8px; font-size: 0.95rem; }
    header .links { margin-top: 16px; display: flex; justify-content: center; gap: 12px; flex-wrap: wrap; }
    header .links a {
      color: var(--muted); text-decoration: none; font-size: 0.85rem;
      border: 1px solid var(--border); border-radius: 8px; padding: 6px 14px;
      transition: border-color 0.15s, color 0.15s;
    }
    header .links a:hover { border-color: var(--accent); color: var(--text); }
    .container { max-width: 860px; margin: 0 auto; padding: 0 20px 60px; }
    .entry { background: var(--card); border: 1px solid var(--border); border-radius: 12px; margin-bottom: 12px; }
    .entry > summary {
      display: flex; align-items: center; gap: 12px; flex-wrap: wrap;
      padding: 16px 20px; cursor: pointer; list-style: none; user-select: none;
    }
    .entry > summary::-webkit-details-marker { display: none; }
    .entry > summary:hover { border-color: var(--accent); }
    .entry[open] > summary { border-bottom: 1px solid var(--border); }
    .ver { font-size: 1.05rem; font-weight: 700; }
    .date { color: var(--muted); font-size: 0.85rem; }
    .current { background: #1a3322; color: var(--green); font-size: 0.75rem; padding: 2px 10px; border-radius: 20px; }
    .entry h4 { font-size: 0.9rem; font-weight: 600; color: var(--accent); margin: 16px 20px 8px; }
    .entry ul { margin: 0 20px 16px; padding-left: 20px; }
    .entry li { font-size: 0.9rem; line-height: 1.7; margin-bottom: 6px; color: var(--text); }
    .entry li::marker { color: var(--muted); }
    code { background: #252529; border-radius: 4px; padding: 1px 6px; font-size: 0.85em; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
    a { color: var(--accent); }
    .footer { text-align: center; padding: 20px; color: var(--muted); font-size: 0.8rem; border-top: 1px solid var(--border); margin-top: 40px; }
  </style>
</head>
<body>
<header>
  <h1>Skills Manager 更新历史</h1>
  <p>企业 Skill 管理与分发桌面工具 — 版本更新记录（最新版本 v${escapeHtml(latest.version)}，${escapeHtml(latest.date)}）</p>
  <div class="links">
    <a href="./download.html">&larr; 返回下载页</a>
    <a href="https://github.com/majq1990/skills-manager-build/releases">GitHub Releases</a>
  </div>
</header>
<div class="container">
${entries.map(entryHtml).join("\n")}
</div>
<footer class="footer">Skills Manager &copy; 2026 · 仅限企业内部使用</footer>
</body>
</html>
`;

fs.writeFileSync(outputPath, html);
console.log(
  `changelog.html written: ${entries.length} versions (${entries[0].version} .. ${entries[entries.length - 1].version})`,
);
