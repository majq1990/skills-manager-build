# 一键发布：Linux + Windows build + scp 到 demo（mac 由 Codemagic 单独处理）
#
# 用法（普通 PowerShell）：
#   cd D:\git\skills-manager
#   .\scripts\release-all.ps1
# 可选：
#   -SkipLinux   跳过 Linux build（用 build-cache/linux 已有产物）
#   -SkipWin     跳过 Windows build（用现有 nsis 产物）
#   -ReleaseNotes "本次更新内容..."

[CmdletBinding()]
param(
    [switch]$SkipLinux,
    [switch]$SkipWin,
    [string]$ReleaseNotes = "Routine release."
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

function Step($msg) {
    Write-Host ""
    Write-Host "================================================================" -ForegroundColor Cyan
    Write-Host " $msg" -ForegroundColor Cyan
    Write-Host "================================================================" -ForegroundColor Cyan
}

# 1) Linux build（在 demo Docker 里跑，产物拉回 build-cache/linux/）
if (-not $SkipLinux) {
    Step "1/3  Linux build (in demo Docker)"
    & bash scripts/build-linux.sh
    if ($LASTEXITCODE -ne 0) { throw "build-linux.sh failed (exit $LASTEXITCODE)" }
} else {
    Step "1/3  Linux build SKIPPED"
}

# 2) Windows build（本地）
if (-not $SkipWin) {
    Step "2/3  Windows build (local)"
    & "$PSScriptRoot\build-windows.ps1"
    if ($LASTEXITCODE -ne 0) { throw "build-windows.ps1 failed (exit $LASTEXITCODE)" }
} else {
    Step "2/3  Windows build SKIPPED"
}

# 3) release.sh：拼 latest.json + scp 全部到 demo
Step "3/3  Assemble + upload to demo.egova.com.cn"
$env:RELEASE_NOTES = $ReleaseNotes
& bash scripts/release.sh
if ($LASTEXITCODE -ne 0) { throw "release.sh failed (exit $LASTEXITCODE)" }

Write-Host ""
Write-Host "=== ALL DONE ===" -ForegroundColor Green
Write-Host "Endpoint: https://demo.egova.com.cn/MediaRoot/skill-manager/latest.json"
Write-Host ""
Write-Host "macOS：push tag 触发 Codemagic 自动 build + scp 到 demo（独立流程）"
