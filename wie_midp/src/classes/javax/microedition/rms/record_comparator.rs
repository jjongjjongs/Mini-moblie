use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::ClassAccessFlags;

use wie_jvm_support::WieJavaClassProto;

// interface javax.microedition.rms.RecordComparator
pub struct RecordComparator;

impl RecordComparator {
    /// What `compare` answers when its first record belongs before its second.
    pub const PRECEDES: i32 = -1;

    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/rms/RecordComparator",
            parent_class: None,
            interfaces: vec![],
            methods: vec![JavaMethodProto::new_abstract("compare", "([B[B)I", Default::default())],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}
