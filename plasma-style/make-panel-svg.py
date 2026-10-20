#!/usr/bin/env python3
"""Generate the ArkaOS Plasma style's panel and dialog background SVGs.

    make-panel-svg.py OUT_DIR

Writes OUT_DIR/widgets/panel-background.svg and OUT_DIR/dialogs/background.svg
(translucent, used with KWin's blur), and the same under OUT_DIR/solid/
(opaque). Dialogs are every popup Plasma draws: notifications, the volume and
brightness popups, and the tray, calendar and launcher popups.

Both follow the ArkaOS design system: a raised surface with a 1px border and
a soft shadow, rounded corners for panels and corners cut at 45 degrees for
popups (see PANEL and DIALOG below). Plasma reads the shape, blur region and
shadow from these SVGs, so the frame, its alpha (the mask) and the shadow
tiles all use the same shape. Colours
come from the colour scheme's classes, so the same files work for Light and
Dark.

The shadow is rasterised (KWin draws it from image tiles anyway) with the
panel's own shape cut out, so it never shows through a translucent panel.
"""
import base64, io, os, sys
from PIL import Image, ImageDraw, ImageFilter, ImageChops

# Plasma gives every panel the same shadow tiles, the flat top bar as well as
# the floating dock, so the panel has a softer shadow than the design system's
# floating-surface token (0 8px 24px, 45%), which looked heavy under the bar.
# A Gaussian reaches about 2 sigma.
PANEL = dict(
    R=24,          # corner radius (design system: dock, launcher panel)
    HINT=8,        # content padding at the sides
    HINT_TB=7,     # content padding above and below (sizes the top bar's clock)
    THICK=True,    # also the thick-panel hints
    SH_Y=2, SH_SIGMA=6, SH_ALPHA=0.22,
    FILL=0.85,     # translucent version; the solid one is opaque
)
# Popups float over windows, so they take a stronger shadow, closer to the
# design system's floating surface. Their corners are cut at 45 degrees, like
# the peaks of the ArkaOS mark and wallpaper, which sets ArkaOS's popups apart
# from the rounded ones of other desktops (chosen 2026-10-20 from four looks).
DIALOG = dict(
    R=10,
    CORNER="cut",
    BORDER=0.16,
    HINT=10,
    HINT_TB=10,
    THICK=False,
    SH_Y=6, SH_SIGMA=10, SH_ALPHA=0.28,
    FILL=0.94,
)
C = 8           # size of the stretchable edge and centre pieces
BORDER = 0.12   # border opacity, as a fraction of the text colour
SS = 4          # supersampling for the shadow cut-out edge


def pad(st):
    spread = 2 * st["SH_SIGMA"]
    return {"top": spread - st["SH_Y"], "bottom": spread + st["SH_Y"], "left": spread, "right": spread}


def shadow_tiles(st):
    """Shadow of a (2R+C) square with corner size R, cut into eight tiles."""
    R, SH_Y, SH_SIGMA, SH_ALPHA, PAD = st["R"], st["SH_Y"], st["SH_SIGMA"], st["SH_ALPHA"], pad(st)
    w = h = 2 * R + C
    W, H = w + PAD["left"] + PAD["right"], h + PAD["top"] + PAD["bottom"]
    x0, y0 = PAD["left"], PAD["top"]

    def shape(dy=0, scale=1):
        im = Image.new("L", (W * scale, H * scale), 0)
        a, b = x0 * scale, (y0 + dy) * scale
        c, d = (x0 + w) * scale - 1, (y0 + dy + h) * scale - 1
        r = R * scale
        if st.get("CORNER") == "cut":
            ImageDraw.Draw(im).polygon([(a + r, b), (c - r, b), (c, b + r), (c, d - r),
                                        (c - r, d), (a + r, d), (a, d - r), (a, b + r)], fill=255)
        else:
            ImageDraw.Draw(im).rounded_rectangle([a, b, c, d], radius=r, fill=255)
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


def frame(st, fill_opacity):
    """Nine vector pieces of the frame (round or cut corners), laid out on a grid.

    The border is a filled 1px shape, not a stroke: QtSvg counts half the
    stroke width in an element's bounds, which left seams between pieces.
    """
    R = st["R"]
    out = []
    bo = f'class="ColorScheme-Text" style="fill:currentColor;fill-opacity:{st.get("BORDER", BORDER)};stroke:none"'
    bg = f'class="ColorScheme-Background" style="fill:currentColor;fill-opacity:{fill_opacity};stroke:none"'
    E = R + C  # start of the far corner column/row

    def piece(name, x, y, w, ht, body):
        # The transparent rect pins each element's bounds to its grid cell.
        out.append(f'<g id="{name}" transform="translate({x},{y})">'
                   f'<rect width="{w}" height="{ht}" style="fill:#000;fill-opacity:0;stroke:none"/>{body}</g>')

    # Top-left corner: quarter disc and a 1px ring along its arc. The other
    # corners are this one mirrored inside their R x R cell.
    if st.get("CORNER") == "cut":
        # A 45-degree cut instead of an arc, and a 1px band along it.
        k = 1.414
        corner = (f'<path {bg} d="M 0 {R} L {R} 0 L {R} {R} Z"/>'
                  f'<path {bo} d="M 0 {R} L {R} 0 L {R} {k} L {k} {R} Z"/>')
    else:
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


def hints(st, y):
    """Content padding (and thick-panel padding) and shadow extent hints."""
    HINT, HINT_TB, PAD = st["HINT"], st["HINT_TB"], pad(st)
    m = '<rect id="{}" x="{}" y="{}" width="{}" height="{}" style="fill:#ff00ff"/>'
    out = []
    for prefix in ("", "thick-") if st["THICK"] else ("",):
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


def svg(st, fill_opacity):
    grid = 2 * st["R"] + C
    hint_svg, y = hints(st, grid + 8)
    parts, x, row_h = [], 0, 0
    for name, (w, h, data) in shadow_tiles(st).items():
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
{frame(st, fill_opacity)}
{hint_svg}
{chr(10).join(parts)}
</svg>
'''


if __name__ == "__main__":
    out = sys.argv[1]
    for st, path in ((PANEL, "widgets/panel-background.svg"), (DIALOG, "dialogs/background.svg")):
        for sub, opacity in (("", st["FILL"]), ("solid", 1)):
            f = os.path.join(out, sub, path)
            os.makedirs(os.path.dirname(f), exist_ok=True)
            with open(f, "w") as fh:
                fh.write(svg(st, opacity))
