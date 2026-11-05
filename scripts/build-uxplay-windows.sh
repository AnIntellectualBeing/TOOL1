#!/usr/bin/env bash
# Build UxPlay on Windows and bundle it with the GStreamer runtime it needs,
# into engines/iphone/. Run inside an MSYS2 UCRT64 shell (CI uses
# msys2/setup-msys2). Needs these packages:
#
#   pacman -S git mingw-w64-ucrt-x86_64-{gcc,cmake,ninja,pkgconf,libplist,openssl} \
#     mingw-w64-ucrt-x86_64-gstreamer mingw-w64-ucrt-x86_64-gst-plugins-{base,good,bad} \
#     mingw-w64-ucrt-x86_64-gst-libav mingw-w64-ucrt-x86_64-glib-networking
#
# Output layout (what PhoneMirror expects):
#   engines/iphone/uxplay.exe, gst-inspect-1.0.exe, *.dll
#   engines/iphone/lib/gstreamer-1.0/*.dll        (only the plugins UxPlay uses)
#   engines/iphone/lib/gio/modules/*.dll          (TLS, for YouTube video casting)
#   engines/iphone/libexec/gstreamer-1.0/gst-plugin-scanner.exe
#   engines/iphone/THIRD_PARTY_MANIFEST.tsv       (every file -> MSYS2 package, version, license)
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/engines.env

PREFIX=${MINGW_PREFIX:-/ucrt64}
OUT=engines/iphone
SRC=${UXPLAY_SRC:-build/uxplay-src}

# ---------- 1. Build UxPlay at the pinned commit ----------
if [[ ! -d "$SRC/.git" ]]; then
  rm -rf "$SRC"
  git init -q "$SRC"
  git -C "$SRC" remote add origin "$UXPLAY_REPO"
fi
# Never convert line endings: the patches must match upstream byte for byte.
git -C "$SRC" config core.autocrlf false
git -C "$SRC" fetch -q --depth 1 origin "$UXPLAY_COMMIT"
git -C "$SRC" checkout -q --force FETCH_HEAD
echo "UxPlay at $(git -C "$SRC" rev-parse HEAD)"
# PhoneMirror's small patches (documented in THIRD_PARTY_LICENSES.md).
for patch in "$PWD"/scripts/patches/*.patch; do
  echo "Applying $(basename "$patch")"
  git -C "$SRC" apply --ignore-whitespace "$patch"
done

cmake -S "$SRC" -B "$SRC/build" -G Ninja -DCMAKE_BUILD_TYPE=Release -DNO_MARCH_NATIVE=ON
cmake --build "$SRC/build"

# ---------- 2. Copy programs and the plugins UxPlay needs ----------
rm -rf "$OUT"
mkdir -p "$OUT/lib/gstreamer-1.0" "$OUT/lib/gio/modules" "$OUT/libexec/gstreamer-1.0"
cp "$SRC/build/uxplay.exe" "$OUT/"
cp "$PREFIX/bin/gst-inspect-1.0.exe" "$OUT/"   # used by CI checks and diagnostics
scanner=$(find "$PREFIX/libexec" "$PREFIX/lib" -name 'gst-plugin-scanner.exe' 2>/dev/null | head -1)
[[ -n "$scanner" ]] && cp "$scanner" "$OUT/libexec/gstreamer-1.0/"

# Element → plugin, from UxPlay's pipelines (renderers/*.c) and Windows sinks:
#   appsrc→app, queue/filesink/fakesink→coreelements, decodebin/playbin→playback,
#   typefind→typefindfunctions, videoconvert/videoscale→videoconvertscale,
#   videoflip→videofilter, h264parse/h265parse→videoparsersbad, aacparse→audioparsers,
#   avdec_h264/avdec_aac/avdec_alac→libav, d3d11videosink/d3d11h264dec→d3d11,
#   d3d12videosink→d3d12, audioconvert, audioresample, volume, level,
#   autoaudiosink/autovideosink→autodetect, wasapisink→wasapi, wasapi2, directsoundsink,
#   mp4mux (-mp4 recording)→isomp4, jpegdec/imagefreeze (cover art), HLS casting
#   (-hls): hls, adaptivedemux2, soup, dash, tsdemux→mpegtsdemux, subparse.
PLUGINS=(
  coreelements app playback typefindfunctions videoconvertscale videoconvert videoscale
  videofilter videoparsersbad audioparsers libav d3d11 d3d12 audioconvert audioresample
  volume level autodetect wasapi wasapi2 directsound isomp4 jpeg imagefreeze
  hls adaptivedemux2 soup dash mpegtsdemux subparse rtp udp
)
missing=()
for p in "${PLUGINS[@]}"; do
  f="$PREFIX/lib/gstreamer-1.0/libgst$p.dll"
  if [[ -f "$f" ]]; then cp "$f" "$OUT/lib/gstreamer-1.0/"; else missing+=("$p"); fi
done
echo "Plugins not present in this GStreamer (fine if merged/renamed): ${missing[*]:-none}"
for m in "$PREFIX"/lib/gio/modules/libgiognutls.dll "$PREFIX"/lib/gio/modules/libgioopenssl.dll; do
  [[ -f "$m" ]] && cp "$m" "$OUT/lib/gio/modules/"
done

# ---------- 3. Pull in every DLL those depend on (recursively) ----------
prefix_win=$(cygpath -m "$PREFIX" | tr '[:upper:]' '[:lower:]')
for pass in 1 2 3 4 5 6; do
  added=0
  while IFS= read -r f; do
    while IFS= read -r dep; do
      base=$(basename "$dep")
      if [[ ! -f "$OUT/$base" ]]; then cp "$dep" "$OUT/"; added=$((added + 1)); fi
    done < <(ldd "$f" 2>/dev/null | awk '{print $3}' | grep -i "^$PREFIX/bin/\|^${prefix_win}/bin/" || true)
  done < <(find "$OUT" -name '*.exe' -o -name '*.dll')
  echo "dependency pass $pass: +$added DLLs"
  [[ $added -eq 0 ]] && break
done

# ---------- 4. Licence manifest + redistribution checks ----------
manifest="$OUT/THIRD_PARTY_MANIFEST.tsv"
printf 'file\tmsys2_package\tversion\tlicense\tsource\n' > "$manifest"
declare -A seen_pkg=()
while IFS= read -r f; do
  rel=${f#"$OUT"/}
  name=$(basename "$f")
  case "$rel" in
    uxplay.exe) printf '%s\t%s\t%s\t%s\t%s\n' "$rel" "UxPlay" "$UXPLAY_COMMIT" "GPL-3.0-or-later" "$UXPLAY_REPO" >> "$manifest"; continue ;;
    THIRD_PARTY_MANIFEST.tsv|*.txt) continue ;;
  esac
  # Find the original in the prefix to ask pacman who owns it.
  orig=$(find "$PREFIX/bin" "$PREFIX/lib/gstreamer-1.0" "$PREFIX/lib/gio/modules" "$PREFIX/libexec" -name "$name" 2>/dev/null | head -1)
  pkg=$( [[ -n "$orig" ]] && pacman -Qqo "$orig" 2>/dev/null || echo unknown)
  if [[ "$pkg" != unknown ]]; then
    ver=$(pacman -Q "$pkg" | cut -d' ' -f2)
    lic=$(pacman -Qi "$pkg" | sed -n 's/^Licenses *: //p')
    descfile=$(ls -d /var/lib/pacman/local/"$pkg"-"$ver"/desc 2>/dev/null || true)
    base=$( [[ -n "$descfile" ]] && awk '/^%BASE%/{getline; print; exit}' "$descfile" || echo "$pkg")
    src="https://repo.msys2.org/mingw/sources/${base}-${ver}.src.tar.zst"
    seen_pkg[$pkg]="$ver"
  else
    ver=?; lic=?; src=?
  fi
  printf '%s\t%s\t%s\t%s\t%s\n' "$rel" "$pkg" "$ver" "$lic" "$src" >> "$manifest"
done < <(find "$OUT" -type f | sort)

echo
echo "Bundled MSYS2 packages:"
for p in "${!seen_pkg[@]}"; do echo "  $p ${seen_pkg[$p]}"; done | sort

# Refuse to ship anything that isn't redistributable.
if grep -qiP '\t(proprietary|custom:nonfree|nonfree|unknown)\t' "$manifest"; then
  echo "ERROR: bundle contains files with a non-free or unknown licence:" >&2
  grep -iP '\t(proprietary|custom:nonfree|nonfree|unknown)\t' "$manifest" >&2
  exit 1
fi
for dll in "$OUT"/avcodec-*.dll; do
  [[ -f "$dll" ]] || continue
  conf=$(strings "$dll" | grep -m1 -- '--prefix=' || true)
  if grep -q -- '--enable-nonfree' <<<"$conf"; then
    echo "ERROR: $dll was built with --enable-nonfree and can't be redistributed" >&2
    exit 1
  fi
  echo "FFmpeg build: $(grep -o -- '--enable-gpl\|--enable-version3' <<<"$conf" | tr '\n' ' ')(no nonfree)"
done

du -sh "$OUT"
echo "UxPlay bundle ready in $OUT"
