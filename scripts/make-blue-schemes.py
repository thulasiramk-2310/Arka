#!/usr/bin/env python3
"""Make the blue ArkaOS colour schemes from the green ones.

    python3 scripts/make-blue-schemes.py

Reads ArkaOS.colors and ArkaOSLight.colors, writes ArkaOSBlue.colors and
ArkaOSLightBlue.colors next to them. Only the accent colours change (selection,
focus, hover, active text, the scheme's own accent). ForegroundPositive stays green:
it means "good", not "ArkaOS". The brand green maps to the blue below; every other
accent green keeps its saturation and brightness and turns by the same amount, so a
darker green stays a darker blue.
"""
import colorsys
import re
import sys
from pathlib import Path

GREEN = (77, 180, 145)
BLUE = (79, 134, 255)            # the ice blue of the logo artwork
ACCENT_KEYS = {"BackgroundAlternate", "BackgroundNormal", "DecorationFocus", "DecorationHover",
               "ForegroundActive", "accentColor", "activeBlend"}

def hue(rgb):
    return colorsys.rgb_to_hsv(*(c / 255 for c in rgb))[0] * 360

SHIFT = hue(BLUE) - hue(GREEN)

def blue_of(rgb):
    if rgb == GREEN:
        return BLUE
    h, s, v = colorsys.rgb_to_hsv(*(c / 255 for c in rgb))
    r, g, b = colorsys.hsv_to_rgb(((h * 360 + SHIFT) % 360) / 360, s, v)
    return tuple(round(c * 255) for c in (r, g, b))

def convert(text, scheme_id, name):
    out = []
    for line in text.splitlines():
        m = re.match(r"^(\w+)=(\d+),(\d+),(\d+)$", line)
        if m and m.group(1) in ACCENT_KEYS:
            rgb = tuple(int(x) for x in m.groups()[1:])
            if 100 <= hue(rgb) <= 200:           # only the greens
                line = "%s=%d,%d,%d" % ((m.group(1),) + blue_of(rgb))
        elif line.startswith("ColorScheme="):
            line = "ColorScheme=" + scheme_id
        elif line.startswith("Name="):
            line = "Name=" + name
        out.append(line)
    return "\n".join(out) + "\n"

root = Path(__file__).resolve().parents[1]
for src, scheme_id, name in (("ArkaOS.colors", "ArkaOSBlue", "ArkaOS Dark Blue"),
                             ("ArkaOSLight.colors", "ArkaOSLightBlue", "ArkaOS Light Blue")):
    text = (root / src).read_text()
    (root / (scheme_id + ".colors")).write_text(convert(text, scheme_id, name))
    print("wrote", scheme_id + ".colors")
