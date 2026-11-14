use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::ClassAccessFlags;

use wie_jvm_support::WieJavaClassProto;

// interface javax.microedition.rms.RecordFilter
pub struct RecordFilter;

impl RecordFilter {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/rms/RecordFilter",
            parent_class: None,
            interfaces: vec![],
            methods: vec![JavaMethodProto::new_abstract("matches", "([B)Z", Default::default())],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}
