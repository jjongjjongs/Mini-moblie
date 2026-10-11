# App icon

A rounded white `m` with a cyan status light, on a deep teal tile. The same
picture is the icon of every build.

## Regenerating

    python3 tools/icon/monogram.py

writes every file below in place. Pillow is the only dependency.

| Build | File | Notes |
|---|---|---|
| Android | `android/app/src/main/res/mipmap-*dpi/` | adaptive background, foreground and monochrome layers on the 108dp canvas, plus the composited square and round icons for API 24-25 |
| iOS | `ios/MiniMobile/Assets.xcassets/AppIcon.appiconset/` | one opaque 1024px square; iOS rounds it. `ios/project.yml` names the set |
| Windows | `wie_portmaster/assets/minimobile.ico` | 16-256px, embedded in the exe by `wie_portmaster/build.rs` |
| Windows, Linux | `wie_portmaster/assets/window_icon.rgba` | 64x64 raw RGBA the program gives SDL for its window |
| Linux | `linux/MiniMobile.png` | 256px, shipped in the tarball |
| PortMaster | `portmaster/minimobile/MiniMobile.png` | 256px, shipped in the port folder |

## How it is drawn

The design lives on a 108-unit tile. The `m` is a centre line - two arches
over three stems - stamped with round dots of the stroke width, so its ends
and joins come out round without a vector renderer. Everything is drawn at
16x and box-filtered down, so edges stay smooth at 16px and at 1024px alike.

On Android the launcher shows only the middle 72dp of the adaptive canvas and
masks it to its own shape, so the tile's 108 units are fitted to those 72dp:
masked, the icon looks like the tile. The background's gradient runs past
them at its end colours, so a launcher that shows more of the canvas while
animating finds no seam.
