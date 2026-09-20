//! What a `client.bin` is, when it is not the one this runtime knows.
//!
//! Fifty-six of the fifty-seven KTF archives here carry the same module: the
//! image starts with its own Thumb stub, the loader calls that stub with the
//! bss size and the module rebases itself and hands back a `WipiExe`. See
//! `crate::runtime::init::load_native`.
//!
//! 텐가이 (01031C47) carries a different one, and reading it as the first kind
//! meant branching into its header - `Invalid memory access; address: 28`,
//! which says nothing about why. Recognising it is what turns that into a
//! sentence.

use alloc::format;
use core::mem::size_of;

use wie_core_arm::ArmCore;
use wie_util::{Result, WieError, read_generic, write_generic};

/// The header a relocated module carries: a bss size and a relocation count,
/// and then the relocations.
///
/// Two words and not three. A third would leave the last offset out of order -
/// it would read the image's own first word as a relocation - and wfeature
/// reads the entry at `image + 0x24`, which is only the odd word when the
/// image starts here.
const HEADER_WORDS: usize = 2;

/// A module the loader is expected to relocate itself.
///
/// The header is two words - the bss size and how many relocations follow -
/// then that many byte offsets into the image, then the image. wfeature, which runs 텐가이, agrees:
/// it refuses a `client.binN` whose first word does not match the suffix
/// ("KTF client image %q suffix names BSS %d but image specifies %d") and
/// biases every word the table names by where the image lands.
///
/// What is known of the image, offsets its own:
///
/// | word | 텐가이's    | what it is                                  |
/// |------|-------------|---------------------------------------------|
/// | +00  | 0x5e7bc     |                                             |
/// | +04  | 0           |                                             |
/// | +08  | 0x5f2c0     | a word past the image - bss                  |
/// | +0c  | 0x57344     | the imported names, NUL separated             |
/// | +10  | 0x5772c     | pointers into the constant pool               |
/// | +14  | 0x480       | the six words the host fills in               |
/// | +18  | 0x5ed6c     | the module field table                        |
/// | +1c  | 0x5f2a4     | bss again                                     |
/// | +20  | 0x13580001  | where the VM context goes, until it is there  |
/// | +24  | 0x52745     | Thumb, and the only odd word: the entry       |
///
/// Everything but `+04` and `+20` is relocated, the entry included.
pub struct RelocatedModule {
    pub bss_size: u32,
    pub relocations: usize,
    /// Where the image begins in the file.
    pub image_offset: usize,
}

impl RelocatedModule {
    /// Whether `data` is one of these, read from the shape of its own header.
    ///
    /// The test is the relocation table checking out: the count has to fit the
    /// file, every offset it names has to fall inside the image that follows
    /// it, and the offsets have to climb. A module of the ordinary kind starts
    /// with Thumb code, whose first words are far too small to be a count that
    /// fits, so it never reaches the rest.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let word = |index: usize| {
            let at = index * size_of::<u32>();
            Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
        };

        let bss_size = word(0)?;
        let relocations = word(1)? as usize;
        if relocations == 0 {
            return None;
        }

        let image_offset = (HEADER_WORDS + relocations) * size_of::<u32>();
        let image_size = data.len().checked_sub(image_offset)?;
        if image_size == 0 {
            return None;
        }

        // Sorted, word aligned, and every one of them inside the image that
        // follows. A module of the ordinary kind starts with Thumb code, whose
        // first words are far too small to be a count that fits, so it never
        // gets this far.
        let mut previous = None;
        for index in 0..relocations {
            let offset = word(HEADER_WORDS + index)?;
            if offset as usize >= image_size || offset % size_of::<u32>() as u32 != 0 {
                return None;
            }

            if previous.is_some_and(|previous| offset < previous) {
                return None;
            }
            previous = Some(offset);
        }

        Some(Self {
            bss_size,
            relocations,
            image_offset,
        })
    }

    /// The relocations, as byte offsets into the image.
    pub fn offsets<'a>(&'a self, data: &'a [u8]) -> impl Iterator<Item = u32> + 'a {
        (0..self.relocations).map(move |index| {
            let at = (HEADER_WORDS + index) * size_of::<u32>();

            u32::from_le_bytes(data[at..at + size_of::<u32>()].try_into().unwrap())
        })
    }

    /// Where the image lands, when the file is loaded at `load_address`.
    ///
    /// The whole file goes in - header, table and image - so the image itself
    /// sits behind the two, and that is the bias, not the load address.
    pub fn base(&self, load_address: u32) -> u32 {
        load_address + self.image_offset as u32
    }

    /// Adds the image's own base to every word the table names.
    ///
    /// This is wfeature's loop at `0x3f71cc`, which reads the word at
    /// `image + offset`, adds `image_offset + load address` and writes it
    /// back.
    pub fn relocate(&self, core: &mut ArmCore, data: &[u8], load_address: u32) -> Result<()> {
        let base = self.base(load_address);

        for offset in self.offsets(data) {
            let at = base + offset;
            let word: u32 = read_generic(core, at)?;

            write_generic(core, at, word.wrapping_add(base))?;
        }

        Ok(())
    }

    /// The word the image keeps its module field table in, at `+0x18`.
    pub const MODULE_FIELDS_OFFSET: usize = 0x18;

    /// One word of the image's header, as it is on the wire.
    fn header_word(&self, data: &[u8], offset: usize) -> Result<u32> {
        let at = self.image_offset + offset;
        let word = data
            .get(at..at + size_of::<u32>())
            .ok_or_else(|| WieError::FatalError(format!("a relocated module with no word at {offset:#x}")))?;

        Ok(u32::from_le_bytes(word.try_into().unwrap()))
    }

    /// The word the image keeps its module descriptor in, at `+0x00`.
    pub const DESCRIPTOR_OFFSET: usize = 0x00;

    /// Where the module descriptor is, as an image offset.
    ///
    /// Six words, and the sixth is the descriptor's own address - which is how
    /// wfeature knows it is holding one, and what it refuses a client for
    /// ("KTF client is not a relocatable module"). The first three are the
    /// class table: the buckets, how many classes are in them, and how many
    /// buckets there are. 텐가이's are 22 and 32, and its buckets sit in the
    /// 0x80 bytes immediately before the descriptor.
    pub fn descriptor(&self, data: &[u8]) -> Result<u32> {
        self.header_word(data, Self::DESCRIPTOR_OFFSET)
    }

    /// Where the module field table is, as an image offset.
    pub fn module_fields(&self, data: &[u8]) -> Result<u32> {
        self.header_word(data, Self::MODULE_FIELDS_OFFSET)
    }

    /// The word the host writes the module's VM context into, at `+0x20`.
    ///
    /// One of the two words the relocation table leaves alone, and the module
    /// reads it through a field of its own: the epilogue of a method with a
    /// `try` in it puts the handler chain back through `+0x2c` of whatever is
    /// here. The `0x13580001` it holds in the file is nobody's address - it is
    /// what an unfilled one looks like.
    pub const VM_CONTEXT_OFFSET: usize = 0x20;

    /// The word the image keeps its jump table in, at `+0x14`.
    pub const JUMP_TABLE_OFFSET: usize = 0x14;

    /// How many entries that table has.
    ///
    /// Six, which is what wfeature reads of it and how many words of 텐가이's
    /// image are zero there - 0x1037a4 to 0x1037b8, with code either side.
    pub const JUMP_TABLE_ENTRIES: u32 = 6;

    /// Where the table of host entry points is, as an image offset.
    ///
    /// The module tail-jumps through these - `ldr r2, [pc, #n]; mov pc, r2`,
    /// never a call - so whatever is behind one answers with `lr` the way the
    /// module's own code would. They are left zero in the file: the host that
    /// loads the module writes them, and a module whose table is still zero
    /// branches to zero the first time it invokes a method.
    pub fn jump_table(&self, data: &[u8]) -> Result<u32> {
        self.header_word(data, Self::JUMP_TABLE_OFFSET)
    }

    /// The word the image keeps its constant pool in, at `+0x10`.
    pub const CONSTANT_POOL_OFFSET: usize = 0x10;

    /// Where the constant pool's pointers are, as an image offset.
    ///
    /// An array of pointers into the names at `+0x0c`, and what an unresolved
    /// import is an index into: a class whose parent is not in this image
    /// carries `(index << 1) | 1` where the parent's address goes, and the
    /// name at that index is what it is waiting for. See
    /// `crate::runtime::init::resolve_module_imports`.
    pub fn constant_pool(&self, data: &[u8]) -> Result<u32> {
        self.header_word(data, Self::CONSTANT_POOL_OFFSET)
    }

    /// Adds the image's base to every word of the module field table.
    ///
    /// The relocation table stops short of it - 텐가이's last offset is
    /// `0x5ed60` and its table starts at `0x5ed6c` - and the module reads one
    /// of those words the moment its entry runs, so something else has to
    /// bias them. wfeature has that something, as `rebaseModuleFields`, and
    /// what it rebases is this: the table runs from the word at `+0x18` to the
    /// end of the image, 334 words of 텐가이's, and every one of them is an
    /// offset into the image.
    ///
    /// Done after [`RelocatedModule::relocate`], because the word that says
    /// where the table is is itself one the relocation table names.
    pub fn rebase_module_fields(&self, core: &mut ArmCore, data: &[u8], load_address: u32) -> Result<()> {
        let base = self.base(load_address);
        let table = self.module_fields(data)?;
        let image_size = (data.len() - self.image_offset) as u32;

        if table >= image_size {
            return Err(WieError::FatalError(format!(
                "a relocated module whose field table at {table:#x} is past its {image_size:#x} bytes"
            )));
        }

        for at in (table..image_size).step_by(size_of::<u32>()) {
            if at + size_of::<u32>() as u32 > image_size {
                break;
            }

            let word: u32 = read_generic(core, base + at)?;
            write_generic(core, base + at, word.wrapping_add(base))?;
        }

        Ok(())
    }

    /// The word the image keeps its entry in, at `+0x24`, which is where
    /// wfeature reads it.
    ///
    /// It is in the relocation table too, so the image's own copy is biased as
    /// well - but the loader needs it before it has run anything, so it takes
    /// it off the wire and biases it itself.
    pub const ENTRY_OFFSET: usize = 0x24;

    /// Where the image's entry is, once the image is at its base.
    pub fn entry(&self, data: &[u8], load_address: u32) -> Result<u32> {
        let at = self.image_offset + Self::ENTRY_OFFSET;
        let word = data
            .get(at..at + size_of::<u32>())
            .ok_or_else(|| WieError::FatalError(format!("a relocated module with no entry word at {at:#x}")))?;

        Ok(self.base(load_address).wrapping_add(u32::from_le_bytes(word.try_into().unwrap())))
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::RelocatedModule;

    /// A module of this kind, built the way 텐가이's is.
    fn relocated(relocations: &[u32], image: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&64u32.to_le_bytes());
        data.extend_from_slice(&(relocations.len() as u32).to_le_bytes());
        for offset in relocations {
            data.extend_from_slice(&offset.to_le_bytes());
        }
        data.extend_from_slice(image);

        data
    }

    #[test]
    fn a_relocated_module_is_read_off_its_own_header() {
        let data = relocated(&[8, 0xc, 0x10, 0x20], &[0u8; 0x40]);

        let module = RelocatedModule::parse(&data).expect("a relocated module");
        assert_eq!(module.bss_size, 64);
        assert_eq!(module.relocations, 4);
        assert_eq!(module.image_offset, (2 + 4) * 4);
    }

    /// The module every other archive carries, which starts with its own Thumb
    /// stub and rebases itself.
    #[test]
    fn a_self_rebasing_module_is_not_one() {
        // 헬싱's first bytes: `b .+8`, `nop`, then its own words.
        let data = [
            0x04, 0xe0, 0xc0, 0x46, 0x24, 0x02, 0x04, 0x20, 0x01, 0x00, 0x02, 0x00, 0x01, 0xb5, 0x15, 0x49,
        ];

        assert!(RelocatedModule::parse(&data).is_none());
    }

    /// Nothing is read out of a file too short to hold what its header claims.
    #[test]
    fn a_header_that_overruns_its_file_is_not_one() {
        let mut data = relocated(&[8, 0xc], &[0u8; 0x40]);
        data.truncate(16);

        assert!(RelocatedModule::parse(&data).is_none());

        // A count that leaves no image behind it is not one either.
        assert!(RelocatedModule::parse(&relocated(&[8, 0xc], &[])).is_none());
    }

    /// An offset outside the image it is meant to point into is not a
    /// relocation, whatever else the header looks like.
    #[test]
    fn an_offset_past_the_image_is_not_one() {
        assert!(RelocatedModule::parse(&relocated(&[8, 0x400], &[0u8; 0x40])).is_none());
        // Nor is an unaligned one.
        assert!(RelocatedModule::parse(&relocated(&[8, 0xd], &[0u8; 0x40])).is_none());
    }

    /// The image's base is behind the header and the table, and the entry is
    /// behind that.
    ///
    /// The numbers are 텐가이's: loaded at 0x100000, its image lands at
    /// 0x103328 and its entry - the word at `+0x20` - at 0x155a6d, Thumb bit
    /// and all.
    #[test]
    fn the_entry_is_behind_the_header_and_the_table() {
        let mut image = vec![0u8; 0x40];
        image[0x24..0x28].copy_from_slice(&0x52745u32.to_le_bytes());

        let data = relocated(&[8, 0xc], &image);
        let module = RelocatedModule::parse(&data).expect("a relocated module");

        assert_eq!(module.base(0x100000), 0x100000 + (2 + 2) * 4);
        assert_eq!(module.entry(&data, 0x100000).unwrap(), 0x100000 + (2 + 2) * 4 + 0x52745);
    }

    /// The offsets come back in the order the table holds them.
    #[test]
    fn the_offsets_are_the_table() {
        let data = relocated(&[0x10, 0x20, 0x2c], &[0u8; 0x40]);
        let module = RelocatedModule::parse(&data).expect("a relocated module");

        assert_eq!(module.offsets(&data).collect::<Vec<_>>(), vec![0x10, 0x20, 0x2c]);
    }
}
