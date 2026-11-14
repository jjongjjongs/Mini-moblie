//! 리얼사커매니저2009 (`00028DE8`): an object given no room for its field.
//!
//! The title is Java compiled to native code, with its own object heap: a
//! pool of headers and a pool of array data, each a free list of blocks whose
//! first word is the block's size - positive free, negative in use - and, in
//! the header pool, a four-byte header before what the block holds. A request
//! is rounded to four bytes and given that header, so `alloc(8)` is a block
//! of twelve: eight for the caller, and the next block's size word right after.
//!
//! Objects of class `0x1334c8` (type `0x10`) are made with `alloc(8)` - the
//! object header alone - at both places that make them (`0xa7d48`, and the
//! make-by-type switch at `0x118a78`). One is made as the title starts, and
//! the code that makes it stores a reference at `+8` (`0xa6c58`): the next
//! block's size word. That block is the rest of the header pool, so its size
//! becomes an address - some gigabyte where 0x6c000 bytes were left - and from
//! then on the pool never runs out. It hands out headers past its own end, over
//! the data pool that follows it, until it hands out one over a live array. The
//! free search with a transfer fee set is what makes enough objects to get
//! that far; the collector's next mark walks the array, reads the header
//! written over it as a reference, and faults (`PC=0x1ff6`, `LR=0x2237`).
//!
//! Both makers are given the twelve bytes the object's own code uses.

use wie_core_arm::ArmCore;
use wie_util::{ByteRead, ByteWrite, Result};

const AID: &str = "00028DE8";

/// The two `movs r0, #8` that size class `0x1334c8`'s objects.
const MAKERS: [u32; 2] = [0xa7d48, 0x118a78];
const ALLOC_8: u16 = 0x2008;
/// `movs r0, #0xc`: the header and the reference at `+8`.
const ALLOC_12: u16 = 0x200c;

/// Sizes the class's objects for their field if this is the title, and the
/// code is the code this was written against.
pub(crate) fn install_object_size_fix(core: &mut ArmCore, aid: &str) -> Result<()> {
    if !aid.eq_ignore_ascii_case(AID) {
        return Ok(());
    }

    for address in MAKERS {
        if halfword_at(core, address)? != ALLOC_8 {
            tracing::warn!("리얼사커매니저2009 object maker at {address:#x}: code differs from the known build; not resized");
            return Ok(());
        }
    }

    for address in MAKERS {
        core.write_bytes(address, &ALLOC_12.to_le_bytes())?;
    }
    tracing::info!("리얼사커매니저2009: class 0x1334c8's objects are given room for their field");

    Ok(())
}

fn halfword_at(core: &ArmCore, address: u32) -> Result<u16> {
    let mut bytes = [0; 2];
    core.read_bytes(address, &mut bytes)?;

    Ok(u16::from_le_bytes(bytes))
}

#[cfg(test)]
mod test {
    use wie_core_arm::ArmCore;
    use wie_util::{ByteRead, ByteWrite, Result};

    use super::*;

    fn title() -> Result<ArmCore> {
        let mut core = ArmCore::new(false, None)?;
        core.load(&[], 0xa0000, 0x80000)?;
        for address in MAKERS {
            core.write_bytes(address, &ALLOC_8.to_le_bytes())?;
        }

        Ok(core)
    }

    #[test]
    fn both_makers_are_given_twelve_bytes() -> Result<()> {
        let mut core = title()?;

        install_object_size_fix(&mut core, AID)?;

        for address in MAKERS {
            assert_eq!(halfword_at(&core, address)?, ALLOC_12, "the maker at {address:#x}");
        }

        Ok(())
    }

    #[test]
    fn another_build_is_left_alone() -> Result<()> {
        let mut core = title()?;
        core.write_bytes(MAKERS[1], &[0x10, 0x20])?;

        install_object_size_fix(&mut core, AID)?;

        // Neither is changed when either differs.
        assert_eq!(halfword_at(&core, MAKERS[0])?, ALLOC_8);
        let mut bytes = [0; 2];
        core.read_bytes(MAKERS[1], &mut bytes)?;
        assert_eq!(bytes, [0x10, 0x20]);

        Ok(())
    }

    #[test]
    fn another_title_is_left_alone() -> Result<()> {
        let mut core = title()?;

        install_object_size_fix(&mut core, "0002C004")?;

        for address in MAKERS {
            assert_eq!(halfword_at(&core, address)?, ALLOC_8);
        }

        Ok(())
    }
}
