use alloc::{string::String, sync::Arc};
use core::{
    fmt::{self, Debug, Formatter},
    mem::size_of,
};

use java_class_proto::JavaFieldProto;
use java_constants::FieldAccessFlags;
use jvm::{Field, JavaType};
use wipi_types::ktf::java::JavaFieldDefinition as RawJavaField;

use wie_core_arm::{Allocator, ArmCore};
use wie_util::{ByteWrite, read_generic, write_generic};

use super::{Result, name::JavaFullName};

/// What a field's own record says, read once when its class was indexed.
///
/// A field is described in guest memory: its name and descriptor are a string
/// there, its offset and flags a record. Answering a read out of those meant
/// three guest reads, a cache lookup and a descriptor parse every time - which
/// a title that reaches for a field per pixel pays per pixel. The record is
/// written when the class is registered and never rewritten, so it is read once
/// and carried on the handle a lookup hands back.
pub struct ResolvedField {
    pub ptr_raw: u32,
    /// Where the value sits in an instance. `None` for a static field, whose
    /// record keeps the value itself in that word rather than an offset - so
    /// there is nothing there to remember.
    pub offset: Option<u32>,
    pub access_flags: FieldAccessFlags,
    pub name: Arc<JavaFullName>,
    pub value_type: JavaType,
}

pub struct JavaField {
    pub ptr_raw: u32,
    core: ArmCore,
    /// What the class index already knows about this field, when it came from
    /// there rather than from a bare pointer.
    resolved: Option<Arc<ResolvedField>>,
}

impl JavaField {
    pub fn from_raw(ptr_raw: u32, core: &ArmCore) -> Self {
        Self {
            ptr_raw,
            core: core.clone(),
            resolved: None,
        }
    }

    /// The handle a class index hands back, carrying what it read when it was
    /// built.
    pub fn resolved(core: &ArmCore, resolved: Arc<ResolvedField>) -> Self {
        Self {
            ptr_raw: resolved.ptr_raw,
            core: core.clone(),
            resolved: Some(resolved),
        }
    }

    /// Everything the class index read about this field, when it has it.
    pub fn resolved_parts(&self) -> Option<&ResolvedField> {
        self.resolved.as_deref()
    }

    pub fn new(core: &mut ArmCore, ptr_class: u32, proto: JavaFieldProto, offset_or_value: u32) -> Result<Self> {
        let full_name = JavaFullName {
            tag: 0,
            name: proto.name,
            descriptor: proto.descriptor,
        };
        let full_name_bytes = full_name.as_bytes();
        let ptr_name = Allocator::alloc(core, full_name_bytes.len() as u32)?;
        core.write_bytes(ptr_name, &full_name_bytes)?;

        let ptr_raw = Allocator::alloc(core, size_of::<RawJavaField>() as u32)?;

        write_generic(
            core,
            ptr_raw,
            RawJavaField {
                access_flags: proto.access_flags.bits() as _,
                ptr_class,
                ptr_name,
                offset_or_value,
            },
        )?;

        tracing::trace!("Wrote field {} at {ptr_raw:#x}", full_name.name);

        Ok(Self::from_raw(ptr_raw, core))
    }

    pub fn name(&self) -> Result<Arc<JavaFullName>> {
        if let Some(resolved) = self.resolved_parts() {
            return Ok(resolved.name.clone());
        }

        let raw: RawJavaField = read_generic(&self.core, self.ptr_raw)?;

        JavaFullName::from_ptr(&self.core, raw.ptr_name)
    }

    pub fn offset(&self) -> Result<u32> {
        if let Some(offset) = self.resolved_parts().and_then(|resolved| resolved.offset) {
            return Ok(offset);
        }

        let raw: RawJavaField = read_generic(&self.core, self.ptr_raw)?;

        Ok(raw.offset_or_value)
    }

    pub fn static_address(&self) -> Result<u32> {
        let address = self.ptr_raw + 12; // offsetof offset_or_value

        Ok(address)
    }
}

impl Field for JavaField {
    fn name(&self) -> String {
        let name = self.name().unwrap();

        name.name.clone()
    }

    fn descriptor(&self) -> String {
        let name = self.name().unwrap();

        name.descriptor.clone()
    }

    fn access_flags(&self) -> FieldAccessFlags {
        if let Some(resolved) = self.resolved_parts() {
            return resolved.access_flags;
        }

        let raw: RawJavaField = read_generic(&self.core, self.ptr_raw).unwrap();

        FieldAccessFlags::from_bits_truncate(raw.access_flags as _)
    }
}

impl Debug for JavaField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JavaMethod").field("ptr_raw", &self.ptr_raw).finish()
    }
}
