"""Pixel-art launcher-icon drafts in the photograph's own pose.

The phone is laid out flat in device space and carried into the picture by
one affine - squash for the viewing angle, then rotate in the plane - so the
lid, its dial and the shell under it all take the same 3/4 pose. That is
rendered large, averaged down onto a pixel grid and snapped to a small
palette, which is what makes it read as pixel art rather than as a photo
someone blurred.

A launcher shows the middle 72 of the 108dp canvas, so the grid is always a
multiple of 3 and the visible art is two thirds of it.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
OUT = os.environ.get("ICON_OUT", os.path.join(HERE, "preview"))
os.makedirs(OUT, exist_ok=True)
sys.path.insert(0, HERE)
FONT_LATIN = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
FONT_KR = os.path.join(REPO, "fonts", "galmuri11.ttf")
import math


from PIL import Image, ImageDraw, ImageFilter, ImageFont
from draw import iridescent, linear_gradient, shape_mask

R = 16                 # working resolution: R pixels per canvas unit
N = 108 * R

WHITE      = (255, 255, 255)
WHITE_LOW  = (214, 222, 227)
WHITE_EDGE = (176, 187, 194)
TEAL       = (28, 146, 160)
TEAL_DEEP  = (12, 78, 92)
TEAL_MID   = (19, 112, 126)
TEAL_LIGHT = (66, 196, 208)
STEEL      = (176, 182, 190)
STEEL_DARK = (118, 125, 134)
SHADOW     = (6, 30, 42)

# Device space: the shut clamshell seen straight down, before the pose.
DW, DH = 116 * R // 4, 250 * R // 4     # lid footprint
DR = 20 * R // 4                        # corner radius
PAD = 120 * R // 4                      # room for the pose to swing into


def device_canvas():
    return Image.new("RGBA", (DW + 2 * PAD, DH + 2 * PAD), (0, 0, 0, 0))


def lid_box():
    return [PAD, PAD, PAD + DW - 1, PAD + DH - 1]


def draw_lid():
    """The white cover, lit from the top left, with its dial and strap loop."""
    img = device_canvas()
    d = ImageDraw.Draw(img)
    x0, y0, x1, y1 = lid_box()

    # The strap loop the photo hangs off the top corner.
    lx, ly = x1 - DW * 0.16, y0 - DR * 0.30
    lr = DR * 0.42
    d.ellipse([lx - lr, ly - lr, lx + lr, ly + lr], fill=STEEL_DARK)
    d.ellipse([lx - lr * 0.72, ly - lr * 0.72, lx + lr * 0.72, ly + lr * 0.72], fill=STEEL)
    d.ellipse([lx - lr * 0.34, ly - lr * 0.34, lx + lr * 0.34, ly + lr * 0.34], fill=(0, 0, 0, 0))

    d.rounded_rectangle([x0, y0, x1, y1], radius=DR, fill=WHITE_EDGE)
    face = linear_gradient((img.width, img.height), WHITE, WHITE_LOW, angle=58).convert("RGBA")
    e = R // 2
    face.putalpha(shape_mask(img.size, lambda g: g.rounded_rectangle(
        [x0 + e, y0 + e, x1 - e, y1 - e], radius=DR - e, fill=255)))
    img.paste(face, (0, 0), face)

    # The cover dial, a touch above centre as the photo has it.
    dd = int(DW * 0.80)
    cx, cy = (x0 + x1) // 2, int(y0 + DH * 0.38)
    bez = ImageDraw.Draw(img)
    bez.ellipse([cx - dd // 2, cy - dd // 2, cx + dd // 2, cy + dd // 2], fill=TEAL_DEEP)
    ib = int(dd * 0.035)
    bez.ellipse([cx - dd // 2 + ib, cy - dd // 2 + ib, cx + dd // 2 - ib, cy + dd // 2 - ib], fill=TEAL_LIGHT)
    ib2 = int(dd * 0.10)
    bez.ellipse([cx - dd // 2 + ib2, cy - dd // 2 + ib2, cx + dd // 2 - ib2, cy + dd // 2 - ib2], fill=TEAL_DEEP)

    fd = int(dd * 0.72)
    face2 = iridescent(fd)
    spots = Image.new("RGBA", (fd, fd), (255, 255, 255, 0))
    sp = ImageDraw.Draw(spots)
    c, step = fd / 2, fd * 0.15
    rad = max(1, int(fd * 0.035))
    for ox, oy in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1),
                   (-1, 1), (1, -1), (0, -2), (0, 2), (-2, 0), (2, 0)]:
        x, y = c + ox * step, c + oy * step
        sp.ellipse([x - rad, y - rad, x + rad, y + rad], fill=(255, 255, 255, 230))
    face2 = Image.alpha_composite(face2, spots)
    img.paste(face2, (cx - fd // 2, cy - fd // 2), face2)
    return img


def draw_dial_face(colour):
    """Just the dial's face, in device space - what a themed icon punches out."""
    img = device_canvas()
    x0, y0, x1, y1 = lid_box()
    dd = int(DW * 0.80 * 0.80)
    cx, cy = (x0 + x1) // 2, int(y0 + DH * 0.38)
    ImageDraw.Draw(img).ellipse([cx - dd // 2, cy - dd // 2, cx + dd // 2, cy + dd // 2], fill=colour)
    return img


def render_dial_mask(theta=52, squash=0.70, scale=1.00, **_):
    """The dial's face in the picture's pose, as a mask."""
    return pose(draw_dial_face((255, 255, 255, 255)), theta, squash, (N, N),
                (N / 2, N / 2 - 3 * R), scale).split()[3]


def draw_silhouette(colour):
    img = device_canvas()
    ImageDraw.Draw(img).rounded_rectangle(lid_box(), radius=DR, fill=colour)
    return img


def pose(img, theta_deg, squash, out_size, centre, scale=1.0):
    """Carry a device-space layer into the picture.

    Screen = diag(1, squash) . R(theta) . scale . (device - device centre) + centre.
    The squash comes last, in screen space, because that is where a camera
    foreshortens: applied before the rotation it runs along the phone's own
    long axis instead and pulls the round dial into an egg.

    PIL wants that mapping inverted.
    """
    th = math.radians(theta_deg)
    cos, sin = math.cos(th), math.sin(th)
    # Inverse of diag(1,squash).R(theta).scale
    a, b = cos / scale, sin / (squash * scale)
    dd, e = -sin / scale, cos / (squash * scale)
    sx, sy = centre
    dcx, dcy = PAD + DW / 2, PAD + DH / 2
    c = -a * sx - b * sy + dcx
    f = -dd * sx - e * sy + dcy
    return img.transform(out_size, Image.AFFINE, (a, b, c, dd, e, f), resample=Image.BICUBIC)


def render_pose(theta=52, squash=0.70, scale=1.00, wall=0.30, soft_shadow=True):
    """The photograph's pose: lid tilted, shell showing along the low edge."""
    centre = (N / 2, N / 2 - 3 * R)
    size = (N, N)

    art = Image.new("RGBA", size, (0, 0, 0, 0))

    # The shell under the lid, smeared down the screen to make its side wall.
    th = math.radians(theta)
    # Square to the long axis, towards the viewer - the edge the photo shows.
    step_x = math.cos(th) * 0.95 * R * wall
    step_y = math.sin(th) * 0.95 * R * wall * squash + 0.85 * R * wall
    for i in range(12, 0, -1):
        t = i / 12
        col = tuple(int(TEAL_DEEP[k] + (TEAL[k] - TEAL_DEEP[k]) * (1 - t) * 0.8) for k in range(3))
        layer = pose(draw_silhouette(col), theta, squash, size,
                     (centre[0] + step_x * i, centre[1] + step_y * i), scale)
        art = Image.alpha_composite(art, layer)

    seam = pose(draw_silhouette(TEAL_DEEP), theta, squash, size,
                (centre[0] + step_x * 0.55, centre[1] + step_y * 0.55), scale)
    art = Image.alpha_composite(art, seam)

    lid = pose(draw_lid(), theta, squash, size, centre, scale)
    art = Image.alpha_composite(art, lid)

    sh = Image.new("RGBA", size, (0, 0, 0, 0))
    if soft_shadow:
        sh.paste((0, 0, 0, 150), (0, 0), art.split()[3])
        sh = sh.filter(ImageFilter.GaussianBlur(1.1 * R))
        off = (int(0.5 * R), int(1.6 * R))
    else:
        sh.paste(SHADOW + (255,), (0, 0), art.split()[3])
        off = (int(1.5 * R), int(2.5 * R))
    out = Image.new("RGBA", size, (0, 0, 0, 0))
    out.paste(sh, off, sh)
    return Image.alpha_composite(out, art)


def background(c0, c1):
    return linear_gradient((N, N), c0, c1, angle=55).convert("RGBA")


PALETTE = [
    # background field
    (11, 15, 24), (18, 24, 36), (27, 35, 50), (38, 48, 66),
    # shell teal
    (12, 78, 92), (19, 112, 126), (28, 146, 160), (66, 196, 208),
    # white lid
    (255, 255, 255), (237, 242, 244), (214, 222, 227), (176, 187, 194), (138, 152, 162),
    # the dial, which has to keep its own colours whatever else goes
    (246, 158, 190), (250, 205, 160), (246, 226, 118), (180, 226, 178),
    (150, 224, 231), (163, 196, 240), (252, 240, 246),
    # strap loop and contact shadow
    (176, 182, 190), (118, 125, 134), (6, 30, 42), (3, 18, 26),
]


def palette_image(n):
    """A PIL palette image holding the first `n` entries, padded out."""
    entries = PALETTE[:n]
    flat = [c for rgb in entries for c in rgb]
    flat += [0, 0, 0] * (256 - len(entries))
    pal = Image.new("P", (1, 1))
    pal.putpalette(flat)
    return pal


def pixelate(icon, grid, colours):
    """Average onto a `grid`x`grid` canvas, then snap to the fixed palette."""
    small = icon.resize((grid, grid), Image.BOX).convert("RGB")
    if colours:
        small = small.quantize(palette=palette_image(colours), dither=Image.NONE).convert("RGB")
    return small


def visible(small):
    """The middle two thirds - all a launcher ever shows."""
    g = small.size[0]
    m = g // 6
    return small.crop((m, m, g - m, g - m))


def mask_squircle(img, radius=0.235):
    out = img.convert("RGBA")
    out.putalpha(shape_mask(img.size, lambda d: d.rounded_rectangle(
        [0, 0, img.size[0] - 1, img.size[1] - 1], radius=int(img.size[0] * radius), fill=255)))
    return out


def mask_circle(img):
    out = img.convert("RGBA")
    out.putalpha(shape_mask(img.size, lambda d: d.ellipse(
        [0, 0, img.size[0] - 1, img.size[1] - 1], fill=255)))
    return out


VARIANTS = [
    ("P1", "32px \uadf8\ub9ac\ub4dc", 48, 24),
    ("P2", "48px \uadf8\ub9ac\ub4dc", 72, 24),
    ("P3", "64px \uadf8\ub9ac\ub4dc", 96, 24),
    ("P4", "96px \uadf8\ub9ac\ub4dc", 144, 24),
]


if __name__ == "__main__":
    phone = render_pose()
    bg = background((38, 48, 66), (11, 15, 24))
    icon = Image.alpha_composite(bg, phone)
    icon.resize((432, 432), Image.LANCZOS).save(f"{OUT}/pose_smooth.png")

    big = ImageFont.truetype(FONT_LATIN, 26)
    kr = ImageFont.truetype(FONT_KR, 22)

    cw, ch = 400, 620
    sheet = Image.new("RGB", (cw * len(VARIANTS), ch), (24, 26, 30))
    sd = ImageDraw.Draw(sheet)

    for i, (tag, name, grid, colours) in enumerate(VARIANTS):
        vis = visible(pixelate(icon, grid, colours))
        shown = vis.resize((360, 360), Image.NEAREST)
        x0 = i * cw
        sd.text((x0 + 20, 14), tag, font=big, fill=(240, 240, 240))
        sd.text((x0 + 62, 18), name, font=kr, fill=(168, 174, 184))
        sq = mask_squircle(shown)
        sheet.paste(sq, (x0 + 20, 58), sq)
        for j, (sz, msk) in enumerate([(150, mask_circle), (96, mask_squircle), (48, mask_circle)]):
            im = msk(vis.resize((sz, sz), Image.NEAREST))
            sheet.paste(im, (x0 + 20 + [0, 170, 285][j], 430 + (150 - sz) // 2), im)
        sd.text((x0 + 20, 590), f"{vis.size[0]}px 원본 · 원형/스퀸어클/48px",
                font=kr, fill=(120, 126, 136))

    sheet.save(f"{OUT}/pixel_sheet.png")
    print("wrote", sheet.size)
