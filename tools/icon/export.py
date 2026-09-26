"""Export the pixel-art launcher icon as Android adaptive-icon layers.

The art is authored once on a 108-pixel grid - the adaptive icon's own
108dp canvas, one art pixel per dp - and every density is an integer
nearest-neighbour multiple of it, so a pixel stays a square. hdpi would be
1.5x and is left out on purpose: Android falls back to xhdpi and scales
down, which beats shipping a layer whose pixels are alternately one and two
device pixels wide.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
OUT = os.environ.get("ICON_OUT", os.path.join(HERE, "preview"))
os.makedirs(OUT, exist_ok=True)
sys.path.insert(0, HERE)


from PIL import Image, ImageDraw
import pixel as P

RES = os.path.join(REPO, "android", "app", "src", "main", "res")
GRID = 108                     # one art pixel per dp
DENSITIES = [("mdpi", 1), ("xhdpi", 2), ("xxhdpi", 3), ("xxxhdpi", 4)]


def snap_rgb(img, colours=24):
    return img.convert("RGB").quantize(palette=P.palette_image(colours), dither=Image.NONE).convert("RGB")


def pixel_layers():
    """The background and foreground layers, on the 108 grid."""
    phone = P.render_pose(soft_shadow=False)

    bg = P.background((38, 48, 66), (11, 15, 24))
    bg_small = snap_rgb(bg.resize((GRID, GRID), Image.BOX))

    fg_small = phone.resize((GRID, GRID), Image.BOX)
    alpha = fg_small.split()[3].point(lambda a: 255 if a >= 128 else 0)
    fg_small = snap_rgb(fg_small).convert("RGBA")
    fg_small.putalpha(alpha)

    # The silhouette a themed icon is tinted from. One flat shape reads as a
    # featureless slab under the system tint, so the dial is punched through
    # it - the phone is then still a phone with a dial on it.
    body = P.render_pose(soft_shadow=False).split()[3].resize((GRID, GRID), Image.BOX)
    body = body.point(lambda a: 255 if a >= 128 else 0)
    hole = P.render_dial_mask().resize((GRID, GRID), Image.BOX).point(lambda a: 255 if a >= 128 else 0)
    cut = Image.composite(Image.new("L", (GRID, GRID), 0), body, hole)
    mono = Image.new("RGBA", (GRID, GRID), (255, 255, 255, 0))
    mono.paste((255, 255, 255, 255), (0, 0), cut)

    return bg_small, fg_small, mono


def write(layer, name):
    for folder, mult in DENSITIES:
        out = f"{RES}/mipmap-{folder}"
        os.makedirs(out, exist_ok=True)
        big = layer.resize((GRID * mult, GRID * mult), Image.NEAREST)
        big.save(f"{out}/{name}.png", optimize=True)


# API 24 and 25 have no adaptive icons and need one composited square. Its
# own 48-grid master keeps the legacy sizes (48/96/144/192) integer multiples
# too; drawn from the 108 grid they would land on thirds and smear.
LEGACY_GRID = 48


def legacy(bg, fg):
    """The composited square and round icons for pre-26 launchers."""
    comp = Image.alpha_composite(bg.convert("RGBA"), fg)
    m = (GRID - 72) // 2
    vis = comp.crop((m, m, GRID - m, GRID - m)).resize((LEGACY_GRID, LEGACY_GRID), Image.BOX)
    vis = snap_rgb(vis).convert("RGBA")

    def masked(draw_fn):
        out = vis.copy()
        out.putalpha(P.shape_mask(vis.size, draw_fn))
        return out

    n = LEGACY_GRID - 1
    square = masked(lambda d: d.rounded_rectangle([0, 0, n, n], radius=int(LEGACY_GRID * 0.22), fill=255))
    round_ = masked(lambda d: d.ellipse([0, 0, n, n], fill=255))
    return square, round_


def write_legacy(layer, name):
    for folder, mult in DENSITIES:
        out = f"{RES}/mipmap-{folder}"
        os.makedirs(out, exist_ok=True)
        layer.resize((LEGACY_GRID * mult, LEGACY_GRID * mult), Image.NEAREST).save(f"{out}/{name}.png", optimize=True)


if __name__ == "__main__":
    bg, fg, mono = pixel_layers()
    square, round_ = legacy(bg, fg)
    write_legacy(square, "ic_launcher")
    write_legacy(round_, "ic_launcher_round")
    write(bg.convert("RGBA"), "ic_launcher_background")
    write(fg, "ic_launcher_foreground")
    write(mono, "ic_launcher_monochrome")

    # A preview of what the launcher will actually put on the home screen.
    comp = Image.alpha_composite(bg.convert("RGBA"), fg)
    m = (GRID - 72) // 2
    vis = comp.crop((m, m, GRID - m, GRID - m))
    sheet = Image.new("RGB", (860, 420), (24, 26, 30))
    sq = P.mask_squircle(vis.resize((360, 360), Image.NEAREST))
    sheet.paste(sq, (20, 30), sq)
    for i, (sz, msk) in enumerate([(200, P.mask_circle), (128, P.mask_squircle), (72, P.mask_circle), (48, P.mask_circle)]):
        im = msk(vis.resize((sz, sz), Image.NEAREST))
        sheet.paste(im, (420 + [0, 220, 220, 310][0] * 0 + [0, 210, 360, 450][i], 30 + (200 - sz) // 2), im)
    sheet.save(f"{P.OUT}/final_preview.png")
    print("wrote layers for", [d for d, _ in DENSITIES], "and final_preview.png")
