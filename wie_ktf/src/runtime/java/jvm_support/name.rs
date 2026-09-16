use alloc::{string::String, sync::Arc, vec::Vec};
use core::fmt::Display;

use wie_core_arm::ArmCore;
use wie_util::{read_generic, read_null_terminated_string_bytes};

use super::Result;

#[derive(Clone)]
pub struct JavaFullName {
    pub tag: u8,
    pub name: String,
    pub descriptor: String,
}

impl JavaFullName {
    /// The name a field or method record points at.
    ///
    /// A record's name is written when its class is registered and never
    /// rewritten, so it is read from guest memory once per address and answered
    /// from the core's cache after that - see [`ArmCore::write_once_metadata`].
    /// Spelling it out again on every lookup is what made a field access cost
    /// tens of guest reads, and a title that reaches for a field per pixel pay
    /// for its own names instead of its drawing.
    pub fn from_ptr(core: &ArmCore, ptr: u32) -> Result<Arc<Self>> {
        core.write_once_metadata(ptr, || {
            let tag = read_generic(core, ptr)?;

            let value = read_null_terminated_string_bytes(core, ptr + 1)?;
            let value = String::from_utf8(value).unwrap();
            let mut values = value.split('+');

            let descriptor = values.next().unwrap().into();
            let name = values.next().unwrap().into();

            Ok(JavaFullName { tag, name, descriptor })
        })
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.push(self.tag);
        bytes.extend_from_slice(self.descriptor.as_bytes());
        bytes.push(b'+');
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.push(0);

        bytes
    }
}

impl Display for JavaFullName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.name.fmt(f)?;
        self.descriptor.fmt(f)?;
        write!(f, "@{}", self.tag)?;

        Ok(())
    }
}

impl PartialEq for JavaFullName {
    fn eq(&self, other: &Self) -> bool {
        self.descriptor == other.descriptor && self.name == other.name
    }
}
