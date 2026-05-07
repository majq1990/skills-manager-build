#!/usr/bin/env bash
# Publish a Skills Manager release to demo.egova.com.cn (your own update endpoint).
#
# Prereqs:
#   - Local machine has ssh-key access to root@demo.egova.com.cn
#   - GitHub token at C:/Users/majq1/.github-token (or env GITHUB_TOKEN)
#   - Mac build on GitHub Actions finished for the current HEAD commit
#     (run `gh workflow run` or UI dispatch before this script)
#   - Local Windows NSIS build finished:
#       npx tauri build --bundles nsis
#
# What it does:
#   1. Read version from src-tauri/tauri.conf.json
#   2. Download the latest GitHub Actions mac artifacts (arm64 + x64)
#   3. Extract mac artifact zips to collect .app.tar.gz + .sig
#   4. Collect local Windows .exe + .nsis.zip + .sig
#   5. Build latest.json from all signatures
#   6. Scp everything to demo.egova.com.cn:/egova/MediaRoot/skill-manager/
set -euo pipefail

REMOTE_HOST=root@demo.egova.com.cn
REMOTE_DIR=/egova/MediaRoot/skill-manager
GH_REPO=majq1990/skills-manager-build
PUBLIC_BASE=https://demo.egova.com.cn/MediaRoot/skill-manager

# GitHub API is firewalled slow (5 KB/s direct). Use local proxy if available.
GH_PROXY=${GH_PROXY:-${ALL_PROXY:-}}
CURL_GH=(curl -fsSL --connect-timeout 10 --retry 5 --retry-all-errors --retry-delay 3)
if [ -n "$GH_PROXY" ]; then
    # Strip scheme for curl --socks5-hostname / --proxy
    case "$GH_PROXY" in
        socks5://*|socks5h://*) CURL_GH+=(--socks5-hostname "${GH_PROXY#socks5*://}") ;;
        *) CURL_GH+=(--proxy "$GH_PROXY") ;;
    esac
    echo "[release] using proxy $GH_PROXY for github.com"
fi

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
CONF=$REPO_ROOT/src-tauri/tauri.conf.json
RELEASE_NOTES=${RELEASE_NOTES:-"Routine release."}

CONF_WIN=$(cygpath -w "$CONF" 2>/dev/null || echo "$CONF")
VERSION=$(python -c "import json,sys; print(json.load(open(sys.argv[1]))['version'])" "$CONF_WIN")
echo "[release] version=$VERSION"

TOKEN_FILE=${GITHUB_TOKEN_FILE:-"$HOME/.github-token"}
if [ -z "${GITHUB_TOKEN:-}" ] && [ -f "$TOKEN_FILE" ]; then
    GITHUB_TOKEN=$(cat "$TOKEN_FILE" | tr -d '\r\n ')
fi
if [ -z "${GITHUB_TOKEN:-}" ]; then
    echo "ERROR: need GITHUB_TOKEN env or $TOKEN_FILE" >&2
    exit 1
fi

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
echo "[release] staging dir: $STAGE"

gh_api() {
    "${CURL_GH[@]}" -H "Authorization: token $GITHUB_TOKEN" -H "Accept: application/vnd.github+json" "$@"
}

# ---- 1. Find the newest workflow run with both mac artifacts available ----
echo "[release] looking up latest build-mac run..."
gh_api "https://api.github.com/repos/$GH_REPO/actions/runs?per_page=10&status=success" > "$STAGE/runs.json"
RUNS_WIN=$(cygpath -w "$STAGE/runs.json" 2>/dev/null || echo "$STAGE/runs.json")
RUN_ID=$(python -c "
import json,sys
d = json.load(open(sys.argv[1], encoding='utf-8'))
for r in d.get('workflow_runs', []):
    if 'Build macOS' in r.get('name','') and r.get('conclusion') == 'success':
        print(r['id']); break
" "$RUNS_WIN")
if [ -z "$RUN_ID" ]; then
    echo "ERROR: no successful mac run found" >&2
    exit 1
fi
echo "[release] using run id: $RUN_ID"

gh_api "https://api.github.com/repos/$GH_REPO/actions/runs/$RUN_ID/artifacts" > "$STAGE/artifacts.json"
ART_WIN=$(cygpath -w "$STAGE/artifacts.json" 2>/dev/null || echo "$STAGE/artifacts.json")

# ---- 2. Download + extract both mac artifacts ----
# Accept a local cache dir to skip GitHub download entirely (useful when the
# network can't reliably pull artifacts — just fetch them manually via browser
# and drop the zip files here).
CACHE_DIR=${BUILD_CACHE:-$REPO_ROOT/build-cache}
mkdir -p "$CACHE_DIR"
for NAME in skills-manager-macos-arm64 skills-manager-macos-x64; do
    CACHED="$CACHE_DIR/$NAME.zip"
    if [ -s "$CACHED" ]; then
        echo "[release] using cached $CACHED ($(du -h "$CACHED" | cut -f1))"
        cp "$CACHED" "$STAGE/$NAME.zip"
    else
        ID=$(python -c "
import json,sys
d = json.load(open(sys.argv[1], encoding='utf-8'))
for a in d.get('artifacts', []):
    if a['name'] == sys.argv[2]:
        print(a['id']); break
" "$ART_WIN" "$NAME")
        if [ -z "$ID" ]; then
            echo "ERROR: artifact $NAME not found in run $RUN_ID" >&2
            exit 1
        fi
        echo "[release] downloading $NAME (artifact id=$ID)..."
        "${CURL_GH[@]}" --max-time 900 -H "Authorization: token $GITHUB_TOKEN" \
            -o "$STAGE/$NAME.zip" \
            "https://api.github.com/repos/$GH_REPO/actions/artifacts/$ID/zip"
        # Cache for future re-runs
        cp "$STAGE/$NAME.zip" "$CACHED"
    fi
    mkdir -p "$STAGE/$NAME"
    unzip -q -o "$STAGE/$NAME.zip" -d "$STAGE/$NAME"
    ls "$STAGE/$NAME"
done

# ---- 3. Collect Windows NSIS from local build output ----
# Tauri v2 Windows updater uses setup.exe + setup.exe.sig directly (no nsis.zip).
WIN_DIR=$REPO_ROOT/src-tauri/target/release/bundle/nsis
WIN_EXE=$(ls "$WIN_DIR"/*"$VERSION"*-setup.exe 2>/dev/null | head -1 || true)
WIN_SIG=$(ls "$WIN_DIR"/*"$VERSION"*-setup.exe.sig 2>/dev/null | head -1 || true)
if [ -z "$WIN_EXE" ] || [ -z "$WIN_SIG" ]; then
    echo "ERROR: missing Windows artifacts for $VERSION in $WIN_DIR" >&2
    echo "       Need: <name>-setup.exe + <name>-setup.exe.sig" >&2
    echo "       Run:  npx tauri build --bundles nsis (with TAURI_SIGNING_PRIVATE_KEY set)" >&2
    ls "$WIN_DIR" || true
    exit 1
fi
echo "[release] windows: $WIN_EXE + $WIN_SIG"

# ---- 4. Find mac artifacts (arm64 + x64) ----
ARM_APP_TGZ=$(ls "$STAGE/skills-manager-macos-arm64"/*.app.tar.gz 2>/dev/null | head -1 || true)
ARM_APP_SIG=$(ls "$STAGE/skills-manager-macos-arm64"/*.app.tar.gz.sig 2>/dev/null | head -1 || true)
ARM_DMG=$(ls "$STAGE/skills-manager-macos-arm64"/*.dmg 2>/dev/null | head -1 || true)
X64_APP_TGZ=$(ls "$STAGE/skills-manager-macos-x64"/*.app.tar.gz 2>/dev/null | head -1 || true)
X64_APP_SIG=$(ls "$STAGE/skills-manager-macos-x64"/*.app.tar.gz.sig 2>/dev/null | head -1 || true)
X64_DMG=$(ls "$STAGE/skills-manager-macos-x64"/*.dmg 2>/dev/null | head -1 || true)
for f in ARM_APP_TGZ ARM_APP_SIG X64_APP_TGZ X64_APP_SIG; do
    eval "v=\$$f"
    if [ -z "$v" ]; then
        echo "ERROR: missing $f — does the CI build include updater artifacts? Set TAURI_SIGNING_PRIVATE_KEY in GH secrets + createUpdaterArtifacts=true." >&2
        exit 1
    fi
done

# ---- 5. Build latest.json ----
ARM_SIG_STR=$(cat "$ARM_APP_SIG")
X64_SIG_STR=$(cat "$X64_APP_SIG")
WIN_SIG_STR=$(cat "$WIN_SIG")
PUB_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Mac .app.tar.gz is named `skills-manager.app.tar.gz` for BOTH targets — rename
# with target suffix so they don't overwrite each other on the server.
ARM_TGZ_NAME="skills-manager_${VERSION}_aarch64.app.tar.gz"
X64_TGZ_NAME="skills-manager_${VERSION}_x86_64.app.tar.gz"
ARM_URL="$PUBLIC_BASE/$ARM_TGZ_NAME"
X64_URL="$PUBLIC_BASE/$X64_TGZ_NAME"
WIN_URL="$PUBLIC_BASE/$(basename "$WIN_EXE")"

cat >"$STAGE/latest.json" <<JSON
{
  "version": "$VERSION",
  "notes": $(python -c "import json,sys; print(json.dumps(sys.argv[1]))" "$RELEASE_NOTES"),
  "pub_date": "$PUB_DATE",
  "platforms": {
    "darwin-aarch64": {
      "signature": $(python -c "import json,sys; print(json.dumps(sys.argv[1]))" "$ARM_SIG_STR"),
      "url": "$ARM_URL"
    },
    "darwin-x86_64": {
      "signature": $(python -c "import json,sys; print(json.dumps(sys.argv[1]))" "$X64_SIG_STR"),
      "url": "$X64_URL"
    },
    "windows-x86_64": {
      "signature": $(python -c "import json,sys; print(json.dumps(sys.argv[1]))" "$WIN_SIG_STR"),
      "url": "$WIN_URL"
    }
  }
}
JSON
echo "[release] latest.json:"
cat "$STAGE/latest.json"

# ---- 6. Scp everything to demo.egova.com.cn ----
echo "[release] ensuring remote dir exists..."
ssh "$REMOTE_HOST" "mkdir -p $REMOTE_DIR"

echo "[release] uploading artifacts..."
# Upload mac .app.tar.gz with target-specific rename; rest keep original names
scp "$ARM_APP_TGZ" "$REMOTE_HOST:$REMOTE_DIR/$ARM_TGZ_NAME"
scp "$ARM_APP_SIG" "$REMOTE_HOST:$REMOTE_DIR/$ARM_TGZ_NAME.sig"
scp "$X64_APP_TGZ" "$REMOTE_HOST:$REMOTE_DIR/$X64_TGZ_NAME"
scp "$X64_APP_SIG" "$REMOTE_HOST:$REMOTE_DIR/$X64_TGZ_NAME.sig"
scp "$ARM_DMG" "$X64_DMG" "$WIN_EXE" "$WIN_SIG" \
    "$STAGE/latest.json" \
    "$REMOTE_HOST:$REMOTE_DIR/"

echo
echo "[release] done."
echo "  Endpoint: $PUBLIC_BASE/latest.json"
echo "  Downloads:"
for f in "$ARM_APP_TGZ" "$ARM_DMG" "$X64_APP_TGZ" "$X64_DMG" "$WIN_EXE"; do
    echo "    $PUBLIC_BASE/$(basename "$f")"
done
