//! `MXUserMemInterf` — the KTF extension library a title reaches through
//! [`MC_knlGetDLLInterface`](super::kernel::get_dll_interface).
//!
//! It is an allocator over memory the *title* owns. 마스터오브소드4 hands it a
//! 200KB static buffer of its own the moment it starts
//! (`table[0](base, 0xc8 << 10)` at `0x1010cc` in its image) and from then on
//! takes every allocation out of that region rather than out of the platform
//! heap. The interface is four function pointers, in the order the reference
//! writes them and the order that title indexes them:
//!
//! | slot | call | arguments |
//! |------|------|-----------|
//! | 0 | `add` | `(base, size)` |
//! | 1 | `alloc` | `(base, size)` |
//! | 2 | `realloc` | `(base, ptr, size)` |
//! | 3 | `free` | `(base, ptr)` |
//!
//! Every call carries the region's base, which is what names the arena: the
//! reference keys its arenas by that word too. So nothing about an arena is
//! kept on the host - the bookkeeping lives in the region itself, the way a C
//! allocator's does, and a title that hands over the same buffer twice gets the
//! same arena back.
//!
//! The layout is a header at the base followed by a chain of blocks:
//!
//! ```text
//! base +0   ArenaHeader { magic, size }
//! base +8   BlockHeader { payload_size, in_use } payload...
//!           BlockHeader { payload_size, in_use } payload...
//!           ...
//! ```
//!
//! A block's payload follows its header, and the next block starts where that
//! payload ends, so the chain is walked from the base rather than linked.

use alloc::vec;

use bytemuck::{Pod, Zeroable};

use wipi_types::wipic::WIPICWord;

use wie_util::{Result, read_generic, write_generic};

use crate::context::WIPICContext;

/// The name a title asks `MC_knlGetDLLInterface` for.
pub const INTERFACE_NAME: &str = "MXUserMemInterf";

/// How many function pointers the interface holds.
pub const INTERFACE_SLOTS: u32 = 4;

/// "MXUM" — written at the base so a call can tell a region that has been
/// handed to `add` from any other word a title passes.
const ARENA_MAGIC: u32 = 0x4D58554D;

const ARENA_HEADER_SIZE: u32 = 8;
const BLOCK_HEADER_SIZE: u32 = 8;

/// Payload alignment. Every size is rounded up to this, so a block header is
/// always reached on a four byte boundary.
const ALIGN: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ArenaHeader {
    magic: u32,
    size: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlockHeader {
    /// Payload bytes, not counting this header.
    payload_size: u32,
    in_use: u32,
}

fn align_up(value: u32) -> u32 {
    value.div_ceil(ALIGN) * ALIGN
}

/// Reads the arena header at `base`, or `None` when `base` does not name one.
fn arena(context: &mut dyn WIPICContext, base: WIPICWord) -> Result<Option<ArenaHeader>> {
    if base == 0 {
        return Ok(None);
    }

    let header: ArenaHeader = read_generic(context, base)?;

    Ok((header.magic == ARENA_MAGIC).then_some(header))
}

/// The addresses of every block in `base`'s chain, in order.
///
/// Walking rather than linking keeps a freed block's neighbours findable, which
/// is what coalescing needs, and costs nothing an arena this size notices.
fn blocks(context: &mut dyn WIPICContext, base: WIPICWord, arena_size: u32) -> Result<vec::Vec<(WIPICWord, BlockHeader)>> {
    let end = base + arena_size;
    let mut out = vec::Vec::new();
    let mut at = base + ARENA_HEADER_SIZE;

    while at + BLOCK_HEADER_SIZE <= end {
        let header: BlockHeader = read_generic(context, at)?;

        // A payload that runs past the region means the chain has been walked
        // off the end of what `add` laid out - stop rather than read further.
        if at + BLOCK_HEADER_SIZE + header.payload_size > end {
            break;
        }

        out.push((at, header));
        at += BLOCK_HEADER_SIZE + header.payload_size;
    }

    Ok(out)
}

/// `add(base, size)` — hand a region over to be allocated from.
///
/// The region is laid out as one free block spanning everything after the
/// header. Returns 0 on success and -1 for a region too small to hold even an
/// empty block, or for a null base.
pub async fn add(context: &mut dyn WIPICContext, base: WIPICWord, size: WIPICWord) -> Result<i32> {
    tracing::debug!("mxusermem.add({base:#x}, {size})");

    if base == 0 || size < ARENA_HEADER_SIZE + BLOCK_HEADER_SIZE {
        tracing::warn!("mxusermem.add({base:#x}, {size}) is not a usable region");
        return Ok(-1);
    }

    // A region that would wrap is one no pointer inside it can be compared
    // against, so it is refused the way the reference refuses it.
    if (base as u64) + (size as u64) > u32::MAX as u64 + 1 {
        tracing::warn!("mxusermem.add({base:#x}, {size}) runs past the address space");
        return Ok(-1);
    }

    write_generic(context, base, ArenaHeader { magic: ARENA_MAGIC, size })?;
    write_generic(
        context,
        base + ARENA_HEADER_SIZE,
        BlockHeader {
            payload_size: size - ARENA_HEADER_SIZE - BLOCK_HEADER_SIZE,
            in_use: 0,
        },
    )?;

    Ok(0)
}

/// `alloc(base, size)` — take `size` bytes out of the region.
///
/// First fit, splitting a block that has room for another header and a payload
/// after it. Answers 0 when the region cannot serve the request, which is what
/// a title checks for.
pub async fn alloc(context: &mut dyn WIPICContext, base: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    let Some(header) = arena(context, base)? else {
        tracing::warn!("mxusermem.alloc({base:#x}, {size}) on a region that was never added");
        return Ok(0);
    };

    // A zero byte request still has to answer a pointer no other allocation
    // holds, so it is served as the smallest block there is.
    let wanted = align_up(size.max(1));

    for (at, block) in blocks(context, base, header.size)? {
        if block.in_use != 0 || block.payload_size < wanted {
            continue;
        }

        // Split only when what is left over can hold a header and a payload of
        // its own; otherwise the remainder goes to this allocation.
        let leftover = block.payload_size - wanted;
        let payload_size = if leftover >= BLOCK_HEADER_SIZE + ALIGN {
            write_generic(
                context,
                at + BLOCK_HEADER_SIZE + wanted,
                BlockHeader {
                    payload_size: leftover - BLOCK_HEADER_SIZE,
                    in_use: 0,
                },
            )?;

            wanted
        } else {
            block.payload_size
        };

        write_generic(context, at, BlockHeader { payload_size, in_use: 1 })?;

        let ptr = at + BLOCK_HEADER_SIZE;
        tracing::debug!("mxusermem.alloc({base:#x}, {size}) -> {ptr:#x}");

        return Ok(ptr);
    }

    tracing::warn!("mxusermem.alloc({base:#x}, {size}) has no room left");

    Ok(0)
}

/// Merges every run of adjacent free blocks into one.
fn coalesce(context: &mut dyn WIPICContext, base: WIPICWord, arena_size: u32) -> Result<()> {
    let chain = blocks(context, base, arena_size)?;

    let mut index = 0;
    while index < chain.len() {
        let (at, block) = chain[index];
        if block.in_use != 0 {
            index += 1;
            continue;
        }

        let mut payload_size = block.payload_size;
        let mut next = index + 1;
        while next < chain.len() && chain[next].1.in_use == 0 {
            payload_size += BLOCK_HEADER_SIZE + chain[next].1.payload_size;
            next += 1;
        }

        if next > index + 1 {
            write_generic(context, at, BlockHeader { payload_size, in_use: 0 })?;
        }

        index = next;
    }

    Ok(())
}

/// `free(base, ptr)` — give a block back.
///
/// A null pointer is a no-op, the way `free(NULL)` is, and so is a pointer that
/// names nothing in this region: a title that frees the same block twice should
/// not take the arena down with it.
pub async fn free(context: &mut dyn WIPICContext, base: WIPICWord, ptr: WIPICWord) -> Result<i32> {
    tracing::debug!("mxusermem.free({base:#x}, {ptr:#x})");

    if ptr == 0 {
        return Ok(0);
    }

    let Some(header) = arena(context, base)? else {
        tracing::warn!("mxusermem.free({base:#x}, {ptr:#x}) on a region that was never added");
        return Ok(-1);
    };

    let chain = blocks(context, base, header.size)?;
    let Some(&(at, block)) = chain.iter().find(|(at, _)| at + BLOCK_HEADER_SIZE == ptr) else {
        tracing::warn!("mxusermem.free({base:#x}, {ptr:#x}) does not name a block");
        return Ok(-1);
    };

    write_generic(
        context,
        at,
        BlockHeader {
            payload_size: block.payload_size,
            in_use: 0,
        },
    )?;

    coalesce(context, base, header.size)?;

    Ok(0)
}

/// `realloc(base, ptr, size)` — resize a block, keeping what it holds.
///
/// The C contract: a null pointer allocates, a zero size frees and answers
/// null, and a request that cannot be served leaves the old block alone and
/// answers null rather than losing it.
pub async fn realloc(context: &mut dyn WIPICContext, base: WIPICWord, ptr: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("mxusermem.realloc({base:#x}, {ptr:#x}, {size})");

    if ptr == 0 {
        return alloc(context, base, size).await;
    }

    if size == 0 {
        free(context, base, ptr).await?;
        return Ok(0);
    }

    let Some(header) = arena(context, base)? else {
        tracing::warn!("mxusermem.realloc({base:#x}, {ptr:#x}, {size}) on a region that was never added");
        return Ok(0);
    };

    let chain = blocks(context, base, header.size)?;
    let Some(&(_, block)) = chain.iter().find(|(at, _)| at + BLOCK_HEADER_SIZE == ptr) else {
        tracing::warn!("mxusermem.realloc({base:#x}, {ptr:#x}, {size}) does not name a block");
        return Ok(0);
    };

    if block.payload_size >= align_up(size) {
        return Ok(ptr);
    }

    let new_ptr = alloc(context, base, size).await?;
    if new_ptr == 0 {
        return Ok(0);
    }

    let mut carried = vec![0u8; block.payload_size as usize];
    context.read_bytes(ptr, &mut carried)?;
    context.write_bytes(new_ptr, &carried)?;

    free(context, base, ptr).await?;

    Ok(new_ptr)
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, vec};

    use wie_backend::{DefaultTaskRunner, System};
    use wie_util::{ByteRead, ByteWrite, Result};

    use test_utils::TestPlatform;

    use crate::context::test::TestContext;

    use super::{add, alloc, free, realloc};

    const BASE: u32 = 0x1000;
    const SIZE: u32 = 0x400;

    fn context() -> TestContext {
        let system = System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner);
        TestContext::with_system(system)
    }

    /// The region a title hands over is the region it is served out of.
    #[futures_test::test]
    async fn an_allocation_comes_out_of_the_region_that_was_added() -> Result<()> {
        let mut context = context();

        assert_eq!(add(&mut context, BASE, SIZE).await?, 0);

        let ptr = alloc(&mut context, BASE, 64).await?;
        assert!(ptr > BASE && ptr + 64 <= BASE + SIZE, "{ptr:#x} is not inside the region");

        Ok(())
    }

    /// Two live allocations never overlap, and what one holds survives the
    /// other being made.
    #[futures_test::test]
    async fn two_allocations_do_not_overlap() -> Result<()> {
        let mut context = context();
        add(&mut context, BASE, SIZE).await?;

        let first = alloc(&mut context, BASE, 32).await?;
        context.write_bytes(first, &[0xab; 32])?;

        let second = alloc(&mut context, BASE, 32).await?;
        context.write_bytes(second, &[0xcd; 32])?;

        assert_ne!(first, second);

        let mut read_back = [0u8; 32];
        context.read_bytes(first, &mut read_back)?;
        assert_eq!(read_back, [0xab; 32]);

        Ok(())
    }

    /// A freed block comes back, and two freed neighbours come back as one -
    /// without coalescing, a title that allocates and frees in a loop runs the
    /// region down into rubble.
    #[futures_test::test]
    async fn freed_blocks_are_reused_and_merged() -> Result<()> {
        let mut context = context();
        add(&mut context, BASE, SIZE).await?;

        let first = alloc(&mut context, BASE, 64).await?;
        let second = alloc(&mut context, BASE, 64).await?;
        assert_eq!(free(&mut context, BASE, first).await?, 0);
        assert_eq!(free(&mut context, BASE, second).await?, 0);

        // 64 + 64 plus the header between them, which only a merge gives back.
        let merged = alloc(&mut context, BASE, 136).await?;
        assert_eq!(merged, first);

        Ok(())
    }

    /// A request the region cannot serve answers zero rather than handing back
    /// a pointer outside it.
    #[futures_test::test]
    async fn a_request_too_big_for_the_region_answers_zero() -> Result<()> {
        let mut context = context();
        add(&mut context, BASE, SIZE).await?;

        assert_eq!(alloc(&mut context, BASE, SIZE * 2).await?, 0);

        Ok(())
    }

    /// Nothing is served out of a region that was never handed over.
    #[futures_test::test]
    async fn a_region_that_was_never_added_serves_nothing() -> Result<()> {
        let mut context = context();

        assert_eq!(alloc(&mut context, BASE, 16).await?, 0);
        assert_eq!(free(&mut context, BASE, BASE + 16).await?, -1);
        assert_eq!(add(&mut context, 0, SIZE).await?, -1);
        assert_eq!(add(&mut context, BASE, 4).await?, -1);

        Ok(())
    }

    /// `realloc` keeps what the block held, and follows the C contract at both
    /// ends: a null pointer allocates and a zero size frees.
    #[futures_test::test]
    async fn realloc_carries_the_contents_over() -> Result<()> {
        let mut context = context();
        add(&mut context, BASE, SIZE).await?;

        let ptr = alloc(&mut context, BASE, 16).await?;
        context.write_bytes(ptr, b"master of sword4")?;

        // Something else behind it, so the block cannot simply grow in place.
        let _pinned = alloc(&mut context, BASE, 16).await?;

        let grown = realloc(&mut context, BASE, ptr, 64).await?;
        assert_ne!(grown, 0);

        let mut read_back = [0u8; 16];
        context.read_bytes(grown, &mut read_back)?;
        assert_eq!(&read_back, b"master of sword4");

        assert_ne!(realloc(&mut context, BASE, 0, 16).await?, 0);
        assert_eq!(realloc(&mut context, BASE, grown, 0).await?, 0);

        Ok(())
    }
}
