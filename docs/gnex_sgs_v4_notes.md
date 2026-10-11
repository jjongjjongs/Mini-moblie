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

Still unknown:

- The opcodes up to 0x7a: about 120 of them, against v1's 59 basic,
  assignment and increment operations.
- Which v4 opcodes the v1 services (0x51 and up) moved to. They are probably
  near +0x40/+0x41: 0x8a-0x90 are frequent and look like v1's 0x4a-0x50
  address operations.
- Bitmap resource format, screen size, and the msg/sys callback contracts.
