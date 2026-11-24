#!/usr/bin/env bash
# Render compositions and master them (two-pass loudness, true-peak limit) into artifacts/launch-out/.
#   video/capture/render.sh                 # the song cut: 16:9, 9:16, 1:1, teaser
#   video/capture/render.sh Song16x9        # just one
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=../artifacts/launch-out
mkdir -p "$OUT"
declare -A NAME=(
  [Song16x9]=song-16x9 [Song9x16]=song-9x16 [Song1x1]=song-1x1 [SongTeaser9x16]=song-teaser-9x16
  [Trailer16x9]=trailer-16x9 [Trailer9x16]=trailer-9x16 [Trailer1x1]=trailer-1x1 [TrailerTeaser9x16]=trailer-teaser-9x16
  [Launch16x9]=16x9 [Launch9x16]=9x16 [Launch1x1]=1x1 [Teaser9x16]=teaser-9x16
)
COMPS=("$@")
[ ${#COMPS[@]} -eq 0 ] && COMPS=(Song16x9 Song9x16 Song1x1 SongTeaser9x16)
for c in "${COMPS[@]}"; do
  n=${NAME[$c]}
  npx remotion render src/index.ts "$c" "$OUT/raw-$n.mp4" --codec=h264 --crf=16 --pixel-format=yuv420p --audio-codec=aac --overwrite --log=error
  python3 capture/master.py "$n"
done
