use alloc::{boxed::Box, vec, vec::Vec};
use core::{
    fmt::{self, Debug, Formatter},
    hash::{Hash, Hasher},
    iter,
    mem::size_of,
};
use java_constants::FieldAccessFlags;

use jvm::{ClassDefinition, ClassInstance, Field, JavaType, JavaValue, Result as JvmResult};
use wipi_types::ktf::java::JavaClassInstance as RawJavaClassInstance;

use wie_core_arm::{Allocator, ArmCore};
use wie_util::{ByteRead, ByteWrite, read_generic, write_generic};

use crate::runtime::java::jvm_support::KtfJvmSupport;

use super::{KtfJvmWord, Result, class_definition::JavaClassDefinition, field::JavaField, value::JavaValueExt};

#[derive(Clone)]
pub struct JavaClassInstance {
    pub ptr_raw: u32,
    core: ArmCore,
}

impl JavaClassInstance {
    pub fn from_raw(ptr_raw: u32, core: &ArmCore) -> Self {
        Self { ptr_raw, core: core.clone() }
    }

    pub fn new(core: &mut ArmCore, class: &JavaClassDefinition) -> Result<Self> {
        let field_size = class.field_size()?;

        let instance = Self::instantiate(core, class, field_size)?;

        tracing::trace!("Instantiated {} at {:#x}", class.name()?, instance.ptr_raw);

        Ok(instance)
    }

    /// The class this object is one of.
    ///
    /// An object this runtime made keeps it in the word beside its fields. One
    /// that a title's own compiled image carries - a string constant, the char
    /// array behind it - has no such word: it is one word holding the address
    /// of the fields that follow it, and the class is what the first of those
    /// fields says, the index of its vtable. So the word beside the fields is
    /// taken only where it really is a class record, which is a record whose
    /// first word is its own address plus four - the same mark
    /// `get_java_method` tests - and the index answers for the rest.
    pub fn class(&self) -> Result<JavaClassDefinition> {
        let raw = self.read_raw()?;

        let mark: Result<u32> = read_generic(&self.core, raw.ptr_class);
        let is_record = mark.is_ok_and(|mark| mark == raw.ptr_class + 4);
        if is_record {
            return Ok(JavaClassDefinition::from_raw(raw.ptr_class, &self.core));
        }

        let vtable_word: u32 = read_generic(&self.core, raw.ptr_fields)?;
        let ptr_class = KtfJvmSupport::class_by_vtable_word(&mut self.core.clone(), vtable_word)?;
        if ptr_class == 0 {
            tracing::warn!("no class for {:#x} by vtable word {vtable_word:#x}", self.ptr_raw);
        }

        Ok(JavaClassDefinition::from_raw(ptr_class, &self.core))
    }

    pub(super) fn field_address(&self, offset: u32) -> Result<u32> {
        let raw = self.read_raw()?;

        Ok(raw.ptr_fields + offset + 4)
    }

    pub(super) fn instantiate(core: &mut ArmCore, class: &JavaClassDefinition, field_size: usize) -> Result<Self> {
        let ptr_raw = Allocator::alloc(core, size_of::<RawJavaClassInstance>() as _)?;
        let ptr_fields = Allocator::alloc(core, (field_size + 4) as _)?;

        let zero = iter::repeat_n(0, (field_size + 4) as _).collect::<Vec<_>>();
        core.write_bytes(ptr_fields, &zero)?;

        let vtable_index = KtfJvmSupport::get_vtable_index(core, class)?;

        write_generic(
            core,
            ptr_raw,
            RawJavaClassInstance {
                ptr_fields,
                ptr_class: class.ptr_raw,
            },
        )?;
        write_generic(core, ptr_fields, (vtable_index * 4) << 5)?;

        tracing::trace!("Instantiate {}, vtable_index {vtable_index:#x} at {ptr_raw:#x}", class.name()?);

        Ok(Self::from_raw(ptr_raw, core))
    }

    fn read_raw(&self) -> Result<RawJavaClassInstance> {
        let instance: RawJavaClassInstance = read_generic(&self.core, self.ptr_raw as _)?;

        Ok(instance)
    }
}

#[async_trait::async_trait]
impl ClassInstance for JavaClassInstance {
    /// Deliberately frees nothing.
    ///
    /// The JVM calls this when its own collector decides an object is garbage,
    /// and for KTF that decision is not to be trusted: the object is a block of
    /// guest memory the ARM code may still be holding in a register, on its
    /// stack or in another object's field, none of which is a JVM root. So the
    /// JVM is allowed to forget the object, which costs nothing, but the guest
    /// memory is not freed.
    ///
    /// Freeing it on the JVM's word corrupts live state, measurably: 투스워즈
    /// loses the byte array behind a resource it is decoding and dies a few
    /// frames later reading a length that has become another block's
    /// bookkeeping.
    ///
    /// Nothing reclaims them instead, so a KTF title's heap only grows. That is
    /// also what the reference emulator does, which is worth saying because it
    /// makes this a design rather than a debt: its KTF runtime builds its Java
    /// objects in guest memory the same way and has no collector for them at
    /// all - the only `collectGarbage` in the whole binary belongs to its
    /// SK-VM, and there is no free, destroy or reclaim of a KTF Java object
    /// anywhere in it. Its one root-visitor for KTF covers strings, for state
    /// snapshots.
    fn destroy(self: Box<Self>) {}

    fn identity(&self) -> usize {
        self.ptr_raw as _
    }

    fn shallow_clone(&self) -> JvmResult<Box<dyn ClassInstance>> {
        let mut core = self.core.clone();
        let class = self.class().unwrap();
        let field_size = class.field_size().unwrap();

        let instance = Self::instantiate(&mut core, &class, field_size).unwrap();

        let mut fields = vec![0; field_size];
        core.read_bytes(self.field_address(0).unwrap(), &mut fields).unwrap();
        core.write_bytes(instance.field_address(0).unwrap(), &fields).unwrap();

        Ok(Box::new(instance))
    }

    fn class_definition(&self) -> Box<dyn ClassDefinition> {
        Box::new(self.class().unwrap())
    }

    fn equals(&self, other: &dyn ClassInstance) -> JvmResult<bool> {
        let other = other.as_any().downcast_ref::<JavaClassInstance>();
        if other.is_none() {
            return Ok(false);
        }

        Ok(self.ptr_raw == other.unwrap().ptr_raw)
    }

    fn get_field(&self, field: &dyn Field) -> JvmResult<JavaValue> {
        let field = field.as_any().downcast_ref::<JavaField>().unwrap();
        // The type its class read when it was indexed, or - for a handle made
        // from a bare pointer - the descriptor parsed here. Parsing it per
        // access allocated a `String` for every reference field, which a title
        // that reads a field per pixel pays per pixel.
        let parsed;
        let field_type = match field.resolved_parts() {
            Some(resolved) => &resolved.value_type,
            None => {
                parsed = JavaType::parse(&field.name().unwrap().descriptor);
                &parsed
            }
        };

        assert!(!field.access_flags().contains(FieldAccessFlags::STATIC));

        let offset = field.offset().unwrap();
        let address = self.field_address(offset).unwrap();

        if matches!(field_type, JavaType::Long | JavaType::Double) {
            let value: KtfJvmWord = read_generic(&self.core, address).unwrap();
            let value_high: KtfJvmWord = read_generic(&self.core, address + 4).unwrap();

            Ok(JavaValue::from_raw64(value, value_high, field_type))
        } else {
            let value: KtfJvmWord = read_generic(&self.core, address).unwrap();

            Ok(JavaValue::from_raw(value, field_type, &self.core))
        }
    }

    fn put_field(&mut self, field: &dyn Field, value: JavaValue) -> JvmResult<()> {
        let field = field.as_any().downcast_ref::<JavaField>().unwrap();
        let parsed;
        let field_type = match field.resolved_parts() {
            Some(resolved) => &resolved.value_type,
            None => {
                parsed = JavaType::parse(&field.name().unwrap().descriptor);
                &parsed
            }
        };

        assert!(!field.access_flags().contains(FieldAccessFlags::STATIC));

        let offset = field.offset().unwrap();
        let address = self.field_address(offset).unwrap();

        if matches!(field_type, JavaType::Long | JavaType::Double) {
            let (value, value_high) = value.as_raw64();

            write_generic(&mut self.core, address, value).unwrap();
            write_generic(&mut self.core, address + 4, value_high).unwrap();
        } else {
            write_generic(&mut self.core, address, value.as_raw()).unwrap();
        }

        Ok(())
    }
}

impl Debug for JavaClassInstance {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.ptr_raw)
    }
}

impl Hash for JavaClassInstance {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ptr_raw.hash(state)
    }
}
