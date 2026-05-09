#!/usr/bin/env bash
# Publish a Skills Manager release to demo.egova.com.cn (your own update endpoint).
#
# Three platforms — all artifacts pre-built locally / on demo / on Codemagic:
#   Linux  : run `bash scripts/build-linux.sh` first (in demo Docker container)
#            → produces build-cache/linux/*
#   Windows: run `pwsh scripts/build-windows.ps1` first (on this Win11 machine)
#            → produces src-tauri/target/release/bundle/nsis/*
#   macOS  : Codemagic builds + scp's directly to demo on tag push
#            → release.sh skips uploading mac if build-cache/macos-* is empty
#              (Codemagic owns those filenames on the server)
#
# Required env (read from signer.key if not set, but the password must be in env):
#   TAURI_SIGNING_PRIVATE_KEY             (used at build time, not here)
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD    (same)
#
# What it does:
#   1. Read version from src-tauri/tauri.conf.json
#   2. Collect Linux artifacts from build-cache/linux/
#   3. Collect Windows artifacts from src-tauri/target/release/bundle/nsis/
#   4. Optionally collect mac artifacts from build-cache/macos-{arm64,x64}/
#      (used to build latest.json signatures; binaries themselves come from Codemagic)
#   5. Build latest.json
#   6. Scp Linux + Windows binaries + latest.json to demo:/egova/MediaRoot/skill-manager/

set -euo pipefail

REMOTE_HOST=${REMOTE_HOST:-root@demo.egova.com.cn}
REMOTE_DIR=${REMOTE_DIR:-/egova/MediaRoot/skill-manager}
PUBLIC_BASE=${PUBLIC_BASE:-https://demo.egova.com.cn/MediaRoot/skill-manager}

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
CONF=$REPO_ROOT/src-tauri/tauri.conf.json
RELEASE_NOTES=${RELEASE_NOTES:-"Routine release."}

CONF_WIN=$(cygpath -w "$CONF" 2>/dev/null || echo "$CONF")
VERSION=$(python -c "import json,sys; print(json.load(open(sys.argv[1]))['version'])" "$CONF_WIN")
echo "[release] version=$VERSION"

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

# helper: json-escape a string with python
json_str() {
    python -c "import json,sys; print(json.dumps(sys.argv[1]))" "$1"
}

# ---- 1. Linux artifacts ----
LINUX_DIR=$REPO_ROOT/build-cache/linux
LINUX_DEB=$(ls "$LINUX_DIR"/*.deb 2>/dev/null | head -1 || true)
LINUX_RPM=$(ls "$LINUX_DIR"/*.rpm 2>/dev/null | head -1 || true)
LINUX_APPIMAGE=$(ls "$LINUX_DIR"/*.AppImage 2>/dev/null | head -1 || true)
LINUX_UPDATER=$(ls "$LINUX_DIR"/*.AppImage.tar.gz 2>/dev/null | head -1 || true)
LINUX_UPDATER_SIG=$(ls "$LINUX_DIR"/*.AppImage.tar.gz.sig 2>/dev/null | head -1 || true)
if [ -z "$LINUX_DEB" ] && [ -z "$LINUX_APPIMAGE" ]; then
    echo "WARN: no Linux artifacts in $LINUX_DIR — run scripts/build-linux.sh first" >&2
    LINUX_AVAILABLE=0
else
    LINUX_AVAILABLE=1
    echo "[release] linux: $LINUX_DEB | $LINUX_APPIMAGE | updater=$LINUX_UPDATER"
fi

# ---- 2. Windows artifacts ----
WIN_DIR=$REPO_ROOT/src-tauri/target/release/bundle/nsis
WIN_EXE=$(ls "$WIN_DIR"/*"$VERSION"*-setup.exe 2>/dev/null | head -1 || true)
WIN_SIG=$(ls "$WIN_DIR"/*"$VERSION"*-setup.exe.sig 2>/dev/null | head -1 || true)
if [ -z "$WIN_EXE" ] || [ -z "$WIN_SIG" ]; then
    echo "WARN: missing Windows artifacts for $VERSION in $WIN_DIR — run scripts/build-windows.ps1 first" >&2
    WIN_AVAILABLE=0
else
    WIN_AVAILABLE=1
    echo "[release] windows: $WIN_EXE + $WIN_SIG"
fi

# ---- 3. mac artifacts (optional — Codemagic uploads binaries directly to demo) ----
ARM_APP_TGZ=$(ls "$REPO_ROOT/build-cache/macos-arm64"/*.app.tar.gz 2>/dev/null | head -1 || true)
ARM_APP_SIG=$(ls "$REPO_ROOT/build-cache/macos-arm64"/*.app.tar.gz.sig 2>/dev/null | head -1 || true)
X64_APP_TGZ=$(ls "$REPO_ROOT/build-cache/macos-x64"/*.app.tar.gz 2>/dev/null | head -1 || true)
X64_APP_SIG=$(ls "$REPO_ROOT/build-cache/macos-x64"/*.app.tar.gz.sig 2>/dev/null | head -1 || true)
if [ -n "$ARM_APP_SIG" ] && [ -n "$X64_APP_SIG" ]; then
    MAC_AVAILABLE=1
    echo "[release] mac: arm64+x64 sigs present (binaries served by Codemagic upload)"
else
    MAC_AVAILABLE=0
    echo "[release] mac: skipped (no sigs in build-cache/macos-*)"
fi

if [ "$LINUX_AVAILABLE" = 0 ] && [ "$WIN_AVAILABLE" = 0 ] && [ "$MAC_AVAILABLE" = 0 ]; then
    echo "ERROR: no platform has artifacts; nothing to release" >&2
    exit 1
fi

# ---- 4. Build latest.json ----
PUB_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
PLATFORMS_JSON=""

add_platform() {
    local key=$1 sig_file=$2 url=$3
    local sig
    sig=$(cat "$sig_file")
    if [ -n "$PLATFORMS_JSON" ]; then PLATFORMS_JSON="$PLATFORMS_JSON,"; fi
    PLATFORMS_JSON+="
    \"$key\": {
      \"signature\": $(json_str "$sig"),
      \"url\": \"$url\"
    }"
}

if [ "$MAC_AVAILABLE" = 1 ]; then
    ARM_TGZ_NAME="skills-manager_${VERSION}_aarch64.app.tar.gz"
    X64_TGZ_NAME="skills-manager_${VERSION}_x86_64.app.tar.gz"
    add_platform "darwin-aarch64" "$ARM_APP_SIG" "$PUBLIC_BASE/$ARM_TGZ_NAME"
    add_platform "darwin-x86_64"  "$X64_APP_SIG" "$PUBLIC_BASE/$X64_TGZ_NAME"
fi
if [ "$WIN_AVAILABLE" = 1 ]; then
    add_platform "windows-x86_64" "$WIN_SIG" "$PUBLIC_BASE/$(basename "$WIN_EXE")"
fi
if [ "$LINUX_AVAILABLE" = 1 ] && [ -n "$LINUX_UPDATER_SIG" ]; then
    add_platform "linux-x86_64" "$LINUX_UPDATER_SIG" "$PUBLIC_BASE/$(basename "$LINUX_UPDATER")"
fi

cat >"$STAGE/latest.json" <<JSON
{
  "version": "$VERSION",
  "notes": $(json_str "$RELEASE_NOTES"),
  "pub_date": "$PUB_DATE",
  "platforms": {$PLATFORMS_JSON
  }
}
JSON

echo "[release] latest.json:"
cat "$STAGE/latest.json"

# ---- 5. Upload ----
echo "[release] ensuring remote dir exists..."
ssh "$REMOTE_HOST" "mkdir -p $REMOTE_DIR"

echo "[release] uploading artifacts..."
UPLOADS=()
if [ "$LINUX_AVAILABLE" = 1 ]; then
    [ -n "$LINUX_DEB" ]          && UPLOADS+=("$LINUX_DEB")
    [ -n "$LINUX_RPM" ]          && UPLOADS+=("$LINUX_RPM")
    [ -n "$LINUX_APPIMAGE" ]     && UPLOADS+=("$LINUX_APPIMAGE")
    [ -n "$LINUX_UPDATER" ]      && UPLOADS+=("$LINUX_UPDATER")
    [ -n "$LINUX_UPDATER_SIG" ]  && UPLOADS+=("$LINUX_UPDATER_SIG")
fi
if [ "$WIN_AVAILABLE" = 1 ]; then
    UPLOADS+=("$WIN_EXE" "$WIN_SIG")
fi
# 总是把 latest.json 拷一份到 build-cache 供预览/手工发布
LOCAL_MANIFEST="$REPO_ROOT/build-cache/latest.json.next"
mkdir -p "$(dirname "$LOCAL_MANIFEST")"
cp "$STAGE/latest.json" "$LOCAL_MANIFEST"

# SKIP_MANIFEST_UPLOAD=1 时只 scp 二进制，远端 latest.json 保持当前（保护老用户）
if [ "${SKIP_MANIFEST_UPLOAD:-}" != "1" ]; then
    UPLOADS+=("$STAGE/latest.json")
fi

scp "${UPLOADS[@]}" "$REMOTE_HOST:$REMOTE_DIR/"

echo
echo "[release] done."
if [ "${SKIP_MANIFEST_UPLOAD:-}" = "1" ]; then
    echo "  Manifest NOT uploaded (SKIP_MANIFEST_UPLOAD=1)."
    echo "    preview: $LOCAL_MANIFEST"
    echo "    publish later: scp \"$LOCAL_MANIFEST\" $REMOTE_HOST:$REMOTE_DIR/latest.json"
else
    echo "  Endpoint: $PUBLIC_BASE/latest.json"
fi
echo "  Binaries uploaded: ${#UPLOADS[@]} files"
[ "$MAC_AVAILABLE" = 0 ] && echo "  (mac binaries served separately by Codemagic when tag pushed)"
