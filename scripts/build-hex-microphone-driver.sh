#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$ROOT_DIR/macos/HexMicrophoneDriver"
OUT_DIR="${1:-$HOME/Library/Caches/hex-mic}"
BUNDLE_DIR="$OUT_DIR/HexMicrophone.driver"

mkdir -p "$BUNDLE_DIR/Contents/MacOS"
cp "$SRC_DIR/Info.plist" "$BUNDLE_DIR/Contents/Info.plist"

clang -std=c11 -O2 -Wall -Wextra -Werror \
  -bundle \
  -arch arm64 -arch x86_64 \
  -mmacosx-version-min=13.0 \
  -framework CoreAudio \
  -framework CoreFoundation \
  "$SRC_DIR/HexMicrophoneDriver.c" \
  -o "$BUNDLE_DIR/Contents/MacOS/HexMicrophoneDriver"

if [[ -n "${VOICE_CONTROL_TEAM_ID:-}" ]]; then
  SIGN_ID="$(security find-identity -v -p codesigning | awk -F'"' -v team="$VOICE_CONTROL_TEAM_ID" '$2 ~ "^Developer ID Application: .*\\(" team "\\)$" { print $2; exit }')"
  if [[ -n "$SIGN_ID" ]]; then
    codesign --force --options runtime --timestamp --sign "$SIGN_ID" "$BUNDLE_DIR"
  else
    codesign --force --sign - "$BUNDLE_DIR"
  fi
else
  codesign --force --sign - "$BUNDLE_DIR"
fi

cat > "$OUT_DIR/install-driver.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
sudo cp -R "$BUNDLE_DIR" /Library/Audio/Plug-Ins/HAL/ && sudo killall coreaudiod
EOF
chmod +x "$OUT_DIR/install-driver.sh"

echo "Built $BUNDLE_DIR"
echo "Install script ready at $OUT_DIR/install-driver.sh"
