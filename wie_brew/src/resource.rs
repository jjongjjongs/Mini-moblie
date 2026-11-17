use alloc::{format, vec::Vec};

use wie_util::{Result, WieError};

const HEADER_SIZE: usize = 0x20;
const VERSION: u16 = 0x11;
const GROUP_STRIDE: usize = 8;
const MAX_GROUPS: usize = 1 << 12;
const MAX_ITEMS: usize = 1 << 16;

/// One run of numbered items of a kind: numbers `first..=first + span` are
/// items `item..` of the index.
struct Group {
    kind: u16,
    first: u16,
    span: u16,
    item: u16,
}

/// A BREW resource file (`.bar`, here under the title's own names), which the
/// title loads items out of by kind and number.
///
/// ```text
/// 0x00  u16 version, 0x11
/// 0x06  u16 group count
/// 0x08  u32 group table offset
/// 0x0c  u32 group table length
/// 0x10  u32 index table offset
/// 0x14  u32 item count
/// 0x18  u32 offset of the first item, repeating the index's first entry
/// ```
///
/// The index holds one offset per item and then the end of the last one.
pub struct ResourceFile {
    data: Vec<u8>,
    groups: Vec<Group>,
    index: Vec<u32>,
}

impl ResourceFile {
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(WieError::FatalError(format!("resource file is {} bytes, too short", data.len())));
        }

        let u16_at = |offset: usize| u16::from_le_bytes([data[offset], data[offset + 1]]);
        let u32_at = |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());

        if u16_at(0) != VERSION {
            return Err(WieError::FatalError(format!("resource file names version {:#x}", u16_at(0))));
        }

        let group_count = u16_at(0x06) as usize;
        let group_table = u32_at(0x08) as usize;
        let group_bytes = u32_at(0x0c) as usize;
        let index_table = u32_at(0x10) as usize;
        let item_count = u32_at(0x14) as usize;

        if group_count == 0 || group_count > MAX_GROUPS || item_count == 0 || item_count > MAX_ITEMS {
            return Err(WieError::FatalError(format!(
                "resource file declares {group_count} groups of {item_count} items"
            )));
        }
        if group_bytes != group_count * GROUP_STRIDE || group_table + group_bytes > data.len() {
            return Err(WieError::FatalError("resource file's group table is not inside it".into()));
        }
        if index_table + (item_count + 1) * 4 > data.len() {
            return Err(WieError::FatalError("resource file's index is not inside it".into()));
        }

        let groups = (0..group_count)
            .map(|index| {
                let record = group_table + index * GROUP_STRIDE;
                Group {
                    kind: u16_at(record),
                    first: u16_at(record + 2),
                    span: u16_at(record + 4),
                    item: u16_at(record + 6),
                }
            })
            .collect::<Vec<_>>();
        let index = (0..=item_count).map(|item| u32_at(index_table + item * 4)).collect::<Vec<_>>();

        if index[0] != u32_at(0x18) || index.windows(2).any(|x| x[1] < x[0]) || *index.last().unwrap() as usize > data.len() {
            return Err(WieError::FatalError("resource file's index is out of order".into()));
        }

        Ok(Self { data, groups, index })
    }

    /// The bytes of item `number` of `kind`, if the file carries it.
    pub fn item(&self, kind: u16, number: u16) -> Option<&[u8]> {
        let group = self
            .groups
            .iter()
            .find(|group| group.kind == kind && number >= group.first && number as u32 <= group.first as u32 + group.span as u32)?;

        let item = group.item as usize + (number - group.first) as usize;
        if item + 1 >= self.index.len() {
            return None;
        }

        Some(&self.data[self.index[item] as usize..self.index[item + 1] as usize])
    }
}
