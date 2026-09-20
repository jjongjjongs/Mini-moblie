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

/// The header a relocated module carries: a bss size, a relocation count, a
/// reserved word, and then the relocations.
const HEADER_WORDS: usize = 3;

/// A module the loader is expected to relocate itself.
///
/// The header is three words - the bss size, how many relocations follow, and
/// a word that is zero in the one archive that has this - then that many byte
/// offsets into the image, then the image. wfeature, which runs 텐가이, agrees:
/// it refuses a `client.binN` whose first word does not match the suffix
/// ("KTF client image %q suffix names BSS %d but image specifies %d") and
/// biases every word the table names by where the image lands.
///
/// What is known of the image, offsets its own:
///
/// | word | 텐가이's | what it is                                    |
/// |------|-----------|-----------------------------------------------|
/// | +00  | 0         |                                               |
/// | +04  | 0x5f2c0   | a word past the image - bss                    |
/// | +08  | 0x57344   | the imported names, NUL separated              |
/// | +0c  | 0x5772c   | pointers into the constant pool                |
/// | +10  | 0x480     | where the code starts                          |
/// | +14  | 0x5ed6c   | the module field table                         |
/// | +18  | 0x5f2a4   | bss again                                      |
/// | +20  | 0x52745   | Thumb, and the only odd word: the entry        |
///
/// Everything but `+00`, `+04` and `+20` is relocated.
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
        if word(2)? != 0 || relocations == 0 {
            return None;
        }

        let image_offset = (HEADER_WORDS + relocations) * size_of::<u32>();
        let image_size = data.len().checked_sub(image_offset)?;
        if image_size == 0 {
            return None;
        }

        let mut highest = 0;
        let mut climbing = 0;
        for index in 0..relocations {
            let offset = word(HEADER_WORDS + index)?;
            if offset as usize >= image_size || offset % size_of::<u32>() as u32 != 0 {
                return None;
            }

            if offset >= highest {
                climbing += 1;
            }
            highest = highest.max(offset);
        }

        // One of 텐가이's 3,271 offsets goes backwards, so this is "climbs"
        // rather than "is sorted" - but a table of anything else would not
        // climb at all.
        if climbing * 100 < relocations * 99 {
            return None;
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

    /// The word the image keeps its entry in, at `+0x20`.
    ///
    /// Not in the relocation table: the loader is what biases it, the same way
    /// it biases every word the table does name.
    pub const ENTRY_OFFSET: usize = 0x20;

    /// Where the image's entry is, once the image is at its base.
    pub fn entry(&self, data: &[u8], load_address: u32) -> Result<u32> {
        let at = self.image_offset + Self::ENTRY_OFFSET;
        let word = data
            .get(at..at + size_of::<u32>())
            .ok_or_else(|| WieError::FatalError(format!("a relocated module with no entry word at {at:#x}")))?;

        Ok(self.base(load_address).wrapping_add(u32::from_le_bytes(word.try_into().unwrap())))
    }

    /// What to tell the user, since this runtime cannot run one yet.
    ///
    /// What is left is named rather than guessed at. The table does not cover
    /// the words from `+0x5ed64` to the end of 텐가이's image, and its entry
    /// reads one of them the moment it runs - 333 of those 335 words are
    /// image offsets, so they are rebased too, by something else. wfeature has
    /// that something: `rebaseModuleFields`, which reads six words of a module
    /// field table (the `+0x14` word) and refuses a client that is not one of
    /// these ("KTF client is not a relocatable module").
    ///
    /// Past that, the entry takes a pointer to a table of the host's own
    /// functions and calls the first of them with a name and two `-1`s, which
    /// is `get_interface` - the same call this runtime already serves at
    /// `InitSvcId::GetInterface`. The name it asks for is `MNInterface`, which
    /// nothing else here asks for.
    pub fn unsupported(&self, filename: &str) -> WieError {
        WieError::FatalError(format!(
            "{filename} is a relocated module - {} relocations, image at {:#x}, bss {:#x} - and this runtime relocates one but cannot yet \
             rebase its module fields or serve the MNInterface its entry asks for. See wie_ktf::module.",
            self.relocations, self.image_offset, self.bss_size
        ))
    }
}

/// Reading one is the caller's business; this is only the recogniser.
pub fn reject_if_relocated(filename: &str, data: &[u8]) -> Result<()> {
    match RelocatedModule::parse(data) {
        Some(module) => Err(module.unsupported(filename)),
        None => Ok(()),
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
        data.extend_from_slice(&0u32.to_le_bytes());
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
        assert_eq!(module.image_offset, (3 + 4) * 4);
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
        image[0x20..0x24].copy_from_slice(&0x52745u32.to_le_bytes());

        let data = relocated(&[8, 0xc], &image);
        let module = RelocatedModule::parse(&data).expect("a relocated module");

        assert_eq!(module.base(0x100000), 0x100000 + (3 + 2) * 4);
        assert_eq!(module.entry(&data, 0x100000).unwrap(), 0x100000 + (3 + 2) * 4 + 0x52745);
    }

    /// The offsets come back in the order the table holds them.
    #[test]
    fn the_offsets_are_the_table() {
        let data = relocated(&[0x10, 0x20, 0x2c], &[0u8; 0x40]);
        let module = RelocatedModule::parse(&data).expect("a relocated module");

        assert_eq!(module.offsets(&data).collect::<Vec<_>>(), vec![0x10, 0x20, 0x2c]);
    }
}
