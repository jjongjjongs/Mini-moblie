//! Just enough of a class file to read the name off it.
//!
//! KTF runs no bytecode: a title's classes are compiled into its own module and
//! the runtime asks the module for them one at a time. So nothing here reads a
//! class file to run it - it reads one only to learn which class a caller
//! holding the bytes is asking for.

use alloc::{string::String, vec::Vec};

/// `0xcafebabe`, and the only thing about these bytes taken on trust.
const MAGIC: [u8; 4] = [0xca, 0xfe, 0xba, 0xbe];

/// Constant pool tags, of which only two are read: the class entry that
/// `this_class` points at, and the UTF-8 entry that names it. The rest are
/// skipped by their width.
const TAG_UTF8: u8 = 1;
const TAG_CLASS: u8 = 7;

/// The binary name of the class these bytes describe - `Clet$CletCard`, in the
/// form the runtime names classes by - or `None` if they are not a class file
/// or do not hold together.
pub fn class_name(data: &[u8]) -> Option<String> {
    let mut reader = Reader::new(data);

    if reader.bytes(4)? != MAGIC {
        return None;
    }
    reader.skip(4)?; // minor and major version

    // The pool is one-based and its declared count is one past its last entry.
    let count = reader.u16()? as usize;
    let mut entries = Vec::with_capacity(count);
    entries.push(Entry::Other);

    let mut index = 1;
    while index < count {
        let tag = reader.u8()?;
        let entry = match tag {
            TAG_UTF8 => {
                let length = reader.u16()? as usize;
                Entry::Utf8(reader.bytes(length)?)
            }
            TAG_CLASS => Entry::Class(reader.u16()?),
            // A string, a method type, a module or a package: one index.
            8 | 16 | 19 | 20 => {
                reader.skip(2)?;
                Entry::Other
            }
            // A method handle: a kind and an index.
            15 => {
                reader.skip(3)?;
                Entry::Other
            }
            // An int, a float, a reference, a name and type, or either of the
            // dynamic pair: two indexes or a four-byte value.
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => {
                reader.skip(4)?;
                Entry::Other
            }
            // A long or a double is eight bytes, and takes the slot after it
            // too - "a poor choice", as the specification itself puts it.
            5 | 6 => {
                reader.skip(8)?;
                Entry::Wide
            }
            _ => return None,
        };

        let wide = matches!(entry, Entry::Wide);
        entries.push(entry);
        index += 1;

        if wide {
            entries.push(Entry::Other);
            index += 1;
        }
    }

    reader.skip(2)?; // access flags
    let this_class = reader.u16()? as usize;

    let name_index = match entries.get(this_class)? {
        Entry::Class(name_index) => *name_index as usize,
        _ => return None,
    };

    match entries.get(name_index)? {
        // The name is modified UTF-8, which differs from UTF-8 only for a NUL
        // and for anything outside the basic plane - neither of which a class
        // name of this era holds.
        Entry::Utf8(bytes) => String::from_utf8(bytes.to_vec()).ok(),
        _ => None,
    }
}

enum Entry<'a> {
    Utf8(&'a [u8]),
    Class(u16),
    /// A long or a double, which the pool gives two slots to.
    Wide,
    Other,
}

struct Reader<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    fn bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.cursor.checked_add(count)?;
        let bytes = self.data.get(self.cursor..end)?;
        self.cursor = end;

        Some(bytes)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.bytes(count).map(|_| ())
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = self.bytes(2)?;

        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::class_name;

    /// A class file with the pool entries a 2003 compiler emits around the two
    /// that are read, so the skips are exercised rather than assumed.
    fn class_file(name: &[u8]) -> Vec<u8> {
        let mut data = vec![0xca, 0xfe, 0xba, 0xbe, 0, 3, 0, 45];

        // 1 long (two slots), 2 unused, 3 utf8, 4 class, 5 methodref
        data.extend_from_slice(&6u16.to_be_bytes());
        data.push(5);
        data.extend_from_slice(&0u64.to_be_bytes());
        data.push(1);
        data.extend_from_slice(&(name.len() as u16).to_be_bytes());
        data.extend_from_slice(name);
        data.push(7);
        data.extend_from_slice(&3u16.to_be_bytes());
        data.push(10);
        data.extend_from_slice(&[0, 4, 0, 3]);

        data.extend_from_slice(&0x21u16.to_be_bytes()); // access flags
        data.extend_from_slice(&4u16.to_be_bytes()); // this_class

        data
    }

    #[test]
    fn a_class_file_answers_the_name_it_carries() {
        assert_eq!(class_name(&class_file(b"Clet$CletCard")).unwrap(), "Clet$CletCard");
    }

    #[test]
    fn anything_that_is_not_a_class_file_answers_nothing() {
        assert!(class_name(b"").is_none());
        assert!(class_name(b"PK\x03\x04and then some").is_none());

        // Truncated part way through the pool, and again before the name.
        let whole = class_file(b"Clet");
        assert!(class_name(&whole[..12]).is_none());
        assert!(class_name(&whole[..whole.len() - 2]).is_none());
    }
}
