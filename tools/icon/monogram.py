"""Render the MiniMobile icon - a rounded white `m` with a cyan status light
on a deep teal tile - and write it for every build.

The design lives on a 108-unit tile, the same box the drafts were drawn in.
Everything is drawn at 16x and box-filtered down, so edges are smooth at
every size without a vector renderer.

    python3 tools/icon/monogram.py

writes, in place:

- Android: the adaptive layers (background, foreground, monochrome) on the
  108dp canvas at mdpi..xxxhdpi, and the composited square and round icons
  pre-26 launchers use. The tile's 108 units are fitted to the 72dp a
  launcher shows, so a masked icon looks like the tile.
- iOS: a 1024px opaque square in `ios/MiniMobile/Assets.xcassets`; iOS
  rounds it itself.
- Windows: `wie_portmaster/assets/minimobile.ico`, embedded in the exe.
- Windows and Linux windows: `wie_portmaster/assets/window_icon.rgba`, 64x64
  raw RGBA the program hands to SDL as its window icon.
- Linux and PortMaster: `linux/MiniMobile.png` and
  `portmaster/minimobile/MiniMobile.png`, shipped in their zips.

Pillow is the only dependency.
"""

import json
import math
import os

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

TILE = 108
SS = 16                              # supersampling factor
CORNER = 24                          # tile corner radius, in tile units

BG_FROM = (0x17, 0x3B, 0x46)         # top left
BG_TO = (0x0A, 0x17, 0x1C)           # bottom right
GLYPH = (255, 255, 255)
LED = (0x3F, 0xD5, 0xE3)

STROKE = 9                           # the m's stroke width, tile units
LED_AT, LED_R, HALO_R, HALO_ALPHA = (81, 29), 4, 7, 0.25


def m_centreline(step=0.25):
    """Points along the m's centre line, in tile units."""
    pts = []

    def line(x0, y0, x1, y1):
        n = max(1, int(math.hypot(x1 - x0, y1 - y0) / step))
        pts.extend((x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n) for i in range(n + 1))

    def arch(cx, cy, r):
        n = max(1, int(math.pi * r / step))
        pts.extend((cx - r * math.cos(math.pi * i / n), cy - r * math.sin(math.pi * i / n)) for i in range(n + 1))

    line(34, 72, 34, 46)
    arch(44, 46, 10)
    line(54, 46, 54, 72)
    arch(64, 46, 10)
    line(74, 46, 74, 72)
    return pts


def gradient(size, box):
    """The tile's diagonal gradient, running corner to corner of `box`
    (x0, y0, x1, y1 in pixels) and held at its end colours past it, so a
    layer bigger than the tile has no seam where the tile ends."""
    w, h = size
    x0, y0, x1, y1 = box
    g = Image.new("RGB", (w, h))
    px = g.load()
    span = (x1 - x0) + (y1 - y0)
    for y in range(h):
        for x in range(w):
            t = min(1.0, max(0.0, ((x + 0.5 - x0) + (y + 0.5 - y0)) / span))
            px[x, y] = tuple(round(a + (b - a) * t) for a, b in zip(BG_FROM, BG_TO))
    return g


def glyph(size, origin, scale, mono=False):
    """The m and its light on a transparent layer: tile unit u lands at
    origin + u * scale pixels."""
    k = SS
    img = Image.new("RGBA", (size * k, size * k), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    to = lambda u, v: ((origin[0] + u * scale) * k, (origin[1] + v * scale) * k)
    r = STROKE / 2 * scale * k
    for u, v in m_centreline():
        x, y = to(u, v)
        d.ellipse([x - r, y - r, x + r, y + r], fill=GLYPH + (255,))
    cx, cy = to(*LED_AT)
    if not mono:
        hr = HALO_R * scale * k
        halo = Image.new("RGBA", img.size, (0, 0, 0, 0))
        ImageDraw.Draw(halo).ellipse([cx - hr, cy - hr, cx + hr, cy + hr], fill=LED + (round(255 * HALO_ALPHA),))
        img = Image.alpha_composite(img, halo)
        d = ImageDraw.Draw(img)
    lr = LED_R * scale * k
    d.ellipse([cx - lr, cy - lr, cx + lr, cy + lr], fill=(GLYPH if mono else LED) + (255,))
    return img.resize((size, size), Image.BOX)


def tile(size, shape="rounded"):
    """The whole icon at `size` px: `rounded` (corners clear), `square`
    (opaque, for iOS) or `round` (a circle)."""
    scale = size / TILE
    art = gradient((size, size), (0, 0, size, size)).convert("RGBA")
    art = Image.alpha_composite(art, glyph(size, (0, 0), scale))
    if shape == "square":
        return art.convert("RGB")
    k = SS
    mask = Image.new("L", (size * k, size * k), 0)
    md = ImageDraw.Draw(mask)
    if shape == "round":
        md.ellipse([0, 0, size * k - 1, size * k - 1], fill=255)
    else:
        md.rounded_rectangle([0, 0, size * k - 1, size * k - 1], radius=CORNER * scale * k, fill=255)
    art.putalpha(mask.resize((size, size), Image.BOX))
    return art


# Android ------------------------------------------------------------------

RES = os.path.join(REPO, "android", "app", "src", "main", "res")
DENSITIES = [("mdpi", 1), ("xhdpi", 2), ("xxhdpi", 3), ("xxxhdpi", 4)]


def android():
    for folder, mult in DENSITIES:
        out = os.path.join(RES, f"mipmap-{folder}")
        os.makedirs(out, exist_ok=True)
        canvas = 108 * mult
        # The launcher shows the middle 72dp of the 108dp canvas; the tile
        # is fitted to that, so a masked icon reads as the tile itself.
        visible = 72 * mult
        inset = (canvas - visible) / 2
        scale = visible / TILE

        gradient((canvas, canvas), (inset, inset, inset + visible, inset + visible)).save(
            os.path.join(out, "ic_launcher_background.png"), optimize=True)
        glyph(canvas, (inset, inset), scale).save(os.path.join(out, "ic_launcher_foreground.png"), optimize=True)
        mono = glyph(canvas, (inset, inset), scale, mono=True)
        white = Image.new("RGBA", mono.size, (255, 255, 255, 0))
        white.putalpha(mono.split()[3])
        white.save(os.path.join(out, "ic_launcher_monochrome.png"), optimize=True)

        legacy = 48 * mult
        tile(legacy, "rounded").save(os.path.join(out, "ic_launcher.png"), optimize=True)
        tile(legacy, "round").save(os.path.join(out, "ic_launcher_round.png"), optimize=True)


# iOS ----------------------------------------------------------------------

def ios():
    catalog = os.path.join(REPO, "ios", "MiniMobile", "Assets.xcassets")
    icon = os.path.join(catalog, "AppIcon.appiconset")
    os.makedirs(icon, exist_ok=True)
    info = {"info": {"author": "xcode", "version": 1}}
    with open(os.path.join(catalog, "Contents.json"), "w") as f:
        json.dump(info, f, indent=2)
        f.write("\n")
    tile(1024, "square").save(os.path.join(icon, "AppIcon-1024.png"), optimize=True)
    contents = {
        "images": [{"filename": "AppIcon-1024.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"}],
        **info,
    }
    with open(os.path.join(icon, "Contents.json"), "w") as f:
        json.dump(contents, f, indent=2)
        f.write("\n")


# Windows, Linux and PortMaster -------------------------------------------

def desktop():
    assets = os.path.join(REPO, "wie_portmaster", "assets")
    os.makedirs(assets, exist_ok=True)
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = [tile(s, "rounded") for s in sizes]
    frames[-1].save(os.path.join(assets, "minimobile.ico"), sizes=[(s, s) for s in sizes], append_images=frames[:-1])
    with open(os.path.join(assets, "window_icon.rgba"), "wb") as f:
        f.write(tile(64, "rounded").tobytes())
    tile(256, "rounded").save(os.path.join(REPO, "linux", "MiniMobile.png"), optimize=True)
    tile(256, "rounded").save(os.path.join(REPO, "portmaster", "minimobile", "MiniMobile.png"), optimize=True)


if __name__ == "__main__":
    android()
    ios()
    desktop()
    print("wrote the Android, iOS, Windows, Linux and PortMaster icons")
