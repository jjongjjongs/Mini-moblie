use alloc::{string::String, vec::Vec};

use wie_util::{Result, WieError};

/// One of a script's variables: where its elements start in the word memory.
/// Every variable, constant or not, starts from the
/// values the script stores for it.
#[derive(Clone, Copy)]
pub struct Variable {
    pub offset: usize,
}

/// A GNEX script in the version-4 layout: code, the variable and resource
/// tables, and the eight callbacks. See `docs/gnex_sgs_v4_notes.md`.
pub struct Program {
    /// The script's bytes after the download wrapper; every offset in it is
    /// relative to its start.
    pub data: Vec<u8>,
    pub title: String,
    /// Initialization, termination, timer, key, two message callbacks and two
    /// system callbacks.
    pub entries: [usize; 8],
    pub code_end: usize,
    pub variables: Vec<Variable>,
    pub memory: Vec<i32>,
    pub resources: Vec<Vec<u8>>,
}

/// Where the code starts: the version-4 header is 128 bytes.
pub const CODE_START: usize = 0x80;

fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    data.get(offset..offset + 2)
        .map(|x| u16::from_le_bytes([x[0], x[1]]))
        .ok_or_else(|| WieError::FatalError("SGS: truncated".into()))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    data.get(offset..offset + 4)
        .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
        .ok_or_else(|| WieError::FatalError("SGS: truncated".into()))
}

/// The script with a download's 32-byte wrapper taken off: its first word is
/// its own length.
pub fn unwrap(data: &[u8]) -> &[u8] {
    if data.len() >= 32 && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == 32 {
        &data[32..]
    } else {
        data
    }
}

impl Program {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let data = unwrap(data);
        if data.len() < CODE_START || data[0] != 4 {
            return Err(WieError::FatalError("SGS: not a version 4 script".into()));
        }

        let mut entries = [0; 8];
        for (i, entry) in entries.iter_mut().enumerate() {
            *entry = u32_at(data, 0x1c + i * 4)? as usize;
        }
        let vd = u32_at(data, 0x48)? as usize;
        let vi = u32_at(data, 0x4c)? as usize;
        let rd = u32_at(data, 0x50)? as usize;
        let ri = u32_at(data, 0x54)? as usize;
        let ordered = CODE_START <= vd && vd <= vi && vi <= rd && rd <= ri && ri <= data.len();
        if !(ordered && (vi - vd).is_multiple_of(8) && (ri - rd).is_multiple_of(12)) {
            return Err(WieError::FatalError("SGS: invalid regions".into()));
        }
        if entries.iter().any(|&x| x != 0 && !(CODE_START..vd).contains(&x)) {
            return Err(WieError::FatalError("SGS: callback outside code".into()));
        }

        // Variables: (flags u16, count u16, u32), each initialized from 32-bit
        // values that follow in order.
        let mut variables = Vec::new();
        let mut memory = Vec::new();
        let mut cursor = vi;
        for i in 0..(vi - vd) / 8 {
            let count = u16_at(data, vd + i * 8 + 2)? as usize;
            variables.push(Variable { offset: memory.len() });
            for j in 0..count {
                memory.push(u32_at(data, cursor + j * 4)? as i32);
            }
            cursor += count * 4;
        }

        // Resources: (flags u32, size u32, u32), their bytes following in order.
        let mut resources = Vec::new();
        let mut cursor = ri;
        for i in 0..(ri - rd) / 12 {
            let size = u32_at(data, rd + i * 12 + 4)? as usize;
            let bytes = data
                .get(cursor..cursor + size)
                .ok_or_else(|| WieError::FatalError("SGS: truncated resource".into()))?;
            resources.push(bytes.to_vec());
            cursor += size;
        }

        let title = data[10..28].split(|&x| x == 0).next().unwrap_or(&[]);
        let (title, _, _) = encoding_rs::EUC_KR.decode(title);

        Ok(Self {
            data: data.to_vec(),
            title: title.trim().into(),
            entries,
            code_end: vd,
            variables,
            memory,
            resources,
        })
    }
}
