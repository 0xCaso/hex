#!/bin/sh
set -eu

target="/Applications/Hex.app"
rollback_app="$HOME/Library/Caches/hex-rollback/Hex.app"
lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

if [ ! -d "$rollback_app" ]; then
    echo "error: no backup bundle found at $rollback_app" >&2
    exit 1
fi

echo "==> Quitting running Hex..."
osascript -e 'tell application id "com.kitlangton.hex2" to quit' >/dev/null 2>&1 || true
pkill -TERM -x hex >/dev/null 2>&1 || true
sleep 1

echo "==> Restoring $rollback_app to $target..."
rm -rf "$target"
/usr/bin/ditto "$rollback_app" "$target"

if [ -x "$lsregister" ]; then
    echo "==> Registering $target with Launch Services..."
    "$lsregister" -f "$target"
fi

echo "==> Launching restored $target..."
open "$target"
echo "==> Rollback complete."
