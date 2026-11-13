#!/bin/sh
# Render the ArkaOS boot splash images into the current directory.
# Everything is drawn at 2x; arkaos.script scales it to the screen, so the
# splash stays sharp on HiDPI panels and is downscaled on small ones.
# Needs ImageMagick (`magick`). $1 may name a font file; otherwise Liberation
# Sans Bold is looked up with fontconfig (its path differs between Fedora
# releases). No usable font is a build error, never a silent fallback.
set -eu
FONT=${1:-}
# The wordmark is set in Inter, the face ArkaOS uses everywhere else (rsms-inter-fonts is in the image);
# Liberation Sans Bold only where Inter is missing. No usable font is a build error.
if [ -z "$FONT" ] || [ ! -f "$FONT" ]; then
  FONT=$(fc-match -f '%{file}' 'Inter:bold' 2>/dev/null || true)
  case "$FONT" in *[Ii]nter*) ;; *) FONT=$(fc-match -f '%{file}' 'Liberation Sans:bold' 2>/dev/null || true) ;; esac
fi
case "$FONT" in
  *[Ii]nter*|*LiberationSans-Bold*) [ -f "$FONT" ] ;;
  *) echo "make-assets: neither Inter nor Liberation Sans Bold was found (got '$FONT')" >&2; exit 1 ;;
esac
G="#4db491"

# The logo (logo-src.png, the same green artwork as the start button) sits where the old
# triangles were. The splash is always green: it is drawn before anyone has signed in, so it
# cannot know an account's accent. The file names stay tri1..tri3 so arkaos.script is unchanged:
# tri1 is the logo, tri2 is empty (its staggered fade becomes a no-op), tri3 is a soft glow cut from
# the logo's own outline that the script pulses.
magick logo-src.png -background none -filter Lanczos -resize x372 -gravity center -extent 520x460 tri1.png
magick -size 520x460 xc:none tri2.png
magick tri1.png -channel A -blur 0x14 +channel -fill "$G" -colorize 100 \
  -channel A -evaluate multiply 0.85 +channel tri3.png
magick -size 800x140 xc:none -font "$FONT" -fill "$G" -pointsize 88 \
  -kerning 28 -gravity center -draw "text 0,0 'ARKA'" wordmark.png

# The opening, drawn ahead of time and played by arkaos.script (Plymouth can't mask while it runs):
# 1. streak0..17: a blade of light crosses the logo diagonally and leaves the logo behind it.
#    The last frame is the logo alone, so the script hands over to tri1.png without a jump.
# The blade fades out towards every edge of the frame, so it never shows a cut-off end.
magick -size 520x460 xc:white -alpha set -channel A -fx 'pow(sin(pi*j/h),1.6)*pow(sin(pi*i/w),0.7)' +channel vmask.png
i=0; N=18
while [ $i -lt $N ]; do
  x=$(awk -v i=$i -v n=$N 'BEGIN { t = i / (n - 1); e = 1 - (1 - t) ^ 3; printf "%d", -180 + e * 900 }')
  if [ $i -lt $((N - 1)) ]; then
    magick -size 520x460 xc:none -fill white -draw "polygon -400,0 $((x + 90)),0 $((x - 90)),460 -400,460" -blur 0x6 reveal.png
    magick tri1.png reveal.png -compose DstIn -composite revealed.png
    magick -size 520x460 xc:none -fill "rgba(77,180,145,0.75)" -draw "polygon $((x + 60)),0 $((x + 120)),0 $((x - 60)),460 $((x - 120)),460" -blur 0x22 glow.png
    magick -size 520x460 xc:none -fill "rgba(232,255,244,1)" -draw "polygon $((x + 86)),0 $((x + 94)),0 $((x - 86)),460 $((x - 94)),460" -blur 0x3 core.png
    magick glow.png core.png -compose Plus -composite vmask.png -compose DstIn -composite blade.png
    magick revealed.png blade.png -compose Plus -composite "streak$i.png"
  else
    cp tri1.png "streak$i.png"
  fi
  i=$((i + 1))
done
rm -f reveal.png revealed.png glow.png core.png vmask.png blade.png

# 2. word0..21: the wordmark decodes from scrambled characters, left to right. The last frame is
#    the finished wordmark above.
decode() { # text width height pointsize kerning colour prefix frames
  T=$1; P=$7; NF=$8; k=0
  while [ $k -lt "$NF" ]; do
    if [ $k -eq $((NF - 1)) ]; then S=$T; else
      S=$(awk -v t="$T" -v k=$k -v nf="$NF" 'BEGIN {
        srand(k * 7919 + 13); cs = "ABCDEFGHKMNPRSTUVXYZ0123456789#/<>"; L = length(t)
        done = int((k / (nf - 1)) * (L + 3)) - 2; out = ""
        for (j = 1; j <= L; j++) { c = substr(t, j, 1)
          if (c == " " || j <= done) out = out c; else out = out substr(cs, int(rand() * length(cs)) + 1, 1) }
        print out }'); fi
    magick -size "$2x$3" xc:none -font "$FONT" -fill "$6" -pointsize "$4" -kerning "$5" -gravity center \
      -draw "text 0,0 '$S'" "$P$k.png"
    k=$((k + 1))
  done
}
decode "ARKA" 800 140 88 28 "$G" word 22

# Activity / progress pieces (stretched by the script).
magick -size 16x6 xc:"$G" bar.png
magick -size 16x6 xc:"#2a3440" barback.png
# Soft-edged segment for the indeterminate activity indicator.
magick -size 6x80 gradient:none-"$G" -rotate -90 seg-l.png
magick seg-l.png -flop seg-r.png
magick seg-l.png seg-r.png +append seg.png
rm -f seg-l.png seg-r.png

# Passphrase box: rounded outline the bullets sit in.
magick -size 760x96 xc:none -fill "#0f141c" -stroke "#2a3440" -strokewidth 3 \
  -draw "roundrectangle 2,2 757,93 20,20" entry.png
# One passphrase dot, drawn as an image: the boot font may have no bullet
# character (it showed as empty boxes in the VM, 2026-10-18).
magick -size 24x24 xc:none -fill "$G" -draw "circle 12,12 12,3" dot.png
