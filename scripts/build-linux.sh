#!/usr/bin/env bash
# 在 demo.egova.com.cn 的 tauri-runner 容器里 build Linux Tauri 包
# 然后把产物（.deb/.rpm/.AppImage + updater .AppImage.tar.gz + .sig）拉回本地。
#
# 用法（Windows Git Bash 或 WSL）：
#   GH_TOKEN_NOT_NEEDED=1 \
#   TAURI_SIGNING_PRIVATE_KEY="$(cat /c/Users/majq1/.tauri-key)" \
#   bash scripts/build-linux.sh
#
# 输出：
#   build-cache/linux/skills-manager_x.y.z_amd64.deb
#   build-cache/linux/skills-manager_x.y.z_amd64.AppImage
#   build-cache/linux/skills-manager_x.y.z_amd64.AppImage.tar.gz
#   build-cache/linux/skills-manager_x.y.z_amd64.AppImage.tar.gz.sig
#   build-cache/linux/skills-manager-x.y.z-1.x86_64.rpm

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
REMOTE_HOST=${REMOTE_HOST:-root@demo.egova.com.cn}
IMAGE=${IMAGE:-tauri-runner:latest}
REMOTE_WORK=/data/skills-manager-build
CACHE_DIR=$REPO_ROOT/build-cache/linux

# ssh keep-alive，避免大 build 期间连接静默被中间网关丢弃
SSH_OPTS=(-o ServerAliveInterval=30 -o ServerAliveCountMax=10 -o TCPKeepAlive=yes)
ssh()  { command ssh  "${SSH_OPTS[@]}" "$@"; }
scp()  { command scp  "${SSH_OPTS[@]}" "$@"; }
# 默认跳过 AppImage（国内网络下 linuxdeploy-plugin-appimage 下载会卡）；
# 启用时：BUNDLES=deb,rpm,appimage bash scripts/build-linux.sh
BUNDLES=${BUNDLES:-deb,rpm}

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    if [ -f "$REPO_ROOT/signer.key" ]; then
        TAURI_SIGNING_PRIVATE_KEY=$(tr -d '[:space:]' < "$REPO_ROOT/signer.key")
        echo "[linux] using TAURI_SIGNING_PRIVATE_KEY from signer.key"
    else
        echo "ERROR: TAURI_SIGNING_PRIVATE_KEY not set and signer.key not found" >&2
        exit 1
    fi
fi

# 解密密码：env 优先，否则从 ~/.tauri-signing-password 读（一行明文）
if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]; then
    PWFILE=${TAURI_SIGNING_PASSWORD_FILE:-"$HOME/.tauri-signing-password"}
    if [ -f "$PWFILE" ]; then
        TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$(tr -d '\r\n' < "$PWFILE")
        echo "[linux] using TAURI_SIGNING_PRIVATE_KEY_PASSWORD from $PWFILE"
    fi
fi
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}

mkdir -p "$CACHE_DIR"

echo "[linux] preparing remote work dir on $REMOTE_HOST..."
ssh "$REMOTE_HOST" "mkdir -p $REMOTE_WORK/src && rm -rf $REMOTE_WORK/dist"

echo "[linux] sync source tree (excluding node_modules/target — they are reused on remote)..."
# rsync 优先（增量同步保留远端 node_modules / target），无 rsync 走 tar （也不删远端 target）
if command -v rsync >/dev/null 2>&1; then
    rsync -az \
        --exclude=node_modules --exclude=src-tauri/target \
        --exclude=build-cache --exclude=dist --exclude=.git \
        --exclude=.github --exclude=_work \
        -e ssh \
        "$REPO_ROOT"/ "$REMOTE_HOST:$REMOTE_WORK/src/"
else
    echo "[linux] rsync missing, falling back to tar over ssh"
    tar -C "$REPO_ROOT" \
        --exclude=node_modules --exclude=src-tauri/target \
        --exclude=build-cache --exclude=dist --exclude=.git \
        --exclude=.github --exclude=_work \
        -czf - . | ssh "$REMOTE_HOST" "tar -C $REMOTE_WORK/src -xzf -"
fi

echo "[linux] running tauri build inside docker..."
ssh "$REMOTE_HOST" \
    "docker run --rm \
        -v $REMOTE_WORK/src:/work \
        -v $REMOTE_WORK/cargo-registry:/usr/local/cargo/registry \
        -e TAURI_SIGNING_PRIVATE_KEY='$TAURI_SIGNING_PRIVATE_KEY' \
        -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD='${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}' \
        -w /work \
        --user root \
        --entrypoint bash \
        $IMAGE \
        -lc 'set -euxo pipefail; \
            git config --global --add safe.directory /work; \
            npm config set registry https://registry.npmmirror.com; \
            npm ci; \
            npx tauri build --target x86_64-unknown-linux-gnu \
                --bundles ${BUNDLES:-deb,rpm}'"

echo "[linux] collecting artifacts..."
BUNDLE=$REMOTE_WORK/src/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle
ssh "$REMOTE_HOST" "rm -f $REMOTE_WORK/dist/* 2>/dev/null; mkdir -p $REMOTE_WORK/dist; \
    for src in \
        '$BUNDLE/deb/'*.deb \
        '$BUNDLE/deb/'*.deb.sig \
        '$BUNDLE/rpm/'*.rpm \
        '$BUNDLE/rpm/'*.rpm.sig \
        '$BUNDLE/appimage/'*.AppImage \
        '$BUNDLE/appimage/'*.AppImage.tar.gz \
        '$BUNDLE/appimage/'*.AppImage.tar.gz.sig; do \
            [ -e \"\$src\" ] && cp -v \"\$src\" $REMOTE_WORK/dist/ ; \
    done; \
    ls -la $REMOTE_WORK/dist/"

echo "[linux] downloading artifacts to $CACHE_DIR..."
rm -f "$CACHE_DIR"/*.deb "$CACHE_DIR"/*.rpm "$CACHE_DIR"/*.AppImage* 2>/dev/null || true
scp "$REMOTE_HOST:$REMOTE_WORK/dist/*" "$CACHE_DIR/"

echo
echo "[linux] done. artifacts in $CACHE_DIR:"
ls -la "$CACHE_DIR/"
