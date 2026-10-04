#!/bin/sh
# Render the ArkaOS default wallpaper: night gradient, soft aurora light, the
# boot splash's nested triangles as a glowing motif, fine grain against
# banding. No text: the wallpaper is scenery, the UI carries the words.
# Usage: make-default.sh OUT.png [WIDTH HEIGHT]   (needs ImageMagick 7)
set -eu
OUT=$1
W=${2:-3840}
H=${3:-2160}
T=$(mktemp -d)
[ -n "${KEEP:-}" ] && echo "tmp: $T" || trap 'rm -rf "$T"' EXIT

# Work in thirds of the final size for the soft layers; they are smooth, so
# upscaling them costs nothing visible and keeps the build fast.
w=$((W / 3)); h=$((H / 3))

# 1. base: deep navy, slightly lighter toward the top-left
magick -size ${w}x${h} radial-gradient:"#101a33"-"#04060c" \
  -distort SRT "$((w / 3)),$((h / 4)) 1.9 0 $((w / 2)),$((h / 2))" "$T/base.png"

# 2. aurora: large colour fields, heavily blurred, screened over the base
blob() {  # colour cx cy rx ry  (in thirds-space)
  magick -size ${w}x${h} xc:none -fill "$1" -draw "ellipse $2,$3 $4,$5 0,360" \
    -blur 0x$((w / 9)) "$T/b$6.png"
}
blob "#0e7a5a" $((w * 30 / 100)) $((h * 78 / 100)) $((w * 34 / 100)) $((h * 16 / 100)) 1
blob "#1d3fae" $((w * 74 / 100)) $((h * 26 / 100)) $((w * 30 / 100)) $((h * 22 / 100)) 2
blob "#5b2bb8" $((w * 88 / 100)) $((h * 74 / 100)) $((w * 22 / 100)) $((h * 18 / 100)) 3
blob "#0b8fa8" $((w * 52 / 100)) $((h * 58 / 100)) $((w * 20 / 100)) $((h * 12 / 100)) 4
magick "$T/base.png" \
  \( "$T/b1.png" -channel A -evaluate multiply 0.55 +channel \) -compose Screen -composite \
  \( "$T/b2.png" -channel A -evaluate multiply 0.50 +channel \) -compose Screen -composite \
  \( "$T/b3.png" -channel A -evaluate multiply 0.42 +channel \) -compose Screen -composite \
  \( "$T/b4.png" -channel A -evaluate multiply 0.35 +channel \) -compose Screen -composite \
  -resize ${W}x${H}\! "$T/sky.png"

# 3. motif: nested triangles (boot splash proportions), centre-right, with bloom
cx=$((W * 69 / 100)); cy=$((H * 60 / 100))
tri() {  # scale strokewidth colour -> polygon around (cx,cy)
  s=$1
  ax=$cx; ay=$((cy - s * 62 / 100))
  bx=$((cx + s * 54 / 100)); by=$((cy + s * 38 / 100))
  cx2=$((cx - s * 54 / 100)); cy2=$by
  printf 'polygon %s,%s %s,%s %s,%s' $ax $ay $bx $by $cx2 $cy2
}
magick -size ${W}x${H} xc:none -fill none -stroke "#3ee6a8" \
  -strokewidth $((H / 540)) -draw "$(tri $((H * 34 / 100)))" \
  -stroke "#5fb8ff" -strokewidth $((H / 900)) -draw "$(tri $((H * 52 / 100)))" \
  -stroke "#8a7dff" -strokewidth $((H / 1200)) -draw "$(tri $((H * 74 / 100)))" \
  "$T/lines.png"
magick "$T/lines.png" -channel A -evaluate multiply 0.8 +channel "$T/lines-a.png"
magick "$T/lines.png" -resize 25% -morphology Dilate Disk:2 -blur 0x$((H / 240)) -resize ${W}x${H}\! \
  -channel A -evaluate multiply 1.4 +channel "$T/bloom.png"
# faint glass fill inside the inner triangle
magick -size ${W}x${H} xc:none -fill "#3ee6a8" -draw "$(tri $((H * 34 / 100)))" \
  -channel A -evaluate multiply 0.05 +channel "$T/fill.png"

# 4. compose, vignette, grain
magick "$T/sky.png" \
  "$T/fill.png" -compose Over -composite \
  "$T/bloom.png" -compose Screen -composite \
  "$T/lines-a.png" -compose Over -composite \
  \( -size ${W}x${H} xc:black \
     \( -size $((W * 14 / 10))x$((H * 14 / 10)) radial-gradient:black-white -gravity center -extent ${W}x${H} \
        -evaluate multiply 0.75 \) -alpha off -compose CopyOpacity -composite \) \
  -compose Over -composite \
  \( -size ${W}x${H} xc:gray50 -attenuate 0.12 +noise Gaussian -colorspace gray \) \
  -compose Overlay -composite \
  -depth 8 -strip "$OUT"
