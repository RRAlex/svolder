#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# make screenshots: draw the README's pictures from the TUI's own renderer on the fake devices
# (the #[ignore] test tui::screenshots::generate), encode them into docs/images/, and say each
# file's size against its budget. A second run on the same code, the same ffmpeg and the same
# cached AutoEq file under target/screenshots/autoeq/ writes the same bytes.
# The one AutoEq file comes from AutoEq's repository, or from SVOLDER_SCREENSHOTS_AUTOEQ_URL (a
# mirror).
set -euo pipefail

root=$(dirname "$(dirname "$(readlink -f "$0")")")
out=$root/target/screenshots
images=$root/docs/images
cargo=${CARGO:-cargo}

missing=()
# Captured first: grep -q ending a pipe early would fail it under pipefail.
encoders=$(ffmpeg -hide_banner -encoders 2>/dev/null || true)
muxers=$(ffmpeg -hide_banner -muxers 2>/dev/null || true)
grep -q ' libaom-av1 ' <<<"$encoders" || missing+=("ffmpeg with libaom-av1 (package ffmpeg)")
grep -q ' libwebp ' <<<"$encoders" || missing+=("ffmpeg with libwebp (package ffmpeg)")
grep -q ' avif ' <<<"$muxers" || missing+=("ffmpeg with the avif muxer (package ffmpeg)")
any() { for f in "$@"; do [ -r "$f" ] && return 0; done; return 1; }
any "/usr/share/fonts/truetype/ubuntu/UbuntuSansMono[wght].ttf" || missing+=("Ubuntu Sans Mono (package fonts-ubuntu)")
any /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf /usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf /usr/share/fonts/TTF/DejaVuSans.ttf || missing+=("DejaVu Sans (package fonts-dejavu-core)")
any /usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf /usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf /usr/share/fonts/TTF/DejaVuSans-Bold.ttf || missing+=("DejaVu Sans Bold (package fonts-dejavu-core)")
if [ ${#missing[@]} -gt 0 ]; then
    printf 'screenshots: missing %s\n' "${missing[@]}" >&2
    exit 1
fi

scratch=$(mktemp -d)
# A failed run leaves no partial raw stream behind (it is gigabytes), and no half-finished output.
trap 'rm -rf "$scratch" "$out/svolder.rgb" "$out/encoded"' EXIT
started=$SECONDS
# Outputs of an earlier run are cleared first, so a run that draws nothing fails instead of
# encoding old frames; the downloaded correction under autoeq/ stays.
mkdir -p "$out"
find "$out" -mindepth 1 -maxdepth 1 ! -name autoeq -exec rm -rf {} +
env -u SVOLDER_MCP_TOKEN XDG_CONFIG_HOME="$scratch/config" XDG_STATE_HOME="$scratch/state" \
  XDG_DATA_HOME="$scratch/data" XDG_CACHE_HOME="$scratch/cache" \
  SVOLDER_WIREPLUMBER="$scratch/no-wireplumber" SVOLDER_HOST=off \
  "$cargo" test -q --manifest-path "$root/Cargo.toml" -p svolder --bin svolder -- \
  --ignored --exact tui::screenshots::generate --nocapture

# Everything is encoded into $out/encoded first and copied into docs/images/ only when every
# encode has succeeded, so a failed encode never leaves the pictures half new.
encoded=$out/encoded
mkdir -p "$encoded" "$images"
# Every call is bit-exact, input and output side, so a second run writes the same bytes.
still=(-v error -y -fflags +bitexact)
meta=(-map_metadata -1 -flags:v +bitexact -fflags +bitexact)

# The stills are lossless WebP. Measured on 2026-10-09 for the five stills: WebP lossless
# (-compression_level 6) 195 KB, PNG (-pred mixed) 591 KB. A lossless encoder takes no -crf; if the
# pre-commit hook refuses a still over a byte run in the compressed data, move -compression_level
# to 5 and say so here.
for name in dac amp tone telemetry devices; do
    ffmpeg "${still[@]}" -i "$out/$name.ppm" "${meta[@]}" \
      -c:v libwebp -lossless 1 -compression_level 6 "$encoded/$name.webp"
done

# The animation is AVIF over AV1 (libaom), 4:4:4, -crf 30, -cpu-used 4. Measured on 2026-10-09 for
# 304 frames of 1920x1360 (before the 24 px margin; now 1968x1408): 4:2:0 646 KB, 4:4:4 784 KB.
# 4:4:4 keeps the thin coloured rules and text edges of the Tone screen free of the pink fringe
# 4:2:0 leaves, and stays far under the 2 MB budget, so crf 30 is kept. -auto-alt-ref 0 is needed:
# with libaom's alt-ref frames on, this content makes the encoder abort on an assertion. One
# thread, no row-mt, one tile: the same bytes on every run. The ground, #070605 in the 24 px margin
# and at the picture's corner, decodes within one level.
read -r width height _ < "$out/svolder.rgb.size"
ffmpeg "${still[@]}" -f rawvideo -pix_fmt rgb24 -video_size "${width}x${height}" -framerate 30 \
  -i "$out/svolder.rgb" "${meta[@]}" -threads 1 \
  -vf scale=out_color_matrix=bt709:out_range=full,format=yuv444p -colorspace bt709 \
  -color_primaries bt709 -color_trc iec61966-2-1 -color_range pc -c:v libaom-av1 -crf 30 \
  -b:v 0 -cpu-used 4 -row-mt 0 -tiles 1x1 -auto-alt-ref 0 -g 300 -loop 0 -f avif \
  "$encoded/svolder.avif"

cp "$encoded"/*.webp "$encoded/svolder.avif" "$images/"
for name in dac amp tone telemetry devices svolder; do
    cp "$out/$name.txt" "$images/$name.txt"
done

check() {
    local file=$1 budget=$2 size
    size=$(stat -c %s "$images/$file")
    if [ "$size" -gt "$budget" ]; then
        printf 'over budget: %s is %d bytes, the budget is %d\n' "$file" "$size" "$budget"
    else
        printf 'ok: %s is %d bytes, the budget is %d\n' "$file" "$size" "$budget"
    fi
}
for name in dac amp tone telemetry devices; do
    check "$name.webp" 300000
done
check svolder.avif 2000000
check svolder.txt 200000

rm -f "$out/svolder.rgb"
printf 'screenshots: done in %d s: %s\n' "$((SECONDS - started))" "$images"
