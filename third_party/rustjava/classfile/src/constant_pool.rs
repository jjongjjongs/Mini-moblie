use alloc::{collections::BTreeMap, string::String, sync::Arc};

use nom::{
    IResult, Parser,
    bytes::complete::take,
    combinator::map_res,
    error::{Error, ErrorKind},
    number::complete::{be_f32, be_f64, be_i32, be_i64, be_u16, u8},
};

fn parse_utf8(data: &[u8]) -> IResult<&[u8], Arc<String>> {
    let (data, length) = be_u16(data)?;
    map_res(take(length as usize), |utf8: &[u8]| decode_modified_utf8(utf8).map(Arc::new).ok_or(())).parse(data)
}

/// Decode a `CONSTANT_Utf8` byte run.
///
/// These are Java's *modified* UTF-8, not standard UTF-8: the NUL character is
/// the two bytes `0xC0 0x80` so no `0x00` byte appears in the run, and a
/// character outside the Basic Multilingual Plane is written as its two UTF-16
/// surrogates, each in the three-byte form. `String::from_utf8` rejects both,
/// so a class carrying an embedded NUL in a string constant failed to load.
fn decode_modified_utf8(bytes: &[u8]) -> Option<String> {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        if b0 & 0x80 == 0 {
            // 1-byte: 0xxxxxxx (0x00 does not occur in the modified form).
            out.push(b0 as char);
            i += 1;
        } else if b0 & 0xE0 == 0xC0 {
            // 2-byte: 110xxxxx 10xxxxxx.
            let b1 = *bytes.get(i + 1)?;
            if b1 & 0xC0 != 0x80 {
                return None;
            }
            let code = (((b0 & 0x1F) as u32) << 6) | ((b1 & 0x3F) as u32);
            out.push(char::from_u32(code)?);
            i += 2;
        } else if b0 & 0xF0 == 0xE0 {
            // 3-byte: 1110xxxx 10xxxxxx 10xxxxxx.
            let b1 = *bytes.get(i + 1)?;
            let b2 = *bytes.get(i + 2)?;
            if b1 & 0xC0 != 0x80 || b2 & 0xC0 != 0x80 {
                return None;
            }
            let unit = (((b0 & 0x0F) as u32) << 12) | (((b1 & 0x3F) as u32) << 6) | ((b2 & 0x3F) as u32);
            if (0xD800..=0xDBFF).contains(&unit) {
                // High surrogate: a supplementary character is the pair, each
                // surrogate in its own three-byte form.
                let (b3, b4, b5) = (*bytes.get(i + 3)?, *bytes.get(i + 4)?, *bytes.get(i + 5)?);
                if b3 & 0xF0 != 0xE0 || b4 & 0xC0 != 0x80 || b5 & 0xC0 != 0x80 {
                    return None;
                }
                let low = (((b3 & 0x0F) as u32) << 12) | (((b4 & 0x3F) as u32) << 6) | ((b5 & 0x3F) as u32);
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return None;
                }
                let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                out.push(char::from_u32(code)?);
                i += 6;
            } else {
                out.push(char::from_u32(unit)?);
                i += 3;
            }
        } else {
            return None;
        }
    }
    Some(out)
}

#[derive(Debug)]
pub enum ConstantPoolItem {
    Utf8(Arc<String>),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class { name_index: u16 },
    String { string_index: u16 },
    Fieldref { class_index: u16, name_and_type_index: u16 },
    Methodref { class_index: u16, name_and_type_index: u16 },
    InterfaceMethodref { class_index: u16, name_and_type_index: u16 },
    NameAndType { name_index: u16, descriptor_index: u16 },
}

impl ConstantPoolItem {
    fn parse_tagged(data: &[u8], tag: u8) -> IResult<&[u8], Self> {
        match tag {
            1 => {
                let (data, utf8) = parse_utf8(data)?;
                Ok((data, Self::Utf8(utf8)))
            }
            3 => {
                let (data, value) = be_i32(data)?;
                Ok((data, Self::Integer(value)))
            }
            4 => {
                let (data, value) = be_f32(data)?;
                Ok((data, Self::Float(value)))
            }
            5 => {
                let (data, value) = be_i64(data)?;
                Ok((data, Self::Long(value)))
            }
            6 => {
                let (data, value) = be_f64(data)?;
                Ok((data, Self::Double(value)))
            }
            7 => {
                let (data, name_index) = be_u16(data)?;
                Ok((data, Self::Class { name_index }))
            }
            8 => {
                let (data, string_index) = be_u16(data)?;
                Ok((data, Self::String { string_index }))
            }
            9 => {
                let (data, class_index) = be_u16(data)?;
                let (data, name_and_type_index) = be_u16(data)?;
                Ok((
                    data,
                    Self::Fieldref {
                        class_index,
                        name_and_type_index,
                    },
                ))
            }
            10 => {
                let (data, class_index) = be_u16(data)?;
                let (data, name_and_type_index) = be_u16(data)?;
                Ok((
                    data,
                    Self::Methodref {
                        class_index,
                        name_and_type_index,
                    },
                ))
            }
            11 => {
                let (data, class_index) = be_u16(data)?;
                let (data, name_and_type_index) = be_u16(data)?;
                Ok((
                    data,
                    Self::InterfaceMethodref {
                        class_index,
                        name_and_type_index,
                    },
                ))
            }
            12 => {
                let (data, name_index) = be_u16(data)?;
                let (data, descriptor_index) = be_u16(data)?;
                Ok((
                    data,
                    Self::NameAndType {
                        name_index,
                        descriptor_index,
                    },
                ))
            }
            _ => Err(nom::Err::Error(Error::new(data, ErrorKind::Switch))),
        }
    }

    pub fn parse_all(data: &[u8]) -> IResult<&[u8], BTreeMap<u16, Self>> {
        let (remaining, count) = be_u16(data)?;
        if count == 0 {
            return Err(nom::Err::Error(Error::new(remaining, ErrorKind::Verify)));
        }
        if count == 1 {
            return Ok((remaining, BTreeMap::new()));
        }

        let mut data = remaining;
        let mut result = BTreeMap::new();
        let mut i = 1;
        loop {
            let (remaining, item) = Self::parse_with_tag(data)?;
            let is_double_entry = match &item {
                Self::Long(_) | Self::Double(_) => {
                    // long or double constant takes two constant pool entries....
                    true
                }
                _ => false,
            };
            result.insert(i, item);

            data = remaining;
            i += 1;
            if is_double_entry {
                i += 1;
            }

            if i > count {
                return Err(nom::Err::Error(Error::new(data, ErrorKind::Verify)));
            }
            if i == count {
                break;
            }
        }

        Ok((data, result))
    }

    pub fn parse_with_tag(data: &[u8]) -> IResult<&[u8], Self> {
        let (data, tag) = u8(data)?;
        Self::parse_tagged(data, tag)
    }

    pub fn utf8(&self) -> Option<Arc<String>> {
        if let ConstantPoolItem::Utf8(x) = self { Some(x.clone()) } else { None }
    }

    pub fn class_name_index(&self) -> Option<u16> {
        if let ConstantPoolItem::Class { name_index } = self {
            Some(*name_index)
        } else {
            None
        }
    }

    pub fn name_and_type(&self) -> Option<(u16, u16)> {
        if let ConstantPoolItem::NameAndType {
            name_index,
            descriptor_index,
        } = self
        {
            Some((*name_index, *descriptor_index))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub enum ConstantPoolReference {
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    String(Arc<String>),
    Class(Arc<String>),
    Method(FieldMethodref),
    InterfaceMethodref(FieldMethodref),
    Field(FieldMethodref),
}

impl ConstantPoolReference {
    pub fn from_constant_pool(constant_pool: &BTreeMap<u16, ConstantPoolItem>, index: u16) -> Option<Self> {
        match constant_pool.get(&index)? {
            ConstantPoolItem::Integer(x) => Some(Self::Integer(*x)),
            ConstantPoolItem::Float(x) => Some(Self::Float(*x)),
            ConstantPoolItem::Long(x) => Some(Self::Long(*x)),
            ConstantPoolItem::Double(x) => Some(Self::Double(*x)),
            ConstantPoolItem::String { string_index } => Some(Self::String(constant_pool.get(string_index)?.utf8()?)),
            ConstantPoolItem::Class { name_index } => Some(Self::Class(constant_pool.get(name_index)?.utf8()?)),
            ConstantPoolItem::Methodref {
                class_index,
                name_and_type_index,
            } => Some(Self::Method(FieldMethodref::from_reference_info(
                constant_pool,
                *class_index,
                *name_and_type_index,
            )?)),
            ConstantPoolItem::Fieldref {
                class_index,
                name_and_type_index,
            } => Some(Self::Field(FieldMethodref::from_reference_info(
                constant_pool,
                *class_index,
                *name_and_type_index,
            )?)),
            ConstantPoolItem::InterfaceMethodref {
                class_index,
                name_and_type_index,
            } => Some(Self::InterfaceMethodref(FieldMethodref::from_reference_info(
                constant_pool,
                *class_index,
                *name_and_type_index,
            )?)),
            _ => None,
        }
    }

    pub fn as_class(&self) -> &str {
        if let Self::Class(x) = self {
            x
        } else {
            panic!("Invalid constant pool item");
        }
    }

    pub fn as_field_ref(&self) -> &FieldMethodref {
        if let Self::Field(x) = self {
            x
        } else {
            panic!("Invalid constant pool item");
        }
    }

    pub fn as_method_ref(&self) -> &FieldMethodref {
        if let Self::Method(x) = self {
            x
        } else {
            panic!("Invalid constant pool item");
        }
    }

    pub fn as_interface_method_ref(&self) -> &FieldMethodref {
        if let Self::InterfaceMethodref(x) = self {
            x
        } else {
            panic!("Invalid constant pool item");
        }
    }
}

#[derive(Clone, Debug)]
pub struct FieldMethodref {
    pub class: Arc<String>,
    pub name: Arc<String>,
    pub descriptor: Arc<String>,
}

impl FieldMethodref {
    pub fn from_reference_info(constant_pool: &BTreeMap<u16, ConstantPoolItem>, class_index: u16, name_and_type_index: u16) -> Option<Self> {
        let class_name_index = constant_pool.get(&class_index)?.class_name_index()?;
        let class_name = constant_pool.get(&class_name_index)?.utf8()?;

        let (name_index, descriptor_index) = constant_pool.get(&name_and_type_index)?.name_and_type()?;
        let name = constant_pool.get(&name_index)?.utf8()?;
        let descriptor = constant_pool.get(&descriptor_index)?.utf8()?;

        Some(Self {
            class: class_name,
            name,
            descriptor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ConstantPoolItem;

    #[test]
    fn empty_constant_pool_is_valid() {
        let (remaining, constant_pool) = ConstantPoolItem::parse_all(&[0x00, 0x01, 0xff]).unwrap();

        assert!(constant_pool.is_empty());
        assert_eq!(remaining, &[0xff]);
    }

    #[test]
    fn long_must_fit_in_two_constant_pool_slots() {
        assert!(ConstantPoolItem::parse_all(&[0x00, 0x02, 0x05, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn utf8_decodes_the_modified_form() {
        use super::decode_modified_utf8;

        // A NUL is 0xC0 0x80, which standard UTF-8 rejects as overlong.
        assert!(alloc::string::String::from_utf8(alloc::vec![0xC0, 0x80]).is_err());
        assert_eq!(decode_modified_utf8(&[0xC0, 0x80]).as_deref(), Some("\0"));

        // ASCII, a NUL in the middle, then a three-byte BMP character (가).
        assert_eq!(
            decode_modified_utf8(&[b'A', 0xC0, 0x80, b'B', 0xEA, 0xB0, 0x80]).as_deref(),
            Some("A\0B가"),
        );

        // A supplementary character (U+1F600) as a surrogate pair, each
        // surrogate in the three-byte form: U+D83D then U+DE00.
        assert_eq!(decode_modified_utf8(&[0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]).as_deref(), Some("\u{1F600}"),);

        // A truncated multi-byte sequence is rejected rather than panicking.
        assert_eq!(decode_modified_utf8(&[0xC0]), None);
    }
}
