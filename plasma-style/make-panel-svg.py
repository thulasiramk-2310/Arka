#!/usr/bin/env python3
"""Generate the ArkaOS Plasma style's panel-background SVGs.

    make-panel-svg.py OUT_DIR

Writes OUT_DIR/widgets/panel-background.svg (translucent, used with KWin's
blur) and OUT_DIR/solid/widgets/panel-background.svg (opaque).

The panel follows the ArkaOS design system: a raised surface with 24px
corners and a 1px border, plus a soft shadow (see SH_* below). Plasma reads the panel's shape, blur region and shadow from this SVG,
so the frame, its alpha (the mask) and the shadow tiles all use the same
rounded rectangle. Colours come from the colour scheme's classes, so the same
file works for Light and Dark.

The shadow is rasterised (KWin draws it from image tiles anyway) with the
panel's own shape cut out, so it never shows through a translucent panel.
"""
import base64, io, os, sys
from PIL import Image, ImageDraw, ImageFilter, ImageChops

R = 24          # corner radius (design system: dock, launcher panel)
C = 8           # size of the stretchable edge and centre pieces
HINT = 8        # content padding at the sides
HINT_TB = 7     # content padding above and below (sizes the top bar's clock)
BORDER = 0.12   # border opacity, as a fraction of the text colour

# Plasma gives every panel the same shadow tiles, the flat top bar as well as
# the floating dock, so this is a softer shadow than the design system's
# floating-surface token (0 8px 24px, 45%), which looked heavy under the bar.
# A Gaussian reaches about 2 sigma.
SH_Y, SH_SIGMA, SH_ALPHA = 2, 6, 0.22
SPREAD = 2 * SH_SIGMA
PAD = {"top": SPREAD - SH_Y, "bottom": SPREAD + SH_Y, "left": SPREAD, "right": SPREAD}
SS = 4          # supersampling for the shadow cut-out edge


def shadow_tiles():
    """Shadow of a (2R+C) square with radius R, cut into eight tiles."""
    w = h = 2 * R + C
    W, H = w + PAD["left"] + PAD["right"], h + PAD["top"] + PAD["bottom"]
    x0, y0 = PAD["left"], PAD["top"]

    def shape(dy=0, scale=1):
        im = Image.new("L", (W * scale, H * scale), 0)
        ImageDraw.Draw(im).rounded_rectangle(
            [x0 * scale, (y0 + dy) * scale, (x0 + w) * scale - 1, (y0 + dy + h) * scale - 1],
            radius=R * scale, fill=255)
        return im if scale == 1 else im.resize((W, H), Image.LANCZOS)

    blur = shape(SH_Y).filter(ImageFilter.GaussianBlur(SH_SIGMA))
    blur = blur.point(lambda v: round(v * SH_ALPHA))
    # Cut the panel's own (antialiased) shape out of the shadow.
    hole = shape(0, SS)
    alpha = ImageChops.multiply(blur, ImageChops.invert(hole))
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    img.putalpha(alpha)

    L, T = x0 + R, y0 + R            # inner corner points
    Rx, By = x0 + R + C, y0 + R + C
    boxes = {
        "shadow-topleft": (0, 0, L, T),
        "shadow-top": (L, 0, Rx, T),
        "shadow-topright": (Rx, 0, W, T),
        "shadow-left": (0, T, L, By),
        "shadow-right": (Rx, T, W, By),
        "shadow-bottomleft": (0, By, L, H),
        "shadow-bottom": (L, By, Rx, H),
        "shadow-bottomright": (Rx, By, W, H),
    }
    tiles = {}
    for name, box in boxes.items():
        buf = io.BytesIO()
        img.crop(box).save(buf, "PNG", optimize=True)
        tiles[name] = (box[2] - box[0], box[3] - box[1], base64.b64encode(buf.getvalue()).decode())
    return tiles


def frame(fill_opacity):
    """Nine vector pieces of the rounded frame, laid out on a grid.

    The border is a filled 1px shape, not a stroke: QtSvg counts half the
    stroke width in an element's bounds, which left seams between pieces.
    """
    out = []
    bo = f'class="ColorScheme-Text" style="fill:currentColor;fill-opacity:{BORDER};stroke:none"'
    bg = f'class="ColorScheme-Background" style="fill:currentColor;fill-opacity:{fill_opacity};stroke:none"'
    E = R + C  # start of the far corner column/row

    def piece(name, x, y, w, ht, body):
        # The transparent rect pins each element's bounds to its grid cell.
        out.append(f'<g id="{name}" transform="translate({x},{y})">'
                   f'<rect width="{w}" height="{ht}" style="fill:#000;fill-opacity:0;stroke:none"/>{body}</g>')

    # Top-left corner: quarter disc and a 1px ring along its arc. The other
    # corners are this one mirrored inside their R x R cell.
    corner = (f'<path {bg} d="M 0 {R} A {R} {R} 0 0 1 {R} 0 L {R} {R} Z"/>'
              f'<path {bo} d="M 0 {R} A {R} {R} 0 0 1 {R} 0 L {R} 1 '
              f'A {R - 1} {R - 1} 0 0 0 1 {R} Z"/>')
    mirror = lambda sx, sy: (f'<g transform="matrix({sx},0,0,{sy},{R if sx < 0 else 0},{R if sy < 0 else 0})">'
                             f'{corner}</g>')
    edge = lambda w, ht, bx, by, bw, bh: (f'<rect {bg} width="{w}" height="{ht}"/>'
                                          f'<rect {bo} x="{bx}" y="{by}" width="{bw}" height="{bh}"/>')
    piece("topleft", 0, 0, R, R, corner)
    piece("top", R, 0, C, R, edge(C, R, 0, 0, C, 1))
    piece("topright", E, 0, R, R, mirror(-1, 1))
    piece("left", 0, R, R, C, edge(R, C, 0, 0, 1, C))
    piece("center", R, R, C, C, f'<rect {bg} width="{C}" height="{C}"/>')
    piece("right", E, R, R, C, edge(R, C, R - 1, 0, 1, C))
    piece("bottomleft", 0, E, R, R, mirror(1, -1))
    piece("bottom", R, E, C, R, edge(C, R, 0, R - 1, C, 1))
    piece("bottomright", E, E, R, R, mirror(-1, -1))
    return "\n".join(out)


def hints(y):
    """Content padding (normal and thick panels) and shadow extent hints."""
    m = '<rect id="{}" x="{}" y="{}" width="{}" height="{}" style="fill:#ff00ff"/>'
    out = []
    for prefix in ("", "thick-"):
        out += [m.format(prefix + "hint-top-margin", 0, y, 1, HINT_TB),
                m.format(prefix + "hint-bottom-margin", 2, y, 1, HINT_TB),
                m.format(prefix + "hint-left-margin", 4, y, HINT, 1),
                m.format(prefix + "hint-right-margin", 4, y + 2, HINT, 1)]
        y += HINT + 4
    out += [m.format("shadow-hint-top-margin", 0, y, 1, PAD["top"]),
            m.format("shadow-hint-bottom-margin", 2, y, 1, PAD["bottom"]),
            m.format("shadow-hint-left-margin", 4, y, PAD["left"], 1),
            m.format("shadow-hint-right-margin", 4, y + 2, PAD["right"], 1)]
    y += max(PAD.values()) + 4
    # Stretch the edge pieces instead of tiling them (tiling left seams).
    out.append(m.format("hint-stretch-borders", 0, y, 1, 1))
    return "\n".join(out), y + 4


def svg(fill_opacity):
    grid = 2 * R + C
    hint_svg, y = hints(grid + 8)
    parts, x, row_h = [], 0, 0
    for name, (w, h, data) in shadow_tiles().items():
        parts.append(f'<image id="{name}" x="{x}" y="{y}" width="{w}" height="{h}" '
                     f'xlink:href="data:image/png;base64,{data}"/>')
        x += w + 4
        row_h = max(row_h, h)
    width, height = max(x, grid), y + row_h
    return f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<!-- Generated by plasma-style/make-panel-svg.py; edit that script, not this file. -->
<defs><style id="current-color-scheme" type="text/css">
.ColorScheme-Text {{ color:#232629; }}
.ColorScheme-Background {{ color:#eff0f1; }}
</style></defs>
{frame(fill_opacity)}
{hint_svg}
{chr(10).join(parts)}
</svg>
'''


if __name__ == "__main__":
    out = sys.argv[1]
    for sub, opacity in (("widgets", 0.85), ("solid/widgets", 1)):
        os.makedirs(os.path.join(out, sub), exist_ok=True)
        with open(os.path.join(out, sub, "panel-background.svg"), "w") as f:
            f.write(svg(opacity))
