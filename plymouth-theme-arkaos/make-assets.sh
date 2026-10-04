#!/bin/sh
# Render the ArkaOS boot splash images into the current directory.
# Everything is drawn at 2x; arkaos.script scales it to the screen, so the
# splash stays sharp on HiDPI panels and is downscaled on small ones.
# Needs ImageMagick (`magick`). $1 may name a font file; otherwise Liberation
# Sans Bold is looked up with fontconfig (its path differs between Fedora
# releases). No usable font is a build error, never a silent fallback.
set -eu
FONT=${1:-}
if [ -z "$FONT" ] || [ ! -f "$FONT" ]; then
  FONT=$(fc-match -f '%{file}' 'Liberation Sans:bold' 2>/dev/null || true)
fi
case "$FONT" in
  *LiberationSans-Bold*) [ -f "$FONT" ] ;;
  *) echo "make-assets: Liberation Sans Bold not found (got '$FONT')" >&2; exit 1 ;;
esac
G="#16c784"

magick -size 520x460 xc:none -fill none -stroke "$G" -strokewidth 10 \
  -draw "polygon 260,70 410,350 110,350" tri1.png
magick -size 520x460 xc:none -fill none -stroke "$G" -strokewidth 6 \
  -draw "polygon 260,24 456,392 64,392" tri2.png
magick -size 520x460 xc:none -fill none -stroke "$G" -strokewidth 16 \
  -draw "polygon 260,24 456,392 64,392" -blur 0x12 tri3.png
magick -size 800x140 xc:none -font "$FONT" -fill "$G" -pointsize 88 \
  -kerning 28 -gravity center -draw "text 0,0 'ARKA'" wordmark.png

# Activity / progress pieces (stretched by the script).
magick -size 16x6 xc:"$G" bar.png
magick -size 16x6 xc:"#1e2630" barback.png
# Soft-edged segment for the indeterminate activity indicator.
magick -size 6x80 gradient:none-"$G" -rotate -90 seg-l.png
magick seg-l.png -flop seg-r.png
magick seg-l.png seg-r.png +append seg.png
rm -f seg-l.png seg-r.png

# Passphrase box: rounded outline the bullets sit in.
magick -size 760x96 xc:none -fill "#0f141c" -stroke "#2a3440" -strokewidth 3 \
  -draw "roundrectangle 2,2 757,93 20,20" entry.png
