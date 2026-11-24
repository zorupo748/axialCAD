#!/usr/bin/env bash
# Regenerate the derived app icons from assets/app-icon/axialcad-source-500.png.
# The outputs are committed, so builds and packaging never need these tools.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
SRC="$DIR/axialcad-source-500.png"
ID="ai.storyteller.cadcraft"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

resize() {
  local src="$1" size="$2" out="$3"
  if command -v sips >/dev/null 2>&1; then
    sips -s format png -z "$size" "$size" "$src" --out "$out" >/dev/null
  elif command -v magick >/dev/null 2>&1; then
    magick "$src" -resize "${size}x${size}!" "$out"
  elif command -v convert >/dev/null 2>&1; then
    convert "$src" -resize "${size}x${size}!" "$out"
  else
    echo "error: neither sips nor ImageMagick (magick/convert) found" >&2
    exit 1
  fi
}

for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  resize "$SRC" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
resize "$SRC" 64 "$DIR/cadcraft-64.png"
resize "$SRC" 256 "$DIR/cadcraft-256.png"
resize "$SRC" 1024 "$DIR/cadcraft-1024.png"

ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  resize "$SRC" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/cadcraft.ico" "${ICO_PNGS[@]}")

# macOS icons sit on Apple's grid: the macOS-margin master is cadcraft-macos-512.png,
# derived from the axialCAD source icon.
resize "$SRC" 512 "$DIR/cadcraft-macos-512.png"

SET="$TMP/cadcraft.iconset"
mkdir -p "$SET"
for s in 16 32 128 256; do
  resize "$DIR/cadcraft-macos-512.png" "$s" "$SET/icon_${s}x${s}.png"
  resize "$DIR/cadcraft-macos-512.png" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
done
resize "$DIR/cadcraft-macos-512.png" 512 "$SET/icon_512x512.png"
if command -v iconutil >/dev/null 2>&1; then
  iconutil -c icns -o "$DIR/cadcraft.icns" "$SET"
elif [ -f "$DIR/cadcraft.icns" ]; then
  echo "note: iconutil not found (macOS only); keeping existing $DIR/cadcraft.icns"
fi
echo "icons written to $DIR"
