use alloc::{
    string::{String, ToString},
    sync::Arc,
};

use classfile::FieldInfo;
use java_class_proto::JavaFieldProto;
use java_constants::FieldAccessFlags;
use jvm::Field;

#[derive(Debug)]
struct FieldInner {
    // The class that declares the field, for what the field prints as. A class
    // may declare a field with the same name and type as one its superclass
    // declares, and an instance then holds both (JVMS 5.4.3.2); they are kept
    // apart by being two fields - see `FieldImpl`'s equality - not by this.
    #[allow(dead_code)]
    class_name: String,
    name: String,
    descriptor: String,
    access_flags: FieldAccessFlags,
}

/// A field is the one its class definition made: every `FieldImpl` an
/// instance or a class stores under is a clone of the one in the definition's
/// field list, so two are the same field exactly when they share it. Equality
/// and order go by that rather than by comparing the class name, name and
/// descriptor, which every field read and write paid for each node of the
/// storage map it walked.
#[derive(Clone, Debug)]
pub struct FieldImpl {
    inner: Arc<FieldInner>,
}

impl PartialEq for FieldImpl {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for FieldImpl {}

impl PartialOrd for FieldImpl {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FieldImpl {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        Arc::as_ptr(&self.inner).cmp(&Arc::as_ptr(&other.inner))
    }
}

impl FieldImpl {
    pub fn new(class_name: &str, name: &str, descriptor: &str, access_flags: FieldAccessFlags) -> Self {
        Self {
            inner: Arc::new(FieldInner {
                class_name: class_name.to_string(),
                name: name.to_string(),
                descriptor: descriptor.to_string(),
                access_flags,
            }),
        }
    }

    pub fn from_field_proto(class_name: &str, proto: JavaFieldProto) -> Self {
        Self::new(class_name, &proto.name, &proto.descriptor, proto.access_flags)
    }

    pub fn from_field_info(class_name: &str, field_info: FieldInfo) -> Self {
        Self {
            inner: Arc::new(FieldInner {
                class_name: class_name.to_string(),
                name: field_info.name.to_string(),
                descriptor: field_info.descriptor.to_string(),
                access_flags: field_info.access_flags,
            }),
        }
    }
}

impl FieldImpl {
    /// Whether this is the field named; see `MethodImpl::is`.
    pub(crate) fn is(&self, name: &str, descriptor: &str) -> bool {
        self.inner.name == name && self.inner.descriptor == descriptor
    }
}

impl Field for FieldImpl {
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    fn access_flags(&self) -> FieldAccessFlags {
        self.inner.access_flags
    }
}
