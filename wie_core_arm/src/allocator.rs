mod bucket;
mod list;

use wie_util::Result;

use crate::{
    ArmCore,
    core::{HEAP_BASE, HEAP_SIZE},
};

use self::{
    bucket::{BUCKET_MAX, BucketAllocator},
    list::ListAllocator,
};

pub struct Allocator;

impl Allocator {
    pub fn init(core: &mut ArmCore) -> Result<()> {
        core.map(HEAP_BASE, HEAP_SIZE)?;

        ListAllocator::init(core, HEAP_BASE, HEAP_SIZE / 2)?;
        BucketAllocator::init(core, HEAP_BASE + HEAP_SIZE / 2, HEAP_SIZE / 2)?;

        Ok(())
    }

    pub fn alloc(core: &mut ArmCore, size: u32) -> Result<u32> {
        if size > BUCKET_MAX as _ {
            ListAllocator::alloc(core, HEAP_BASE, HEAP_SIZE / 2, size)
        } else {
            BucketAllocator::alloc(core, HEAP_BASE + HEAP_SIZE / 2, size)
        }
    }

    /// Frees the block at `address`, which half of the heap it lies in saying
    /// which allocator gave it out, and in the buckets which slot - not
    /// `size`, which can be wrong where the address is not: 리얼사커매니저2009
    /// hands `MC_grpCreateImage` a pointer into the middle of a block of its
    /// own, and the word before it, read as the size when the image went,
    /// sent a list address to the buckets, whose index ran off the end. An
    /// address no allocator gave out is now an error to the caller.
    pub fn free(core: &mut ArmCore, address: u32, _size: u32) -> Result<()> {
        Self::free_unsized(core, address)
    }

    pub fn allocation_size(core: &ArmCore, address: u32) -> Result<u32> {
        if address < HEAP_BASE + HEAP_SIZE / 2 {
            ListAllocator::allocation_size(core, address)
        } else {
            BucketAllocator::allocation_size(HEAP_BASE + HEAP_SIZE / 2, address)
        }
    }

    /// The start and size of the bucket slot `address` falls in, if it falls
    /// in one. Every allocation of up to [`BUCKET_MAX`] bytes is such a slot.
    pub fn bucket_slot_of(address: u32) -> Option<(u32, u32)> {
        BucketAllocator::slot_of(HEAP_BASE + HEAP_SIZE / 2, address)
    }

    /// Hands every live allocation whose address `skip` does not want to
    /// `visit`, with its bytes: what a conservative collector scans for
    /// references. A bucket allocation comes with its whole slot, which is at
    /// least what was asked for.
    pub fn scan_live_blocks(core: &ArmCore, mut skip: impl FnMut(u32) -> bool, mut visit: impl FnMut(u32, &[u8])) -> Result<()> {
        ListAllocator::scan_live_blocks(core, HEAP_BASE, HEAP_SIZE / 2, &mut skip, &mut visit)?;
        BucketAllocator::scan_live_blocks(core, HEAP_BASE + HEAP_SIZE / 2, &mut skip, &mut visit)
    }

    pub fn free_unsized(core: &mut ArmCore, address: u32) -> Result<()> {
        if address < HEAP_BASE + HEAP_SIZE / 2 {
            ListAllocator::free(core, address)
        } else {
            BucketAllocator::free_unsized(core, HEAP_BASE + HEAP_SIZE / 2, address)
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use wie_util::{Result, write_generic};

    use crate::ArmCore;

    use super::Allocator;

    #[test]
    fn allocation_size_recovers_bucket_and_list_capacity() -> Result<()> {
        let mut core = ArmCore::new(false, None).unwrap();
        Allocator::init(&mut core)?;

        let bucket = Allocator::alloc(&mut core, 20)?;
        assert_eq!(Allocator::allocation_size(&core, bucket)?, 32);

        let list = Allocator::alloc(&mut core, 513)?;
        assert_eq!(Allocator::allocation_size(&core, list)?, 516);

        Ok(())
    }

    /// A collector walking the heap sees every block in use, with its bytes,
    /// and none that was freed or that it asked to pass.
    #[test]
    fn scan_live_blocks_visits_what_is_in_use() -> Result<()> {
        let mut core = ArmCore::new(false, None).unwrap();
        Allocator::init(&mut core)?;

        let small = Allocator::alloc(&mut core, 8)?;
        let freed = Allocator::alloc(&mut core, 8)?;
        let passed = Allocator::alloc(&mut core, 100)?;
        let large = Allocator::alloc(&mut core, 2000)?;
        Allocator::free(&mut core, freed, 8)?;
        write_generic(&mut core, small, 0x1234_5678u32)?;
        write_generic(&mut core, large + 1996, 0x9abc_def0u32)?;

        let mut seen = Vec::new();
        Allocator::scan_live_blocks(&core, |address| address == passed, |address, bytes| seen.push((address, bytes.to_vec())))?;

        let small_block = seen.iter().find(|(address, _)| *address == small).unwrap();
        assert_eq!(small_block.1.len(), 8);
        assert_eq!(&small_block.1[..4], &0x1234_5678u32.to_le_bytes());

        let large_block = seen.iter().find(|(address, _)| *address == large).unwrap();
        assert_eq!(&large_block.1[1996..2000], &0x9abc_def0u32.to_le_bytes());

        assert!(!seen.iter().any(|(address, _)| *address == freed || *address == passed));

        // An address anywhere in a slot names that slot.
        assert_eq!(Allocator::bucket_slot_of(small + 5), Some((small, 8)));
        assert_eq!(Allocator::bucket_slot_of(large), None);

        Ok(())
    }

    /// A free of an address no allocator gave out fails, whatever size comes
    /// with it, and leaves what was given out as it was.
    #[test]
    fn free_of_an_address_not_given_out_fails() -> Result<()> {
        let mut core = ArmCore::new(false, None).unwrap();
        Allocator::init(&mut core)?;

        let list = Allocator::alloc(&mut core, 0x2000)?;
        let bucket = Allocator::alloc(&mut core, 100)?;
        write_generic(&mut core, list + 0x100, 0x20u32)?;

        assert!(Allocator::free(&mut core, list + 0x104, 0x24).is_err());
        assert!(Allocator::free(&mut core, bucket + 4, 100).is_err());

        Allocator::free(&mut core, list, 0x2000)?;
        Allocator::free(&mut core, bucket, 100)?;
        assert_eq!(Allocator::alloc(&mut core, 100)?, bucket);

        Ok(())
    }

    #[test]
    fn free_unsized_recovers_bucket_and_list_allocations() -> Result<()> {
        let mut core = ArmCore::new(false, None).unwrap();
        Allocator::init(&mut core)?;

        // 20 bytes is served by the 32-byte bucket. Unsized free must recover
        // that class from the returned address rather than from a caller size.
        let bucket = Allocator::alloc(&mut core, 20)?;
        Allocator::free_unsized(&mut core, bucket)?;
        let bucket_again = Allocator::alloc(&mut core, 20)?;
        assert_eq!(bucket_again, bucket);

        // 513 bytes crosses BUCKET_MAX and is served by the list allocator.
        // The heap-half address alone must select the list free path.
        let list = Allocator::alloc(&mut core, 513)?;
        Allocator::free_unsized(&mut core, list)?;
        let list_again = Allocator::alloc(&mut core, 513)?;
        assert_eq!(list_again, list);

        Ok(())
    }
}
