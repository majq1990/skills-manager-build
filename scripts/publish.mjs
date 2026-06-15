#!/usr/bin/env node
/**
 * publish.mjs — 一键发布脚本
 *
 * 用法：
 *   node scripts/publish.mjs            # patch (默认)
 *   node scripts/publish.mjs patch
 *   node scripts/publish.mjs minor
 *   node scripts/publish.mjs major
 *   node scripts/publish.mjs 2.0.0      # 指定版本号
 *   node scripts/publish.mjs --dry-run  # 只预览，不实际执行
 *
 * 流程：
 *   1. npm run release:prepare — 更新版本号 + CHANGELOG
 *   2. git commit + tag
 *   3. git push --follow-tags（触发 GitHub Actions 自动构建 + 发布）
 *
 * 前提：
 *   - GitHub 仓库已配置 Secret: TAURI_SIGNING_PRIVATE_KEY
 *   - 本地有推送权限
 *   - 工作区干净（无未提交的修改）
 */

import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2);
const dryRun = args.includes('--dry-run');
const releaseArg = args.find(a => !a.startsWith('--')) || 'patch';

const root = process.cwd();

function run(cmd, opts = {}) {
  console.log(`  $ ${cmd}`);
  if (dryRun) return '';
  return execSync(cmd, { cwd: root, stdio: 'pipe', encoding: 'utf8', ...opts }).trim();
}

function runLive(cmd) {
  console.log(`  $ ${cmd}`);
  if (dryRun) return;
  execSync(cmd, { cwd: root, stdio: 'inherit' });
}

// ── Pre-flight checks ──

console.log('\n🔍 Pre-flight checks...\n');

// 1. Clean working tree
const status = run('rtk git status --porcelain');
if (status) {
  console.error('❌ 工作区不干净，请先提交或 stash 所有修改：');
  console.error(status);
  process.exit(1);
}
console.log('✅ 工作区干净');

// 2. On a branch (not detached HEAD)
const branch = run('rtk git branch --show-current');
if (!branch) {
  console.error('❌ 当前处于 detached HEAD，请切回主分支');
  process.exit(1);
}
console.log(`✅ 当前分支: ${branch}`);

// ── Prepare release ──

console.log(`\n📦 Preparing release: ${releaseArg}\n`);
runLive(`npm run release:prepare -- ${releaseArg}`);

// Read new version
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const version = pkg.version;
console.log(`\n📌 New version: ${version}\n`);

// ── Git commit + tag ──

console.log('📝 Committing and tagging...\n');
runLive(`rtk git add -A`);
runLive(`rtk git commit -m "chore: bump version to ${version}"`);
runLive(`rtk git tag -a "v${version}" -m "v${version}"`);

// ── Push ──

console.log('\n🚀 Pushing to remote (will trigger CI build)...\n');
runLive('rtk git push --follow-tags origin HEAD');

console.log(`
╔══════════════════════════════════════════════════════════════╗
║  ✅  Release v${version} pushed!                                  ║
║                                                              ║
║  GitHub Actions will now:                                    ║
║    1. Build for Windows / macOS / Linux                      ║
║    2. Sign with TAURI_SIGNING_PRIVATE_KEY                    ║
║    3. Create GitHub Release with installers + latest.json    ║
║                                                              ║
║  Monitor: https://github.com/majq1990/skills-manager-build/actions
║                                                              ║
║  Users will auto-update from:                                ║
║  https://github.com/majq1990/skills-manager-build/releases/latest/download/latest.json
╚══════════════════════════════════════════════════════════════╝
`);
