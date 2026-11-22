#!/usr/bin/python3
"""Draw the ArkaOS boot splash pictures ("prism of light") and write arkaos.script next to them.

    make-assets.py SRC_DIR OUT_DIR

A spark of white light drifts in and lands on the tip of a dark glass crystal (the logo), the
crystal fills with light from the tip down, and a green beam leaves its base and sweeps across,
lighting ARKA letter by letter. Plymouth's script plugin can only move, scale, rotate and fade
pictures, so everything that needs a mask (the light filling the crystal, the glint crossing it)
is drawn here as a short run of frames, and arkaos.script crossfades between them.

Sizes are in units of a 1080-line screen. Pictures are drawn at A pixels a unit and arkaos.script
scales them to the real screen. The logo source is 184 px, so drawing larger would not be sharper.
The letters' angles from the beam's origin depend on the font, so they are measured here and
written into arkaos.script in place of its "// @LETTERS@" line.
"""
import math
import os
import shutil
import subprocess
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

A = 1.5                    # pixels a unit
S = 260                    # the crystal, units
FRAMES = 16                # light filling the crystal; the glint crossing it
WORD_DY = 165              # the word's centre below the screen's centre, units
CRYSTAL_DY = -105          # the crystal's centre
BEAM_DY = CRYSTAL_DY - S / 2 + 0.80 * S   # where the beam leaves the crystal
BEAM_LEN = 420

GREEN = np.array([0.30, 0.86, 0.62])
MINT = np.array([0.70, 1.0, 0.88])
HALO = np.array([0.35, 0.90, 0.65])
BANDC = np.array([0.85, 1.0, 0.94])
EDGEC = np.array([0.50, 0.78, 0.70])
INK = (236, 244, 240)
G_HEX = (77, 180, 145)


def px(u):
    return int(round(u * A))


def save(arr_rgb, alpha, path):
    rgba = np.dstack([np.clip(arr_rgb, 0, 1), np.clip(alpha, 0, 1)])
    Image.fromarray((rgba * 255 + 0.5).astype(np.uint8), "RGBA").save(path, optimize=True)


def light(L, path):
    """Save additive light as a picture that, laid over a dark screen, looks the same: alpha is the
    brightest channel and the colour is the light divided by it."""
    a = np.clip(L.max(-1), 0, 1)
    rgb = np.where(a[..., None] > 1e-4, L / np.maximum(a[..., None], 1e-4), 0)
    save(rgb, a, path)


def glow_dot(core, glow, halo=0.0, gk=0.45, hk=0.14):
    """A point of light: white core, mint glow, green halo (radii in units)."""
    R = (halo * 2.2 if hk else glow * 2.4) + 2
    n = px(2 * R) | 1
    c = (n - 1) / 2
    y, x = np.mgrid[0:n, 0:n].astype(float)
    r2 = ((x - c) ** 2 + (y - c) ** 2) / A ** 2
    L = np.exp(-r2 / core ** 2)[..., None] * np.array([1.0, 1.0, 1.0]) + (np.exp(-r2 / glow ** 2) * gk)[..., None] * MINT
    if hk:
        L = L + (np.exp(-r2 / halo ** 2) * hk)[..., None] * HALO
    return L


def inter_semibold(size):
    """Inter SemiBold from whichever Inter files fontconfig knows (Fedora ships the variable font)."""
    out = subprocess.run(["fc-list", ":", "file", "family"], capture_output=True, text=True).stdout
    files = sorted({l.partition(":")[0].strip() for l in out.splitlines()
                    if {f.strip() for f in l.partition(":")[2].split(",")} & {"Inter", "Inter Variable"}})
    files.sort(key=lambda f: (not f.endswith(".ttc"), "Variable" not in f, "Italic" in f))
    for path in files:
        if "Italic" in path:
            continue
        if path.endswith(".ttc"):
            for i in range(64):
                try:
                    f = ImageFont.truetype(path, size, index=i)
                except OSError:
                    break
                if f.getname()[0] in ("Inter", "Inter Variable") and f.getname()[1] == "SemiBold":
                    return f
        else:
            f = ImageFont.truetype(path, size)
            try:
                f.set_variation_by_name("SemiBold")
            except (OSError, ValueError):
                if f.getname()[1] != "SemiBold":
                    continue
            return f
    sys.exit("make-assets: Inter SemiBold not found")


def main():
    src, out = sys.argv[1:3]
    os.makedirs(out, exist_ok=True)
    sp = px(S)

    # ---- the crystal: dark glass, lit, the light filling it, the glint crossing it ----
    logo = Image.open(os.path.join(src, "logo-src.png")).convert("RGBA").resize((sp, sp), Image.LANCZOS)
    la = np.asarray(logo, float) / 255
    lit_rgb, a = la[..., :3], la[..., 3]
    edge = np.clip(a - np.asarray(logo.split()[3].filter(ImageFilter.MinFilter(5)), float) / 255, 0, 1)
    dark_rgb = lit_rgb.mean(-1, keepdims=True) * 0.20 + np.array([0.012, 0.026, 0.026]) + edge[..., None] * EDGEC * 0.30
    save(dark_rgb, a * 0.88, os.path.join(out, "dark.png"))
    save(lit_rgb, a, os.path.join(out, "lit.png"))

    yy = np.arange(sp, dtype=float)[:, None]
    for n in range(FRAMES):
        fill = (n + 1) / FRAMES
        front = -0.1 * sp + 1.2 * sp * fill
        m = np.clip((front - yy) / (0.08 * sp), 0, 1) * np.ones((1, sp))
        band = np.exp(-((yy - front) / (0.045 * sp)) ** 2) * (1 - max(0, n - 11) / 4) * np.ones((1, sp))
        rgb = dark_rgb * (1 - m[..., None]) + lit_rgb * m[..., None] + band[..., None] * BANDC
        save(rgb, a * (0.88 * (1 - m) + m), os.path.join(out, f"fill{n}.png"))
    gx, gy = np.meshgrid(np.arange(sp, dtype=float), np.arange(sp, dtype=float))
    diag = (gx + 0.45 * gy) / (1.45 * sp)
    for n in range(FRAMES):
        c = -0.25 + 1.5 * (n + 0.5) / FRAMES
        band = np.exp(-((diag - c) / 0.045) ** 2) * 0.5
        save(lit_rgb + band[..., None] * BANDC, a, os.path.join(out, f"glint{n}.png"))

    pad = int(sp * 0.38)
    g = Image.new("L", (sp + 2 * pad, sp + 2 * pad))
    g.paste(logo.split()[3], (pad, pad))
    ga = np.asarray(g.filter(ImageFilter.GaussianBlur(sp * 0.124)), float) / 255
    save(np.ones(ga.shape + (3,)) * GREEN, ga, os.path.join(out, "glow.png"))

    # ---- the spark, its trail, the faint line of its path, the flash where it lands ----
    light(glow_dot(3.0, 16.5, 45, 0.45, 0.14), os.path.join(out, "spark.png"))
    light(glow_dot(2.2, 7.2, 0, 0.32, 0) * 0.55, os.path.join(out, "trail.png"))
    light(glow_dot(1.5, 6.5, 0, 0.40, 0) * 0.20, os.path.join(out, "pathdot.png"))
    light(glow_dot(4.5, 27, 75, 0.5, 0.2), os.path.join(out, "flash.png"))

    # ---- the beam: drawn pointing down from the centre of a square, so rotating the square about
    # its centre turns the beam about the point it leaves the crystal ----
    n = px(2 * BEAM_LEN) | 1
    c = (n - 1) / 2
    y, x = np.mgrid[0:n, 0:n].astype(float)
    dx, dy = (x - c) / A, (y - c) / A
    d = np.hypot(dx, dy)
    ang = np.arctan2(dx, np.maximum(dy, 1e-6))
    I = np.exp(-(ang / 0.085) ** 2) * (1 - np.exp(-d / 45)) * np.exp(-d / 345) * np.clip(1 - (d / BEAM_LEN) ** 2, 0, 1) ** 2
    I[dy <= 0] = 0
    light(I[..., None] * np.array([0.40, 1.0, 0.72]) * 0.55, os.path.join(out, "beam.png"))

    # ---- the soft green light around the crystal once it is lit (dithered: no rings) ----
    n = px(340 * 2 * 1.6) // 2
    c = (n - 1) / 2
    y, x = np.mgrid[0:n, 0:n].astype(float)
    r2 = ((x - c) ** 2 + (y - c) ** 2) / (n / 2 / 1.6) ** 2
    al = np.exp(-r2) * 0.20 + (np.random.default_rng(3).random((n, n)) - 0.5) / 255
    save(np.ones((n, n, 3)) * np.array([0.15, 0.42, 0.31]), np.clip(al, 0, 1), os.path.join(out, "amb.png"))

    # ---- the word: one picture per letter, each on the whole word's canvas, so they all sit at
    # the same place; and a green glow for each ----
    f = inter_semibold(px(66))
    track = px(25.5)
    widths = [f.getlength(ch) for ch in "ARKA"]
    total = sum(widths) + track * 3
    cw, ch_ = int(total + px(80)), px(150)
    xx = (cw - total) / 2
    angles = []
    for i, (ch, w) in enumerate(zip("ARKA", widths)):
        img = Image.new("RGBA", (cw, ch_))
        ImageDraw.Draw(img).text((xx + w / 2, ch_ / 2), ch, font=f, fill=INK + (255,), anchor="mm")
        img.save(os.path.join(out, f"l{i}.png"), optimize=True)
        gl = np.asarray(img.split()[3].filter(ImageFilter.GaussianBlur(px(16.5))), float) / 255
        save(np.ones(gl.shape + (3,)) * GREEN, np.clip(gl * 1.6, 0, 1), os.path.join(out, f"lg{i}.png"))
        cx_units = (xx + w / 2 - cw / 2) / A
        angles.append(math.atan2(cx_units, WORD_DY - BEAM_DY))
        xx += w + track

    # ---- the passphrase box and its dots; the bar for system updates ----
    bw, bh = px(480), px(63)
    box = Image.new("RGBA", (bw, bh))
    ImageDraw.Draw(box).rounded_rectangle((1, 1, bw - 2, bh - 2), radius=px(15), fill=(14, 18, 20, 235),
                                          outline=(255, 255, 255, 46), width=max(1, px(1.5)))
    box.save(os.path.join(out, "entry.png"))
    dd = px(12)
    dimg = Image.new("RGBA", (dd * 4, dd * 4))
    ImageDraw.Draw(dimg).ellipse((0, 0, dd * 4 - 1, dd * 4 - 1), fill=G_HEX + (255,))
    dimg.resize((dd, dd), Image.LANCZOS).save(os.path.join(out, "dot.png"))
    Image.new("RGBA", (16, 6), G_HEX + (255,)).save(os.path.join(out, "bar.png"))
    Image.new("RGBA", (16, 6), (42, 52, 64, 255)).save(os.path.join(out, "barback.png"))

    # ---- arkaos.script with the measured letter angles, and the theme file ----
    script = open(os.path.join(src, "arkaos.script"), encoding="utf-8").read()
    marker = "// @LETTERS@"
    if script.count(marker) != 1:
        sys.exit("make-assets: arkaos.script must hold exactly one '// @LETTERS@' line")
    lines = "\n".join(f"letter[{i}].angle = {ang:.4f};" for i, ang in enumerate(angles))
    open(os.path.join(out, "arkaos.script"), "w", encoding="utf-8").write(script.replace(marker, lines))
    shutil.copy(os.path.join(src, "arkaos.plymouth"), os.path.join(out, "arkaos.plymouth"))


if __name__ == "__main__":
    main()
