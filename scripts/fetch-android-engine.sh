#!/usr/bin/env bash
# Download scrcpy (which ships adb) for Windows into engines/android/,
# verifying the published SHA-256. Works in Git Bash, MSYS2, Linux, macOS.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/engines.env

out=engines/android
zip="scrcpy-win64-v${SCRCPY_VERSION}.zip"
url="https://github.com/Genymobile/scrcpy/releases/download/v${SCRCPY_VERSION}/${zip}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $url"
curl -fsSL --retry 4 -o "$tmp/$zip" "$url"
actual=$( (sha256sum "$tmp/$zip" 2>/dev/null || shasum -a 256 "$tmp/$zip") | cut -d' ' -f1)
if [[ "$actual" != "$SCRCPY_WIN64_SHA256" ]]; then
  echo "Checksum mismatch for $zip: expected $SCRCPY_WIN64_SHA256, got $actual" >&2
  exit 1
fi
echo "Checksum OK"

rm -rf "$out"
mkdir -p "$out"
if command -v unzip >/dev/null; then
  unzip -q "$tmp/$zip" -d "$tmp/x"
else
  powershell -NoProfile -Command "Expand-Archive -LiteralPath '$(cygpath -w "$tmp/$zip" 2>/dev/null || echo "$tmp/$zip")' -DestinationPath '$(cygpath -w "$tmp/x" 2>/dev/null || echo "$tmp/x")'"
fi
src="$tmp/x/scrcpy-win64-v${SCRCPY_VERSION}"
# Everything except the helper scripts meant for manual use.
for f in "$src"/*; do
  case "$(basename "$f")" in
    open_a_terminal_here.bat|scrcpy-noconsole.vbs|scrcpy-console.bat) ;;
    LICENSE.txt) cp "$f" "$out/LICENSE-scrcpy.txt" ;;
    *) cp "$f" "$out/" ;;
  esac
done
echo "scrcpy ${SCRCPY_VERSION} (with adb) -> $out"
ls -la "$out"
