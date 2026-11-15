#!/bin/sh
# Build the icon set for both built-in themes from Phosphor Icons (MIT), one source and two
# weights by role: "fill" for the app and menu glyphs that have a solid shape, "bold" for line
# glyphs and every small control. All are neutral white-to-silver with a faint tint of the theme;
# green is kept for the files this script does not touch (shield-green, check-green).
# Usage: tools/icons.sh   (needs curl; run from shell-dawn/)
set -eu
VERSION=2.1.1
CACHE=${XDG_CACHE_HOME:-$HOME/.cache}/arka-desk-icons/$VERSION
mkdir -p "$CACHE"
FILL="shopping-bag gear cube folder squares-four shield"
BOLD="globe code terminal power arrow-right arrow-left magnifying-glass sliders-horizontal speaker-high sun \
wifi-high wifi-slash bluetooth bell bell-slash palette clipboard-text trash x cpu memory hard-drive \
thermometer-simple arrows-clockwise caret-right battery-high battery-low battery-charging network lock \
sign-out arrow-clockwise seal-check"
fetch() { # name weight
    file="$CACHE/$1-$2.svg"
    [ -s "$file" ] || curl -fsS -m 30 -o "$file" "https://unpkg.com/@phosphor-icons/core@$VERSION/assets/$2/$1-$2.svg"
    echo "$file"
}
tint() { # source tone destination
    path=$(sed -n 's/.*<svg[^>]*>\(.*\)<\/svg>.*/\1/p' "$1")
    printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="url(#g)"><defs><linearGradient id="g" x1="0" y1="0" x2="0.6" y2="1"><stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="%s"/></linearGradient></defs>%s</svg>\n' "$2" "$path" > "$3"
}
for theme_tone in "sky-blue #B4C8E8" "emerald #C4D8CE"; do
    theme=${theme_tone% *}; tone=${theme_tone#* }
    for name in $FILL; do tint "$(fetch "$name" fill)" "$tone" "assets/$theme/$name-fill.svg"; done
    for name in $BOLD; do tint "$(fetch "$name" bold)" "$tone" "assets/$theme/$name-bold.svg"; done
done
echo "icons written for sky-blue and emerald"
