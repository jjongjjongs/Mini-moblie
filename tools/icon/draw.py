"""Launcher-icon drafts, drawn straight to pixels (no SVG renderer here).

Laid out on the adaptive icon's 108x108 canvas. A launcher shows only the
middle 72 of that and masks it to its own shape, so the subject is kept
inside a 56-unit box centred on the canvas and nothing that matters goes
near the edge.
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

S = 8
N = 108 * S

WHITE       = (255, 255, 255)
WHITE_LOW   = (223, 229, 232)
WHITE_EDGE  = (198, 206, 211)
TEAL        = (26, 141, 155)
TEAL_DEEP   = (13, 82, 94)
TEAL_LIGHT  = (61, 190, 202)
TEAL_RING   = (31, 160, 174)
KEY         = (150, 219, 226)


def px(u):
    return int(round(u * S))


def rrect(d, box, r, **kw):
    d.rounded_rectangle([px(box[0]), px(box[1]), px(box[2]), px(box[3])], radius=px(r), **kw)


def linear_gradient(size, c0, c1, angle=55):
    w, h = size
    grad = Image.new("RGB", (w, h))
    pix = grad.load()
    a = math.radians(angle)
    dx, dy = math.cos(a), math.sin(a)
    span = abs(dx) * w + abs(dy) * h
    off = w * max(0.0, -dx) + h * max(0.0, -dy)
    for y in range(h):
        base = y * dy + off
        for x in range(w):
            t = min(1.0, max(0.0, (x * dx + base) / span))
            pix[x, y] = (int(c0[0] + (c1[0] - c0[0]) * t),
                         int(c0[1] + (c1[1] - c0[1]) * t),
                         int(c0[2] + (c1[2] - c0[2]) * t))
    return grad


def shape_mask(size, draw_fn):
    m = Image.new("L", size, 0)
    draw_fn(ImageDraw.Draw(m))
    return m


def iridescent(diameter):
    """The cover dial: a pastel wheel, near-white in the middle.

    The photo runs pink across the top, yellow down the left, cyan and blue
    to the right.
    """
    stops = [(0, (246, 158, 190)), (55, (250, 205, 160)), (115, (246, 226, 118)),
             (175, (180, 226, 178)), (235, (150, 224, 231)), (300, (163, 196, 240)),
             (360, (246, 158, 190))]
    img = Image.new("RGBA", (diameter, diameter), (0, 0, 0, 0))
    pix = img.load()
    c = (diameter - 1) / 2
    for y in range(diameter):
        for x in range(diameter):
            dx, dy = x - c, y - c
            r = math.hypot(dx, dy) / c
            if r > 1.0:
                continue
            deg = (math.degrees(math.atan2(dy, dx)) + 90) % 360
            for i in range(len(stops) - 1):
                a0, c0 = stops[i]
                a1, c1 = stops[i + 1]
                if a0 <= deg <= a1:
                    t = (deg - a0) / (a1 - a0)
                    col = [c0[k] + (c1[k] - c0[k]) * t for k in range(3)]
                    break
            wash = max(0.0, 1.0 - r * 1.35) * 0.72
            col = [col[k] + (255 - col[k]) * wash for k in range(3)]
            shade = 1.0 - max(0.0, r - 0.8) * 0.5
            a = 255 if r < 0.97 else int(255 * (1.0 - r) / 0.03)
            pix[x, y] = (int(col[0] * shade), int(col[1] * shade), int(col[2] * shade), a)
    return img


def dial(size_u, dots=True):
    """The cover dial in its teal bezel, `size_u` canvas units across."""
    d = px(size_u)
    img = Image.new("RGBA", (d, d), (0, 0, 0, 0))
    dr = ImageDraw.Draw(img)

    dr.ellipse([0, 0, d - 1, d - 1], fill=TEAL_DEEP)
    ring = max(1, int(d * 0.02))
    dr.ellipse([ring, ring, d - 1 - ring, d - 1 - int(d * 0.06)], fill=TEAL_RING)
    dr.ellipse([ring, ring, d - 1 - ring, d - 1 - int(d * 0.16)], fill=TEAL_LIGHT)

    inset = int(d * 0.135)
    dr.ellipse([inset, inset, d - 1 - inset, d - 1 - inset], fill=TEAL_DEEP)

    fi = int(d * 0.17)
    fd = d - 2 * fi
    face = iridescent(fd)

    if dots:
        # Drawn on a layer of its own and composited, so the dots lighten the
        # face instead of cutting holes in its alpha.
        spots = Image.new("RGBA", (fd, fd), (255, 255, 255, 0))
        f = ImageDraw.Draw(spots)
        c = fd / 2
        step = fd * 0.15
        rad = max(1, int(fd * 0.032))
        for ox, oy in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1),
                       (-1, 1), (1, -1), (0, -2), (0, 2), (-2, 0), (2, 0)]:
            x, y = c + ox * step, c + oy * step
            f.ellipse([x - rad, y - rad, x + rad, y + rad], fill=(255, 255, 255, 225))
        face = Image.alpha_composite(face, spots)

    img.paste(face, (fi, fi), face)
    return img


def lid(w_u, h_u, r_u):
    """The white upper shell: a slab lit from the top left."""
    w, h, r = px(w_u), px(h_u), px(r_u)
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(img).rounded_rectangle([0, 0, w - 1, h - 1], radius=r, fill=WHITE_EDGE)

    e = px(0.7)
    face = linear_gradient((w, h), WHITE, WHITE_LOW, angle=58).convert("RGBA")
    face.putalpha(shape_mask((w, h), lambda d: d.rounded_rectangle(
        [e, e, w - 1 - e, h - 1 - e], radius=r - e, fill=255)))
    img.paste(face, (0, 0), face)
    return img


def closed_phone(w_u, h_u, dial_u):
    """The shut clamshell seen straight on, teal shell under a white lid."""
    lip = 3.0
    w, h = px(w_u), px(h_u + lip)
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    dr = ImageDraw.Draw(img)
    r = px(min(w_u, h_u) * 0.2)

    # The teal lower half, seen as a lip along the bottom and down the sides.
    dr.rounded_rectangle([0, px(1.4), w - 1, h - 1], radius=r, fill=TEAL_DEEP)
    dr.rounded_rectangle([px(0.8), px(1.4), w - 1 - px(0.8), h - 1 - px(0.8)],
                         radius=r, fill=TEAL)

    top = lid(w_u - 2.4, h_u, min(w_u, h_u) * 0.2)
    img.paste(top, (px(1.2), 0), top)

    d = dial(dial_u)
    img.paste(d, ((w - d.width) // 2, px(h_u * 0.27)), d)
    return img


def background(kind):
    if kind == "teal":
        g = linear_gradient((N, N), (25, 104, 119), (8, 41, 55))
    elif kind == "ink":
        g = linear_gradient((N, N), (37, 46, 60), (14, 18, 27))
    elif kind == "slate":
        g = linear_gradient((N, N), (91, 98, 112), (43, 47, 56))
    elif kind == "paper":
        g = linear_gradient((N, N), (248, 250, 251), (206, 219, 224))
    return g.convert("RGBA")


def drop(fg, blur_u=2.0, dy_u=1.4, alpha=115):
    sh = Image.new("RGBA", fg.size, (0, 0, 0, 0))
    sh.paste((0, 0, 0, alpha), (0, 0), fg.split()[3])
    sh = sh.filter(ImageFilter.GaussianBlur(px(blur_u)))
    out = Image.new("RGBA", fg.size, (0, 0, 0, 0))
    out.paste(sh, (0, px(dy_u)), sh)
    out.paste(fg, (0, 0), fg)
    return out


def centred(part, dy_u=0):
    fg = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    fg.paste(part, ((N - part.width) // 2, (N - part.height) // 2 + px(dy_u)), part)
    return fg


# ---- the drafts -------------------------------------------------------------

def draft_a():
    """Tilted shut clamshell - closest to the photo."""
    phone = closed_phone(33, 50, 19)
    phone = phone.rotate(-21, resample=Image.BICUBIC, expand=True)
    return drop(centred(phone)), "teal"


def draft_b():
    """The same clamshell straight on - simplest silhouette."""
    return drop(centred(closed_phone(34, 52, 20))), "ink"


def draft_c():
    """The dial alone - most legible when tiny."""
    return drop(centred(dial(52)), blur_u=2.4, dy_u=1.6, alpha=120), "ink"


def draft_d():
    """Open flip - the shape the icon has today, in the phone's own colours."""
    fg = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    dr = ImageDraw.Draw(fg)

    # Lower half: the teal keypad shell, meeting the lid on one hinge line.
    rrect(dr, (38, 53.5, 70, 80), 4.5, fill=TEAL_DEEP)
    rrect(dr, (38.7, 53.5, 69.3, 79.2), 4.2, fill=TEAL)
    for row in range(3):
        for col in range(3):
            x, y = 41.6 + col * 8.5, 58.5 + row * 6.0
            rrect(dr, (x, y, x + 5.4, y + 3.4), 1.2, fill=KEY)

    # Upper half: the white lid, sharing that line.
    top = lid(32, 28, 4.5)
    fg.paste(top, (px(38), px(26)), top)
    d = dial(17.5)
    fg.paste(d, (px(54) - d.width // 2, px(31)), d)
    return drop(fg), "slate"


DRAFTS = [("A", "기울인 닫힌 폴더", draft_a),
          ("B", "정면 닫힌 폴더", draft_b),
          ("C", "다이얼 단독", draft_c),
          ("D", "열린 폴더 (현행 구도)", draft_d)]


def mask_circle(img):
    out = img.copy()
    out.putalpha(shape_mask(img.size, lambda d: d.ellipse([0, 0, img.size[0] - 1, img.size[1] - 1], fill=255)))
    return out


def mask_squircle(img):
    out = img.copy()
    out.putalpha(shape_mask(img.size, lambda d: d.rounded_rectangle(
        [0, 0, img.size[0] - 1, img.size[1] - 1], radius=int(img.size[0] * 0.235), fill=255)))
    return out


def compose(fn, size=360):
    fg, bg_kind = fn()
    icon = background(bg_kind)
    icon.paste(fg, (0, 0), fg)
    m = (N - px(72)) // 2                      # the middle 72 is all a launcher shows
    return icon.crop((m, m, N - m, N - m)).resize((size, size), Image.LANCZOS)


if __name__ == "__main__":
    big = ImageFont.truetype(FONT_LATIN, 26)
    kr = ImageFont.truetype(FONT_KR, 22)

    cw, ch = 400, 620
    sheet = Image.new("RGB", (cw * 4, ch), (24, 26, 30))
    sd = ImageDraw.Draw(sheet)

    for i, (letter, name, fn) in enumerate(DRAFTS):
        icon = compose(fn)
        x0 = i * cw
        sd.text((x0 + 20, 14), letter, font=big, fill=(240, 240, 240))
        sd.text((x0 + 50, 18), name, font=kr, fill=(168, 174, 184))
        sq = mask_squircle(icon)
        sheet.paste(sq, (x0 + 20, 58), sq)
        for j, (sz, msk, lbl) in enumerate([(150, mask_circle, "원형"), (96, mask_squircle, "스퀘어클"), (48, mask_circle, "48px")]):
            im = msk(icon.resize((sz, sz), Image.LANCZOS))
            sheet.paste(im, (x0 + 20 + [0, 170, 285][j], 430 + (150 - sz) // 2), im)
        sd.text((x0 + 20, 590), "원형 / 스퀘어클 / 48px", font=kr, fill=(120, 126, 136))

    sheet.save(f"{OUT}/sheet.png")
    print("wrote", sheet.size)
