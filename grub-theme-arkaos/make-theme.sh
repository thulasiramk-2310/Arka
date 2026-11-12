#!/bin/sh
# Render the ArkaOS GRUB theme into $2 (default: ./out). Needs ImageMagick (`magick`) and
# grub2-mkfont (or grub-mkfont). $1 may name the font file; otherwise Inter is looked up with
# fontconfig. No usable font is a build error, never a silent fallback.
# The look follows the boot splash that comes right after it: the same dark ground and the same
# green logo, so GRUB -> splash -> login reads as one sequence.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
FONT=${1:-}
OUT=${2:-"$HERE/out"}
[ -n "$FONT" ] && [ -f "$FONT" ] || FONT=$(fc-match -f '%{file}' 'Inter' 2>/dev/null || true)
case "$FONT" in *[Ii]nter*) [ -f "$FONT" ] ;; *) echo "make-theme: Inter not found (got '$FONT')" >&2; exit 1 ;; esac
MKFONT=$(command -v grub2-mkfont || command -v grub-mkfont) || { echo "make-theme: no grub-mkfont" >&2; exit 1; }
mkdir -p "$OUT"
cp "$HERE/theme.txt" "$OUT/theme.txt"

# Fonts get a fixed family name, so theme.txt never depends on how a font file names itself.
"$MKFONT" -n "ArkaSans" -s 18 -o "$OUT/sans-18.pf2" "$FONT"
"$MKFONT" -n "ArkaSans" -s 14 -o "$OUT/sans-14.pf2" "$FONT"
"$MKFONT" -n "ArkaSansBold" -b -s 18 -o "$OUT/sans-bold-18.pf2" "$FONT"

# PNG32 everywhere: GRUB reads only 8-bit RGB/RGBA; a palette or grey PNG shows black with
# coloured fringes (seen in the QEMU preview).
# The logo, the splash's own artwork, at the size it is drawn (GRUB does not scale images well).
magick "$HERE/logo-src.png" -background none -filter Lanczos -resize 112x112 \
  -gravity center -extent 112x112 "PNG32:$OUT/logo.png"

# The selected entry: a soft rounded pill cut into nine pieces. GRUB stretches the edges and the
# centre and draws the edge pieces outside the entry. The left and right pieces are 16 px wide, so
# the text sits 16 px inside the pill.
PILL="$OUT/pill.png"
magick -size 40x24 xc:none -fill "#1d2229" -stroke "#2f3640" -strokewidth 1 \
  -draw "roundrectangle 0.5,0.5 39.5,23.5 8,8" "PNG32:$PILL"
crop() { magick "$PILL" -crop "$1" +repage "PNG32:$OUT/select_$2.png"; }
crop 16x8+0+0 nw;  crop 8x8+16+0 n;  crop 16x8+24+0 ne
crop 16x8+0+8 w;   crop 8x8+16+8 c;  crop 16x8+24+8 e
crop 16x8+0+16 sw; crop 8x8+16+16 s; crop 16x8+24+16 se
rm -f "$PILL"
echo "make-theme: wrote $OUT"
