use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// The one class outside `org.kwis.*` a KTF handset provides that a title links
/// against: the handset's own device-management record. The archive ships no
/// class file for it, so a title that names it finds nothing unless the
/// platform publishes it.
///
/// It is KTF's, so it is published here rather than in the shared WIPI-Java
/// classes an LGT or SKT title also loads.
///
/// Only two members are known to be asked of it - the static that answers the
/// single instance, and the instance's own subscriber number. Anything else is
/// left absent deliberately: a title that asks for more fails by name, which is
/// the evidence needed to serve it.
pub struct DMInfo;

impl DMInfo {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "wec/DMInfo",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("getDMInfo", "()Lwec/DMInfo;", Self::get_dm_info, MethodAccessFlags::STATIC),
                JavaMethodProto::new("gethandsetMIN", "()Ljava/lang/String;", Self::get_handset_min, Default::default()),
            ],
            fields: vec![JavaFieldProto::new("__wieInstance", "Lwec/DMInfo;", FieldAccessFlags::STATIC)],
            access_flags: Default::default(),
        }
    }

    async fn init(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("wec.DMInfo::<init>()");

        Ok(())
    }

    /// The handset's record, which is one object for the life of the handset: a
    /// title that asks twice compares what it got with what it got last time, so
    /// the first answer is kept and handed back.
    async fn get_dm_info(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<ClassInstanceRef<Self>> {
        tracing::debug!("wec.DMInfo::getDMInfo()");

        let existing: ClassInstanceRef<Self> = jvm.get_static_field("wec/DMInfo", "__wieInstance", "Lwec/DMInfo;").await?;
        if !existing.is_null() {
            return Ok(existing);
        }

        let instance = jvm.new_class("wec/DMInfo", "()V", ()).await?;
        jvm.put_static_field("wec/DMInfo", "__wieInstance", "Lwec/DMInfo;", instance.clone())
            .await?;

        Ok(instance.into())
    }

    /// The subscriber number, which is what a MIN is. It is read back through
    /// `HandsetProperty` rather than recovered again here, so this and the
    /// `MC_knlGetSystemProperty` path cannot tell a title two different numbers
    /// for the handset it is running on.
    async fn get_handset_min(jvm: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<String>> {
        let name = JavaLangString::from_rust_string(jvm, "MIN").await?;
        let min: ClassInstanceRef<String> = jvm
            .invoke_static(
                "org/kwis/msp/handset/HandsetProperty",
                "getSystemProperty",
                "(Ljava/lang/String;)Ljava/lang/String;",
                (name,),
            )
            .await?;

        tracing::debug!("wec.DMInfo::gethandsetMIN() -> {:?}", JavaLangString::to_rust_string(jvm, &min).await?);

        Ok(min)
    }
}
