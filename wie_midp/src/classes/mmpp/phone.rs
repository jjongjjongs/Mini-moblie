use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.phone.Phone
//
// LG's handset properties for MIDP titles. 문명3 (COM2US) asks
// `getProperty("MIN")` while building its game screen and keeps the answer;
// with no class to answer it the title stopped on `NoClassDefFoundError`.
pub struct Phone;

impl Phone {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/phone/Phone",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new(
                "getProperty",
                "(Ljava/lang/String;)Ljava/lang/String;",
                Self::get_property,
                MethodAccessFlags::STATIC,
            )],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    /// The subscriber number for `MIN` and `PHONENUMBER`, the same fallback
    /// the other platforms report; `null` for a property the handset does not
    /// know.
    async fn get_property(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> JvmResult<ClassInstanceRef<String>> {
        let name = JavaLangString::to_rust_string(jvm, &name).await?;
        tracing::debug!("mmpp.phone.Phone::getProperty({name})");

        let value = match name.as_str() {
            "MIN" | "PHONENUMBER" => wie_backend::subscriber::FALLBACK,
            _ => {
                tracing::warn!("stub mmpp.phone.Phone::getProperty({name})");
                return Ok(None.into());
            }
        };

        Ok(JavaLangString::from_rust_string(jvm, value).await?.into())
    }
}
