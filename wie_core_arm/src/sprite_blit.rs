//! Gamevil's run-length sprite blitter, answered natively.
//!
//! The Gamevil engine behind 제노니아 1·2, 슈퍼사커 and others draws nearly every
//! sprite through one small Thumb routine: a stream of 16-bit codes - skip so
//! many transparent pixels, draw a run of palette indices, move to the next
//! row, stop - turned into RGB565 pixels on the screen. It is the whole of the
//! title's drawing, so it is the whole of its cost: in a 제노니아2 field it was
//! 58% of every instruction the title ran. A second copy of it clips the
//! sprite to a rectangle, for one that runs off the screen's edge.
//!
//! Both are found by their exact bytes, so a title that carries them gets them
//! answered and one that does not is untouched. Each one's first instruction
//! becomes an `svc` the engine answers itself (a [`SvcIntrinsic`]) followed by
//! a `bx lr`, so a call costs one native blit instead of one guest
//! instruction per pixel and a dozen per run. An engine that cannot answer it
//! hands it to the handler here, which draws the same pixels the slow way.

use alloc::{collections::BTreeMap, format, sync::Arc, vec};

use wie_util::{ByteRead, ByteWrite, Result, WieError};

use crate::{ArmCore, engine::ArmRegister, engine::SvcIntrinsic, function::JumpTo};

/// The `svc` category the patched entries carry.
pub(crate) const SPRITE_BLIT_SVC: u32 = 0x81;

/// `blit(dst, codes, palette, stride)`: the routine as the titles carry it,
/// literal pool and all.
pub(crate) const PLAIN: &[u8] = &[
    0x70, 0xb5, 0x16, 0x1c, 0x1d, 0x1c, 0x4b, 0x78, 0x0a, 0x78, 0x1b, 0x02, 0x13, 0x4c, 0x1a, 0x43, 0xa2, 0x42, 0x1f, 0xd0, 0x12, 0x4b, 0x02, 0x31,
    0x9a, 0x42, 0x01, 0xd1, 0x6b, 0x00, 0x03, 0xe0, 0x13, 0x04, 0x00, 0x2b, 0x02, 0xdb, 0x53, 0x00, 0xc0, 0x18, 0xec, 0xe7, 0x0d, 0x4b, 0x1a, 0x40,
    0x53, 0x1e, 0x1b, 0x04, 0x1a, 0x0c, 0xa2, 0x42, 0xe5, 0xd0, 0x0b, 0x78, 0x01, 0x31, 0x5b, 0x00, 0x9b, 0x5b, 0x03, 0x80, 0x53, 0x1e, 0x1b, 0x04,
    0x1a, 0x0c, 0x04, 0x4b, 0x02, 0x30, 0x9a, 0x42, 0xf3, 0xd1, 0xd8, 0xe7, 0x70, 0xbc, 0x01, 0xbc, 0x00, 0x47, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00,
    0xfe, 0xff, 0x00, 0x00, 0xff, 0x7f, 0x00, 0x00,
];

/// `blit_clipped(dst, codes, palette, stride, x, width, y, height)`: the same,
/// drawing only the sprite's columns `x..x + width` and rows `y..y + height`.
pub(crate) const CLIPPED: &[u8] = &[
    0xf0, 0xb5, 0x82, 0xb0, 0x01, 0x92, 0x00, 0x93, 0x08, 0x9d, 0x0a, 0x9c, 0x07, 0x9a, 0x09, 0x9b, 0x00, 0x26, 0xb4, 0x46, 0xad, 0x18, 0xe4, 0x18,
    0x4b, 0x78, 0x0a, 0x78, 0x1b, 0x02, 0x1f, 0x4f, 0x1a, 0x43, 0xba, 0x42, 0x35, 0xd0, 0x1e, 0x4b, 0x02, 0x31, 0x9a, 0x42, 0x08, 0xd1, 0x01, 0x27,
    0xbc, 0x44, 0xa4, 0x45, 0x2d, 0xda, 0x00, 0x9a, 0x00, 0x26, 0x53, 0x00, 0xc0, 0x18, 0xeb, 0xe7, 0x13, 0x04, 0x00, 0x2b, 0x03, 0xdb, 0x53, 0x00,
    0xc0, 0x18, 0xb6, 0x18, 0xe4, 0xe7, 0x15, 0x4b, 0x1a, 0x40, 0x09, 0x9b, 0x9c, 0x45, 0x03, 0xda, 0x53, 0x00, 0xc0, 0x18, 0x89, 0x18, 0xdb, 0xe7,
    0x53, 0x1e, 0x1b, 0x04, 0x1a, 0x0c, 0xba, 0x42, 0xd6, 0xd0, 0x07, 0x9f, 0xbe, 0x42, 0x06, 0xdb, 0xae, 0x42, 0x04, 0xda, 0x0b, 0x78, 0x01, 0x9f,
    0x5b, 0x00, 0xdb, 0x5b, 0x03, 0x80, 0x53, 0x1e, 0x1b, 0x04, 0x1a, 0x0c, 0x05, 0x4b, 0x02, 0x30, 0x01, 0x31, 0x01, 0x36, 0x9a, 0x42, 0xec, 0xd1,
    0xc2, 0xe7, 0x02, 0xb0, 0xf0, 0xbc, 0x01, 0xbc, 0x00, 0x47, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0xfe, 0xff, 0x00, 0x00, 0xff, 0x7f, 0x00, 0x00,
];

/// `svc #0x81; bx lr`, over each routine's first two instructions.
pub(crate) const ENTRY: [u8; 4] = [SPRITE_BLIT_SVC as u8, 0xdf, 0x70, 0x47];

const END: u32 = 0xffff;
const NEXT_ROW: u32 = 0xfffe;
const RUN: u32 = 0x8000;

/// Where a blit reads and writes. `None` is an address nothing is mapped at,
/// which the routine would have faulted on.
pub(crate) trait BlitMemory {
    fn byte(&self, address: u32) -> Option<u8>;
    fn half(&self, address: u32) -> Option<u16>;
    fn set_half(&mut self, address: u32, value: u16) -> Option<()>;
}

/// The routine's arguments: `r0`..`r3`, and for the clipped one the four
/// words its caller left on the stack.
#[derive(Clone, Copy)]
pub(crate) struct Blit {
    pub dst: u32,
    pub codes: u32,
    pub palette: u32,
    pub stride: u32,
    pub clip: Option<Clip>,
}

#[derive(Clone, Copy)]
pub(crate) struct Clip {
    pub x: i32,
    pub width: i32,
    pub y: i32,
    pub height: i32,
}

impl Blit {
    /// The blit for `kind`, from the registers and stack it was entered with.
    pub(crate) fn from_entry(kind: SvcIntrinsic, regs: [u32; 4], stack: impl Fn(u32) -> Option<u32>) -> Option<Self> {
        let clip = match kind {
            SvcIntrinsic::SpriteRle => None,
            SvcIntrinsic::SpriteRleClipped => Some(Clip {
                x: stack(0)? as i32,
                width: stack(4)? as i32,
                y: stack(8)? as i32,
                height: stack(12)? as i32,
            }),
            _ => return None,
        };

        Some(Self {
            dst: regs[0],
            codes: regs[1],
            palette: regs[2],
            stride: regs[3],
            clip,
        })
    }

    /// Draws the sprite exactly as the routine does, and returns the span of
    /// bytes it wrote (empty when it wrote none) - or the address it would
    /// have faulted at. A fault part-way leaves the pixels before it drawn, as
    /// the routine would have; drawing the same sprite again from the start
    /// writes the same pixels, so a caller can hand a failed blit to a slower
    /// path that reports the fault.
    pub(crate) fn run<M: BlitMemory>(&self, memory: &mut M) -> core::result::Result<(u32, u32), u32> {
        let mut dst = self.dst;
        let mut codes = self.codes;
        let mut low = u32::MAX;
        let mut high = 0;

        // The clipped routine's column within the row and row within the
        // sprite, compared signed as it compares them.
        let mut column: i32 = 0;
        let mut row: i32 = 0;

        loop {
            let code = memory.byte(codes).ok_or(codes)? as u32 | (memory.byte(codes.wrapping_add(1)).ok_or(codes.wrapping_add(1))? as u32) << 8;
            if code == END {
                break;
            }
            codes = codes.wrapping_add(2);

            if code == NEXT_ROW {
                if let Some(clip) = self.clip {
                    row = row.wrapping_add(1);
                    if row >= clip.y.wrapping_add(clip.height) {
                        break;
                    }
                    column = 0;
                }
                dst = dst.wrapping_add(self.stride << 1);
                continue;
            }

            if code & RUN == 0 {
                // Transparent: step over them.
                dst = dst.wrapping_add(code << 1);
                column = column.wrapping_add(code as i32);
                continue;
            }

            let count = code & !RUN;
            let shown = match self.clip {
                // A row above the clip is passed over whole.
                Some(clip) if row < clip.y => {
                    dst = dst.wrapping_add(count << 1);
                    codes = codes.wrapping_add(count);
                    continue;
                }
                Some(clip) => Some((clip.x, clip.x.wrapping_add(clip.width))),
                None => None,
            };

            for _ in 0..count {
                if shown.is_none_or(|(from, to)| column >= from && column < to) {
                    let index = memory.byte(codes).ok_or(codes)? as u32;
                    let entry = self.palette.wrapping_add(index << 1);
                    let pixel = memory.half(entry).ok_or(entry)?;
                    memory.set_half(dst, pixel).ok_or(dst)?;
                    low = low.min(dst);
                    high = high.max(dst.wrapping_add(2));
                }
                dst = dst.wrapping_add(2);
                codes = codes.wrapping_add(1);
                column = column.wrapping_add(1);
            }
        }

        Ok(if low < high { (low, high) } else { (0, 0) })
    }
}

/// Finds the blitters in `ranges` and has them answered natively. Returns how
/// many it found.
pub fn install_sprite_blits(core: &mut ArmCore, ranges: &[(u32, u32)]) -> Result<usize> {
    let mut found = BTreeMap::new();
    for &(base, size) in ranges {
        let mut image = vec![0u8; size as usize];
        if core.read_bytes(base, &mut image).is_err() {
            continue;
        }
        for (pattern, kind) in [(PLAIN, SvcIntrinsic::SpriteRle), (CLIPPED, SvcIntrinsic::SpriteRleClipped)] {
            for (offset, _) in image.windows(pattern.len()).enumerate().filter(|(_, window)| *window == pattern) {
                // Halfword-aligned, as Thumb code is.
                if offset % 2 == 0 {
                    found.insert(base + offset as u32, kind);
                }
            }
        }
    }
    if found.is_empty() {
        return Ok(0);
    }

    for (&address, &kind) in &found {
        core.write_bytes(address, &ENTRY)?;
        core.inner.lock().engine.set_svc_intrinsic(address, kind);
        tracing::info!("Sprite blitter at {address:#x} answered natively ({kind:?})");
    }
    core.register_svc_handler(SPRITE_BLIT_SVC, handle_sprite_blit_svc, &Arc::new(found.clone()))?;

    Ok(found.len())
}

/// The guest's memory through the core, one access at a time - the slow way,
/// for an engine that does not answer the blit itself.
struct CoreMemory<'a>(&'a mut ArmCore);

impl BlitMemory for CoreMemory<'_> {
    fn byte(&self, address: u32) -> Option<u8> {
        let mut value = [0u8; 1];
        self.0.read_bytes(address, &mut value).ok().map(|_| value[0])
    }

    fn half(&self, address: u32) -> Option<u16> {
        let mut value = [0u8; 2];
        self.0.read_bytes(address, &mut value).ok().map(|_| u16::from_le_bytes(value))
    }

    fn set_half(&mut self, address: u32, value: u16) -> Option<()> {
        self.0.write_bytes(address, &value.to_le_bytes()).ok()
    }
}

async fn handle_sprite_blit_svc(core: &mut ArmCore, blitters: &mut Arc<BTreeMap<u32, SvcIntrinsic>>) -> Result<JumpTo> {
    let (pc, lr) = core.read_pc_lr()?;
    let address = pc.wrapping_sub(2) & !1;
    let kind = *blitters
        .get(&address)
        .ok_or_else(|| WieError::FatalError(format!("sprite blit svc at unregistered {address:#x}")))?;

    let (regs, sp) = {
        let inner = core.inner.lock();
        (
            [
                inner.engine.reg_read(ArmRegister::R0),
                inner.engine.reg_read(ArmRegister::R1),
                inner.engine.reg_read(ArmRegister::R2),
                inner.engine.reg_read(ArmRegister::R3),
            ],
            inner.engine.reg_read(ArmRegister::SP),
        )
    };
    let stack = |offset: u32| {
        let mut word = [0u8; 4];
        core.read_bytes(sp.wrapping_add(offset), &mut word).ok().map(|_| u32::from_le_bytes(word))
    };
    let blit = Blit::from_entry(kind, regs, stack).ok_or(WieError::InvalidMemoryAccess(sp))?;

    blit.run(&mut CoreMemory(core)).map_err(WieError::InvalidMemoryAccess)?;

    Ok(JumpTo(lr))
}
