//! Neutralising the SK-VM phone-number licence check.
//!
//! A family of SK-VM titles binds itself to the handset it was bought on: a
//! method reads `MIN` - the phone number - and `m.CARRIER` from the system
//! properties, hashes them together with the download's `SERVICE_ID` and a
//! fixed salt, and compares the digest against the `MIDlet-Key` the download
//! server wrote into the descriptor. On any handset but the buyer's the digest
//! differs, so the title draws a licence-error box and, a couple of seconds
//! later, calls `System.exit` - 스페셜포스 does exactly this and closes itself
//! the moment it starts.
//!
//! The digest cannot be matched without the original phone number, so the
//! check is neutralised rather than satisfied: its closing `String.equals` is
//! rewritten to always yield true. This is the transform the reference
//! emulator makes (wfeature, `prepareAuthentication`). `pop2; iconst_1; nop`
//! has the stack effect and the width of the `invokevirtual` it replaces, so
//! the class stays the same length and every branch target and exception range
//! stays valid; the title still reads its properties, builds its digest and
//! catches its own exceptions as before, only the final comparison is forced.

use alloc::{borrow::Cow, vec::Vec};

/// The descriptor of the licence check: `boolean check(MIDlet)`.
const CHECK_DESCRIPTOR: &[u8] = b"(Ljavax/microedition/midlet/MIDlet;)Z";

/// Return `class` with any SK-VM licence check neutralised, borrowing it
/// unchanged when there is nothing to do.
///
/// The scan is gated on the three descriptor strings the check is built from,
/// none of which a KTF or LGT class carries, so it is a no-op for every class
/// but a copy-protected SK-VM title's.
pub fn neutralize_license_check(class: &[u8]) -> Cow<'_, [u8]> {
    match patch_offsets(class) {
        Some(offsets) if !offsets.is_empty() => {
            let mut out = class.to_vec();
            for offset in offsets {
                // `invokevirtual String.equals` (0xb6 hi lo) -> `pop2; iconst_1; nop`.
                out[offset] = 0x58;
                out[offset + 1] = 0x04;
                out[offset + 2] = 0x00;
            }
            Cow::Owned(out)
        }
        _ => Cow::Borrowed(class),
    }
}

/// The byte offsets of the `invokevirtual` that closes each licence check, or
/// `None` when the class is not one (or cannot be read).
fn patch_offsets(class: &[u8]) -> Option<Vec<usize>> {
    if !contains(class, b"MIDlet-Key") || !contains(class, b"SERVICE_ID=") || !contains(class, b"MIN") {
        return None;
    }

    let mut reader = Reader { data: class, pos: 0 };
    if reader.u32()? != 0xCAFE_BABE {
        return None;
    }
    reader.skip(4)?; // minor, major version

    let constants = reader.constant_pool()?;

    reader.skip(6)?; // access flags, this class, super class
    let interfaces = reader.u16()? as usize;
    reader.skip(interfaces * 2)?;
    reader.skip_members()?; // fields

    let mut offsets = Vec::new();
    let methods = reader.u16()?;
    for _ in 0..methods {
        reader.skip(2)?; // access flags
        let _name = reader.u16()?;
        let descriptor = reader.u16()?;
        let is_check = constants.utf8(descriptor) == Some(CHECK_DESCRIPTOR);

        let attributes = reader.u16()?;
        for _ in 0..attributes {
            let name = reader.u16()?;
            let length = reader.u32()? as usize;
            let body = reader.pos;
            reader.skip(length)?;

            if !is_check || constants.utf8(name) != Some(b"Code") {
                continue;
            }
            // Code: u16 max_stack, u16 max_locals, u32 code_length, code[...].
            let code_length = u32_at(class, body + 4)? as usize;
            let code_start = body + 8;
            let code_end = code_start.checked_add(code_length)?;
            if code_length < 4 || code_end > class.len() {
                continue;
            }
            // The check ends with `invokevirtual java/lang/String.equals; ireturn`.
            let code = &class[code_start..code_end];
            if code[code_length - 4] != 0xb6 || code[code_length - 1] != 0xac {
                continue;
            }
            let method_ref = u16_at(class, code_start + code_length - 3)? as u16;
            if !constants.is_string_equals(method_ref) {
                continue;
            }
            offsets.push(code_start + code_length - 4);
        }
    }

    Some(offsets)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|window| window == needle)
}

fn u16_at(data: &[u8], offset: usize) -> Option<usize> {
    data.get(offset..offset + 2).map(|b| ((b[0] as usize) << 8) | b[1] as usize)
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// The constant pool, kept as just enough of each entry to resolve a method
/// reference's class and name back to their UTF-8 text.
struct ConstantPool<'a> {
    data: &'a [u8],
    /// One entry per pool index (1-based; index 0 is a placeholder). A `Long`
    /// or `Double` leaves the following index a placeholder too, as the format
    /// requires.
    entries: Vec<Constant>,
}

#[derive(Clone, Copy)]
enum Constant {
    /// A UTF-8 string, as an offset and length into the class bytes.
    Utf8(usize, usize),
    /// A `Class`, holding its name index.
    Class(u16),
    /// A `NameAndType`, holding its name and descriptor indices.
    NameAndType(u16, u16),
    /// A `Methodref`/`Fieldref`/etc., holding its class and name-and-type indices.
    Reference(u16, u16),
    Other,
}

impl<'a> ConstantPool<'a> {
    fn get(&self, index: u16) -> Option<Constant> {
        self.entries.get(index as usize).copied()
    }

    fn utf8(&self, index: u16) -> Option<&'a [u8]> {
        match self.get(index)? {
            Constant::Utf8(offset, length) => self.data.get(offset..offset + length),
            _ => None,
        }
    }

    /// Whether the method reference at `index` is `java/lang/String.equals`.
    fn is_string_equals(&self, index: u16) -> bool {
        let Some(Constant::Reference(class, name_and_type)) = self.get(index) else {
            return false;
        };
        let Some(Constant::Class(class_name)) = self.get(class) else {
            return false;
        };
        if self.utf8(class_name) != Some(b"java/lang/String") {
            return false;
        }
        let Some(Constant::NameAndType(name, descriptor)) = self.get(name_and_type) else {
            return false;
        };
        self.utf8(name) == Some(b"equals") && self.utf8(descriptor) == Some(b"(Ljava/lang/Object;)Z")
    }
}

/// A bounds-checked forward cursor over the class bytes.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u16(&mut self) -> Option<u16> {
        let value = u16_at(self.data, self.pos)? as u16;
        self.pos += 2;
        Some(value)
    }

    fn u32(&mut self) -> Option<u32> {
        let value = u32_at(self.data, self.pos)?;
        self.pos += 4;
        Some(value)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.pos = self.pos.checked_add(count)?;
        (self.pos <= self.data.len()).then_some(())
    }

    fn constant_pool(&mut self) -> Option<ConstantPool<'a>> {
        let count = self.u16()? as usize;
        let mut entries = Vec::with_capacity(count);
        entries.push(Constant::Other); // index 0 is never used
        let mut index = 1;
        while index < count {
            let tag = *self.data.get(self.pos)?;
            self.pos += 1;
            let entry = match tag {
                1 => {
                    let length = self.u16()? as usize;
                    let offset = self.pos;
                    self.skip(length)?;
                    Constant::Utf8(offset, length)
                }
                7 | 8 | 16 | 19 | 20 => {
                    let value = self.u16()?; // name/text index (only Class is kept)
                    if tag == 7 { Constant::Class(value) } else { Constant::Other }
                }
                15 => {
                    self.skip(3)?; // MethodHandle: reference kind + index
                    Constant::Other
                }
                12 => {
                    let name = self.u16()?;
                    let descriptor = self.u16()?;
                    Constant::NameAndType(name, descriptor)
                }
                9 | 10 | 11 | 17 | 18 => {
                    let class = self.u16()?;
                    let name_and_type = self.u16()?;
                    Constant::Reference(class, name_and_type)
                }
                3 | 4 => {
                    self.skip(4)?; // Integer, Float
                    Constant::Other
                }
                5 | 6 => {
                    self.skip(8)?; // Long, Double occupy this index and the next
                    entries.push(Constant::Other);
                    index += 1;
                    Constant::Other
                }
                _ => return None,
            };
            entries.push(entry);
            index += 1;
        }

        Some(ConstantPool { data: self.data, entries })
    }

    /// Skip a `fields` or `methods` table (both are member arrays with the same
    /// shape).
    fn skip_members(&mut self) -> Option<()> {
        let members = self.u16()?;
        for _ in 0..members {
            self.skip(6)?; // access flags, name, descriptor
            let attributes = self.u16()?;
            for _ in 0..attributes {
                self.skip(2)?; // attribute name
                let length = self.u32()? as usize;
                self.skip(length)?;
            }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    /// Assemble a class carrying the three gate strings and one method,
    /// `boolean check(MIDlet)`, whose body is `... invokevirtual
    /// String.equals; ireturn` - the shape the licence check has.
    fn synthetic_class(check_descriptor: &[u8]) -> (Vec<u8>, usize) {
        // Constant pool, built so the closing invokevirtual resolves to
        // java/lang/String.equals(Ljava/lang/Object;)Z.
        let utf8s: [&[u8]; 9] = [
            b"MIDlet-Key SERVICE_ID= MIN", // 1: forces the gate strings into the pool
            check_descriptor,              // 2: the method descriptor
            b"Code",                       // 3
            b"java/lang/String",           // 4
            b"equals",                     // 5
            b"(Ljava/lang/Object;)Z",      // 6
            b"check",                      // 7: method name
            b"C",                          // 8: this class name
            b"java/lang/Object",           // 9: super name
        ];
        let mut pool = Vec::new();
        for text in utf8s {
            pool.push(1u8);
            pool.extend_from_slice(&(text.len() as u16).to_be_bytes());
            pool.extend_from_slice(text);
        }
        // 10: Class "java/lang/String" (name -> 4)
        pool.extend_from_slice(&[7]);
        pool.extend_from_slice(&4u16.to_be_bytes());
        // 11: NameAndType equals:(Object)Z (name -> 5, desc -> 6)
        pool.extend_from_slice(&[12]);
        pool.extend_from_slice(&5u16.to_be_bytes());
        pool.extend_from_slice(&6u16.to_be_bytes());
        // 12: Methodref String.equals (class -> 10, nat -> 11)
        pool.extend_from_slice(&[10]);
        pool.extend_from_slice(&10u16.to_be_bytes());
        pool.extend_from_slice(&11u16.to_be_bytes());
        // 13: Class "C" (this)  14: Class "java/lang/Object" (super)
        pool.extend_from_slice(&[7]);
        pool.extend_from_slice(&8u16.to_be_bytes());
        pool.extend_from_slice(&[7]);
        pool.extend_from_slice(&9u16.to_be_bytes());
        let constant_count = 15u16; // entries 1..=14, count is +1

        let mut class = Vec::new();
        class.extend_from_slice(&0xCAFE_BABEu32.to_be_bytes());
        class.extend_from_slice(&0u16.to_be_bytes()); // minor
        class.extend_from_slice(&46u16.to_be_bytes()); // major
        class.extend_from_slice(&constant_count.to_be_bytes());
        class.extend_from_slice(&pool);
        class.extend_from_slice(&0x0021u16.to_be_bytes()); // access flags
        class.extend_from_slice(&13u16.to_be_bytes()); // this class -> 13
        class.extend_from_slice(&14u16.to_be_bytes()); // super class -> 14
        class.extend_from_slice(&0u16.to_be_bytes()); // interfaces
        class.extend_from_slice(&0u16.to_be_bytes()); // fields

        // One method: check(MIDlet)Z with a Code attribute.
        class.extend_from_slice(&1u16.to_be_bytes()); // methods count
        class.extend_from_slice(&0x0008u16.to_be_bytes()); // access flags (static)
        class.extend_from_slice(&7u16.to_be_bytes()); // name -> "check"
        class.extend_from_slice(&2u16.to_be_bytes()); // descriptor -> 2
        class.extend_from_slice(&1u16.to_be_bytes()); // attributes count
        class.extend_from_slice(&3u16.to_be_bytes()); // attribute name -> "Code"
        // Code body: max_stack(2) max_locals(2) code_length(4) code exception(2) attrs(2)
        let code: [u8; 4] = [0xb6, 0x00, 0x0c, 0xac]; // invokevirtual #12; ireturn
        let mut code_attr = Vec::new();
        code_attr.extend_from_slice(&2u16.to_be_bytes()); // max_stack
        code_attr.extend_from_slice(&1u16.to_be_bytes()); // max_locals
        code_attr.extend_from_slice(&(code.len() as u32).to_be_bytes());
        code_attr.extend_from_slice(&code);
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // exception table
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // attributes
        class.extend_from_slice(&(code_attr.len() as u32).to_be_bytes());
        let code_offset = class.len() + 8; // after max_stack/max_locals/code_length
        class.extend_from_slice(&code_attr);
        class.extend_from_slice(&0u16.to_be_bytes()); // class attributes

        (class, code_offset)
    }

    #[test]
    fn neutralizes_the_licence_check() {
        let (class, code_offset) = synthetic_class(CHECK_DESCRIPTOR);
        let patched = neutralize_license_check(&class);
        // The invokevirtual at the end of the code became pop2; iconst_1; nop.
        assert_eq!(&patched[code_offset..code_offset + 4], &[0x58, 0x04, 0x00, 0xac]);
    }

    #[test]
    fn leaves_a_normal_class_untouched() {
        // A class without the gate strings is a plain borrow, unchanged.
        let (mut class, _) = synthetic_class(CHECK_DESCRIPTOR);
        // Blank the gate strings' UTF-8 so the gate fails.
        for needle in [b"MIDlet-Key".as_slice(), b"SERVICE_ID=", b"MIN"] {
            while let Some(at) = class.windows(needle.len()).position(|w| w == needle) {
                class[at] = b'.';
            }
        }
        assert!(matches!(neutralize_license_check(&class), Cow::Borrowed(_)));
    }

    #[test]
    fn leaves_a_different_method_untouched() {
        // The gate strings are present but no method has the check descriptor,
        // so nothing is patched.
        let (class, _) = synthetic_class(b"(I)Z");
        assert!(matches!(neutralize_license_check(&class), Cow::Borrowed(_)));
    }
}
