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

use wie_util::{Result, WieError};

/// The header a relocated module carries: a bss size, a relocation count, a
/// reserved word, and then the relocations.
const HEADER_WORDS: usize = 3;

/// A module the loader is expected to relocate itself.
///
/// The header is three words - the bss size, how many relocations follow, and
/// a word that is zero in the one archive that has this - then that many
/// byte offsets into the image, then the image.
///
/// What is known of the image, from 텐가이's (offsets are the image's own):
///
/// | word | value    | what it is                                          |
/// |------|----------|-----------------------------------------------------|
/// | +00  | 0        |                                                     |
/// | +04  | 0x5f2c0  | a word past the image - bss                          |
/// | +08  | 0x57344  | the imported names, NUL separated                    |
/// | +0c  | 0x5772c  | pointers into the constant pool                      |
/// | +10  | 0x480    | where the code starts                                |
/// | +14  | 0x5ed6c  | a table of name and target pairs                     |
/// | +18  | 0x5f2a4  | bss again                                            |
/// | +20  | 0x52745  | Thumb, and the only odd word: an entry               |
///
/// Everything but `+00`, `+04` and `+20` is relocated.
///
/// It is not a WIPI exe. It carries neither `WIPI_exe` nor `ExeInterface` -
/// the two names every other module here has - and the first name in its
/// import table is `MNInterface`, which nothing else asks for. So running it
/// needs a second module ABI and not only a second container, and until that
/// is written, saying so is the useful thing to do.
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

    /// What to tell the user, since this runtime cannot run one yet.
    pub fn unsupported(&self, filename: &str) -> WieError {
        WieError::FatalError(format!(
            "{filename} is a relocated module - {} relocations, image at {:#x}, bss {:#x} - and not the self-rebasing kind this runtime loads. \
             It is not a WIPI exe either: it carries no ExeInterface and asks for an interface named MNInterface. See wie_ktf::module.",
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
}
