# Windows 本机 Tauri 打包
# 用法（普通 PowerShell）：
#   cd D:\git\skills-manager
#   .\scripts\build-windows.ps1
# 输出（被 release.sh 拉取）：
#   src-tauri\target\release\bundle\nsis\skills-manager_x.y.z_x64-setup.exe
#   src-tauri\target\release\bundle\nsis\skills-manager_x.y.z_x64-setup.exe.sig

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    if (Test-Path "$repoRoot\signer.key") {
        $raw = Get-Content "$repoRoot\signer.key" -Raw
        $env:TAURI_SIGNING_PRIVATE_KEY = ($raw -replace '\s', '')
        Write-Host "[win] using TAURI_SIGNING_PRIVATE_KEY from signer.key"
    } else {
        Write-Error "TAURI_SIGNING_PRIVATE_KEY not set and signer.key not found"
        exit 1
    }
}
if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
    $pwfile = if ($env:TAURI_SIGNING_PASSWORD_FILE) { $env:TAURI_SIGNING_PASSWORD_FILE } else { "$env:USERPROFILE\.tauri-signing-password" }
    if (Test-Path $pwfile) {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content $pwfile -Raw).Trim()
        Write-Host "[win] using TAURI_SIGNING_PRIVATE_KEY_PASSWORD from $pwfile"
    } else {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
    }
}

# 1) 前端依赖
if (-not (Test-Path "$repoRoot\node_modules\.package-lock.json")) {
    Write-Host "[win] npm ci ..."
    npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
} else {
    Write-Host "[win] node_modules present, skipping npm ci"
}

# 2) tauri build
Write-Host "[win] tauri build (nsis) ..."
npx tauri build --bundles nsis
if ($LASTEXITCODE -ne 0) { throw "tauri build failed (exit $LASTEXITCODE)" }

# 3) 列出产物
$bundleDir = "$repoRoot\src-tauri\target\release\bundle\nsis"
Write-Host ""
Write-Host "[win] artifacts in $bundleDir :"
Get-ChildItem $bundleDir -ErrorAction SilentlyContinue | Format-Table Name, Length, LastWriteTime -AutoSize
