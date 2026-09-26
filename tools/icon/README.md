# Launcher icon

The icon is pixel art of a shut clamshell in the pose of the photograph the
design came from: white lid, teal shell along its lower edge, the iridescent
cover dial, and the strap loop at the top corner. No wordmark from the
original handset is reproduced.

## Regenerating

    python3 tools/icon/export.py

writes `android/app/src/main/res/mipmap-*dpi/` in place. Pillow is the only
dependency.

## How it is built

`pixel.py` lays the phone out flat in device space and carries it into the
picture with one affine - squash for the viewing angle in *screen* space,
then rotate in the plane. Doing the squash first instead foreshortens along
the phone's own long axis and pulls the round dial into an egg.

That is rendered at 16 px per canvas unit, averaged down onto the pixel grid
and snapped to the fixed palette in `PALETTE`. The palette is fixed rather
than chosen per image because an adaptive quantiser spends its colours where
the pixels are - the field and the white lid - and returns the dial, which is
the whole point of the icon, in grey.

## Sizes

The art is authored on a 108-pixel grid, one pixel per dp of the adaptive
icon's 108dp canvas, so every density is an integer nearest-neighbour
multiple: mdpi 1x, xhdpi 2x, xxhdpi 3x, xxxhdpi 4x. hdpi would be 1.5x and is
left out on purpose - Android falls back to xhdpi and scales down, which
beats a layer whose pixels are alternately one and two device pixels wide.

API 24 and 25 have no adaptive icons and take the composited `ic_launcher`
/ `ic_launcher_round` squares instead. Those have their own 48-grid master so
that their sizes (48/96/144/192) stay integer multiples too.

`draw.py` holds the smooth vector-style drafts the design was chosen from and
the shared colour helpers `pixel.py` imports.
