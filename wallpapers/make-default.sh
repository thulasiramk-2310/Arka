#!/bin/sh
# Render an ArkaOS default wallpaper: a quiet, nearly flat field with the Arka
# triangle drawn as three hairlines, cropped off the right edge. Only the inner
# line carries the brand green. No glow, no gradients to speak of, no text:
# the wallpaper is scenery, the UI carries the words. Fine grain prevents
# banding on 8-bit panels.
# Usage: make-default.sh OUT.png [WIDTH HEIGHT [light|dark]]   (ImageMagick 7)
set -eu
OUT=$1
W=${2:-3840}
H=${3:-2160}
LOOK=${4:-dark}

if [ "$LOOK" = light ]; then
  TOP="#eceff3"; BOTTOM="#e4e8ed"; LINE="#c9d0d9"; GREEN_A=0.70
else
  TOP="#17191d"; BOTTOM="#121418"; LINE="#2c3038"; GREEN_A=0.60
fi

# Geometry is designed on a 1440x900 canvas and scaled; the apex sits at
# x=1180 and the bases run past the bottom edge.
sx() { echo $(( $1 * W / 1440 )); }
sy() { echo $(( $1 * H / 900 )); }
tri() {  # apex-y half-width -> polygon
  printf 'polygon %s,%s %s,%s %s,%s' \
    "$(sx 1180)" "$(sy "$1")" "$(sx $((1180 + $2)))" "$(sy 930)" "$(sx $((1180 - $2)))" "$(sy 930)"
}
SW=$(( H / 900 + 1 ))

magick -size "${W}x${H}" gradient:"$TOP"-"$BOTTOM" \
  -fill none -stroke "$LINE" -strokewidth "$SW" \
  -draw "$(tri 120 467)" -draw "$(tri 250 392)" \
  \( -size "${W}x${H}" xc:none -fill none -stroke "#16c784" -strokewidth "$SW" \
     -draw "$(tri 380 317)" -channel A -evaluate multiply "$GREEN_A" +channel \) \
  -compose Over -composite \
  \( -size "${W}x${H}" xc:gray50 -attenuate 0.05 +noise Gaussian -colorspace gray \) \
  -compose Overlay -composite \
  -depth 8 -strip "$OUT"
