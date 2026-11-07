# GNEX SGS version 4 - format notes

Findings from five SKT GNEX titles (절묘한타이밍, 물가에돌튕기기2, 2006프로야구,
마구마구, 놈투(GS)), checked byte for byte against all five. The reference
emulator wfeature 0.5.2 (MIT, `internal/sgsvm`) runs only header versions 1
and 2 (at most 128 KiB). Every one of these five is version 4, at 300-840 KiB,
and the reference rejects them while loading.

## Package

Each title is a `.mod` descriptor (`application/x-gnex-sgs`, `sgs`, a
version such as `36.9.0`), the `.sgs` body, and a `.wmr` holding the menu
icons as BMPs. Some packages carry save files beside these.

## Wrapper

The file starts with a 32-byte wrapper. Its first u32 is 32, and the 16 bytes
at offset 8 are either zero or a value per title (perhaps a key or digest; it
is not needed to read the body). The body is not encrypted. Every offset below
is relative to the end of the wrapper.

## Header (128 bytes; code starts at 0x80)

| Offset | Field |
|---|---|
| 0x00 | version, `4` |
| 0x01 | LCD class mask (`0x08` or `0xff`) |
| 0x08 | u16, purpose unknown |
| 0x0a | title, EUC-KR, NUL-padded |
| 0x1c | eight callbacks, u32 each: init, term, timer, key, msg1, msg2, sys1, sys2 (v1 had u16) |
| 0x48 | four u32 region offsets, in order: variable descriptors, variable initializers, resource descriptors, resource data (v1: u16 at 44) |
| 0x5c | `0xffff` |
| 0x60 | `0x10000` |
| 0x78 | u32, the body's total length |

## Variables

- Each descriptor is 8 bytes: a u16 of flags (low byte = mutable; high byte
  is 1 or 0, meaning unknown), a u16 element count, and a u32 that is 0.
- Every variable, mutable or not, has an initializer.
- Each element is 32 bits (v1: 16 bits, and only some variables had one).
- Check: the element count × 4 equals the initializer region exactly for
  all five titles.

## Resources

- Each descriptor is 12 bytes: a u32 of flags (byte 0 = mutable, byte 1 =
  kind), a u32 size, and a u32 that is 0.
- The data follows in order, and the sizes add up exactly to the end of the
  file for all five titles.
- Kind 2 is SMAF audio: the kind-2 count matches the number of `MMMD`
  headers in every title.
- Kind 1 is probably bitmaps (not yet decoded). Kind 0 is strings and data.

## Code

The instruction stream is v1's, with each byte widened to a little-endian u16
slot. A v1 word operand becomes two slots, high then low, so a branch target
is `(hi << 16) | lo` and can pass 64 KiB.

The control opcodes are v1's moved up by 0x40, with one new opcode before
call:

| v4 | v1 | Operation | Evidence |
|---|---|---|---|
| 0x7b-0x80 | 0x3b-0x40 | compare with an immediate, then branch: op, imm, hi, lo | 0x7d-0x80 seen |
| 0x81 | 0x41 | jump | 1225 uses |
| 0x82 | 0x42 | jump if true | |
| 0x83 | 0x43 | jump if false | |
| 0x84 | - | new, unused in 놈투 | |
| 0x85 | 0x44 | call | 795 of 826 targets follow a 0x86 |
| 0x86 | 0x45 | return | |
| 0xff | - | end of a callback | |

Basic operations 0x01-0x0a look unchanged in shape: for example, `04 vv`
pushes a variable.

Still unknown when this was first written (most of it is answered in the next section):

- The opcodes up to 0x7a: about 120 of them, against v1's 59 basic,
  assignment and increment operations.
- Which v4 opcodes the v1 services (0x51 and up) moved to. They are probably
  near +0x40/+0x41: 0x8a-0x90 are frequent and look like v1's 0x4a-0x50
  address operations.
- Bitmap resource format, screen size, and the msg/sys callback contracts.

## Running it: `wie_gnex`

`wie_gnex` runs these titles. The emulator picks it for an archive that holds a
`.mod` naming `application/x-gnex-sgs` beside a version-4 `.sgs`.

**Panel**: three of the five were built for 176x220 handsets. On a larger panel
they put their fixed-size pictures at its centre or edges and leave the rest
bare; on 176x220 each fills the screen as drawn. 놈투 sizes itself to any
panel and runs on 240x320. 마구마구 carries only its small-handset pictures
(a 130x150 stadium, a 130-wide batting backdrop) and picks the layout they
belong to only when the panel is no larger than 130x150 (`var94`, set at
0x21e from the screen's width and height); on anything bigger it places them
in a 180x220 layout with gaps the previous screen shows through. It runs on
128x150. Nothing in the package says which a title is, so 176x220 is the
default and the others are listed by name.

All five
reach play: 놈투(GS) through its story to Stage 1, 절묘한타이밍 to its gear
game, 물가에돌튕기기2 to its beach, 2006프로야구 to its team selection, and
마구마구 to an inning.

What was worked out beyond the tables above:

- **Opcodes**:
  - 0x05/0x06/0x07 push a signed 16-bit, an unsigned 16-bit and a 32-bit
    immediate.
  - 0x08-0x0b store; 0x0c/0x0d mark and cut the stack.
  - 0x0e inc, 0x0f dec, 0x10 dup, 0x11 negate, 0x12 swap, 0x13 add, 0x14 sub.
  - 0x50-0x5e: mul, div, mod, and, or, not, xor, shr, shl, >, <, >=, <=, ==, !=.
  - 0x5f-0x76 assign: four destinations × the sources `var[var2[i]]`,
    `var[var2]`, `var[i]`, `var`, signed imm16, unsigned imm16 (절묘한타이밍's
    `0x76` settles the last: one slot, not two).
  - 0x77-0x7a add an immediate.
  - 0x89-0x8c store and keep; 0x8d/0x8e push an address; 0x8f read it;
    0x90/0x91 write it.
  - 0x92 n calls service n.
  - 0x94-0x97 draw a bitmap: (x, y, res), +mirror, +palette, +both. 0x98 and
    0x99 (마구마구) take seven and eight values: x, y, res, [palette], two
    unknown, the mirror and one more.
  - Sums: 0xe8 pushes `var a + var b + imm`, 0xeb `var a + var b`, 0xee adds
    `var a` to the top of the stack; 0xf1 pushes the *address* of `var a`
    plus `imm`.
  - The same operations on flat addresses (a word's position among all the
    variables' elements laid end to end), which 2006프로야구 is compiled to
    throughout: 0xd9 read `[a + [b]]`, 0xda read `[a]`, 0xdb/0xdc write
    them; 0xde `[a + [b]] = [c]`, 0xdf `[a + [b]] = imm`, 0xe0 `[a] = [b +
    [c]]`, 0xe1 `[a] = [b]`, 0xe2 `[a] = imm`, 0xe4 `[a] += imm`, 0xe6 store
    and keep, 0xe7 push `a`; and the sums 0xea, 0xed, 0xf0 (values) and 0xf3
    (`a + imm`, an address).
  - An index is not held to its variable's length: 절묘한타이밍 reads past the
    end of one into the words after it.
- **System variables**: 0 is the event parameter, 4/5 are the screen's width
  and height, and 10 is the runtime version (1.1).
- **Bitmaps**: types 2-8 as v1. Types 9-12 carry their own RGB palette: 1, 2,
  4 or 8 bits a pixel; after the type, size and hot spot come a flag that
  makes the first colour transparent, the colour count (0 for 256) and the
  triples, then the pixels packed row after row.
- **Services**: see `wie_gnex/src/services.rs`. They include:
  - text: 0x28 attributes, 0x55 draw (0x57/0x59 with an effect), 0x1e/0x1f
    font width and height, 0x20 a string's width;
  - drawing:
    - 0x22 clip to two corners, both inside; 0x23 unclip;
    - colour: 0x26 a script colour, 0x27 RGB;
    - 0x3c point; lines: 0x42 line, 0x43 horizontal, 0x44 vertical;
    - boxes: 0x45 outline, 0x46 fill, 0x47 rounded outline, 0x49 rounded
      fill (2006프로야구's dialogs and 놈투's selection bar are nested
      0x49 boxes with text written on them; 0x48 is taken as a fill too,
      unseen); 0x4b/0x4c ellipse outline and fill;
    - 0x3a/0x3b clear white/black, 0x36 palette bank;
    - 0x54 scroll a whole buffer;
  - frames: 0x51/0x52 save/restore the backdrop, 0x5f present (0x60 for a
    region);
  - 0x2d pack a palette;
  - timers 0x95-0x9a, save words 0x93/0x94;
  - files, named by a string resource and kept in the handset's storage:
    0x6b exists, 0x63 open (1 existing, 2 new, 3 read), 0x64 close, 0x65
    delete, 0x66 seek, 0x67/0x69 write/read words, 0x68/0x6a write/read
    bytes, 0x62 size, 0x6c-0x6e directories. A package's own data files
    (마구마구's `bb/`, 절묘한타이밍's `f1`/`f2`) are there to read;
  - 0xc1 reads a signed little-endian number of 1-4 bytes out of a resource
    at an offset into a word: 절묘한타이밍 walks its title and game
    animation data with it;
  - 0xb4 the handset's text entry: (prompt, destination string, most
    bytes, 1 for digits only). The script is held until the player is done;
    2006프로야구 names a player through it, 절묘한타이밍 takes a phone
    number;
  - strings 0x9d-0xb2; numbers 0xc8-0xd9 (0xcd/0xce sine and cosine
    ×100, 0xd3 square root), vectors 0xdd-0xdf, 0xe0 range test;
  - the network 0x101-0x105, which never connects;
  - 0x1d8 system property.
- **Licence check**: 놈투 checks a licence unless the platform property
  `PLATFORM` is `WIPI`.

Still to do:

- Background music plays once. No service asks for a repeat, and the
  reference player plays these once too.

## Versions 1 and 2: `wie_gnex::classic`

The earlier layouts run in a separate interpreter, `wie_gnex/src/classic`,
ported from wfeature's `internal/sgsvm` and the services in
`internal/platform/skt/script*.go` (MIT). They keep v1's one-byte slots and
16-bit values, address variables through a mutable and a constant bank
(`0x4000` marks the constant one), and call the platform by opcode (0x51 and
up) rather than through `0x92 n`. The drawing buffer, palette, bitmaps,
timers, sound and files are this crate's, shared with version 4. The panel
follows the header's LCD class mask: 128x160, or 176x176 for a script that
leaves out class 4 but takes class 8. 샤이닝스톤 (`0014478632`, version 2)
runs from its logo through the story into Act 1's fights.
