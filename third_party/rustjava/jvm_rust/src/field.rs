use alloc::{
    string::{String, ToString},
    sync::Arc,
};

use classfile::FieldInfo;
use java_class_proto::JavaFieldProto;
use java_constants::FieldAccessFlags;
use jvm::Field;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct FieldInner {
    // The class that declares the field is part of what the field is: a class
    // may declare a field with the same name and type as one its superclass
    // declares, and an instance then holds both (JVMS 5.4.3.2). Without it the
    // two compare equal and share one slot in the instance's storage.
    class_name: String,
    name: String,
    descriptor: String,
    access_flags: FieldAccessFlags,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FieldImpl {
    inner: Arc<FieldInner>,
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
