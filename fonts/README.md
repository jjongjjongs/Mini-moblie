# Fonts

The system font a title draws its text with. Every face here is a pixel font:
its glyphs are drawn on a whole-pixel grid, so it is only itself at whole
multiples of its own design height and turns to grey mush at anything else.
`wie_backend::canvas` picks the face whose design height divides the height a
title asked for.

| File | Design height | Crisp at | Licence |
| --- | --- | --- | --- |
| `neodgm.ttf` | 16px | 16, 32, … | SIL OFL 1.1 |
| `galmuri9.ttf` | 11px | 11, 22, … | SIL OFL 1.1 (`galmuri-OFL.md`) |
| `galmuri11.ttf` | 14px | 14, 28, … | SIL OFL 1.1 (`galmuri-OFL.md`) |

## How the Galmuri faces were prepared

Upstream is <https://github.com/quiple/galmuri>, whose `dist/Galmuri9.ttf` and
`dist/Galmuri11.ttf` are 4.6MB and 5.4MB and carry kana and CJK ideographs a
Korean handset title has no use for. Each was subset to what a title draws and
then rescaled so that one design pixel is a whole number of font units, which
is what makes the rasteriser land on the pixel grid:

```sh
pyftsubset Galmuri9.ttf --output-file=galmuri9.ttf \
  --unicodes="U+0020-007E,U+00A0-00FF,U+3130-318F,U+AC00-D7A3,U+2000-206F,U+20A9,U+00B7,U+2022,U+2026,U+2190-2193,U+25A0-25FF,U+2600-26FF,U+FF01-FF60" \
  --layout-features='' --no-hinting --desubroutinize

python3 -c "
from fontTools.ttLib import TTFont
from fontTools.ttLib.scaleUpem import scale_upem
f = TTFont('galmuri9.ttf'); scale_upem(f, 20); f.save('galmuri9.ttf')"
```

Galmuri11 is the same with `scale_upem(f, 24)`. The units-per-em cannot go
below 16 - the smallest OpenType allows - so the grid is two font units per
design pixel rather than one.
