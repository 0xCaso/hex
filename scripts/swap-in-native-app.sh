#!/bin/sh
set -eu

candidate=${1:-/private/var/folders/4t/_jnrck017r5f_jttbc19b7s40000gn/T/opencode/hex-native-build/Build/Products/Release/Hex.app}
target="/Applications/Hex.app"
rollback_dir="$HOME/Library/Caches/hex-rollback"
rollback_app="$rollback_dir/Hex.app"
lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

if [ ! -d "$candidate" ]; then
    echo "error: candidate bundle not found at $candidate" >&2
    echo "usage: $0 /path/to/Hex.app" >&2
    exit 1
fi

if [ ! -x "$candidate/Contents/MacOS/hex" ]; then
    echo "error: $candidate/Contents/MacOS/hex is missing or not executable" >&2
    exit 1
fi

bundle_id=$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$candidate/Contents/Info.plist" 2>/dev/null || true)
if [ "$bundle_id" != "com.kitlangton.hex2" ]; then
    echo "error: expected CFBundleIdentifier com.kitlangton.hex2, got '$bundle_id'" >&2
    exit 1
fi

team_id=$(codesign -dv "$candidate" 2>&1 | sed -n 's/^TeamIdentifier=//p' | head -n 1)
if [ "$team_id" != "QC99C9JE59" ]; then
    echo "error: expected TeamIdentifier=QC99C9JE59 so macOS TCC permissions carry over, got '$team_id'" >&2
    exit 1
fi

echo "==> Quitting running Hex..."
osascript -e 'tell application id "com.kitlangton.hex2" to quit' >/dev/null 2>&1 || true
pkill -TERM -x hex >/dev/null 2>&1 || true
sleep 1

if [ -d "$target" ]; then
    mkdir -p "$rollback_dir"
    rm -rf "$rollback_app"
    echo "==> Backing up $target to $rollback_app..."
    /usr/bin/ditto "$target" "$rollback_app"
fi

echo "==> Installing $candidate to $target..."
rm -rf "$target"
/usr/bin/ditto "$candidate" "$target"

if [ -x "$lsregister" ]; then
    echo "==> Registering $target with Launch Services..."
    "$lsregister" -f "$target"
fi

echo "==> Launching $target..."
open "$target"
echo "==> Done. Run ./scripts/restore-previous-app.sh to roll back."
