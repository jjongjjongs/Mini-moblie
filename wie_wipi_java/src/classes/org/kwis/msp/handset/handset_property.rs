use alloc::{string::String as RustString, vec, vec::Vec};

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::lang::String;
use jvm::{
    ClassInstanceRef, Jvm, Result as JvmResult,
    runtime::{JavaIoInputStream, JavaLangClassLoader, JavaLangString},
};

use wie_backend::subscriber;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// What `getFlipState` reports on a handset with nothing to fold.
const FLIP_OPEN: i32 = 1;

// class org.kwis.msp.handset.HandsetProperty
pub struct HandsetProperty;

impl HandsetProperty {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msp/handset/HandsetProperty",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("getFlipState", "()I", Self::get_flip_state, MethodAccessFlags::STATIC),
                JavaMethodProto::new("bgmStop", "()I", Self::bgm_stop, MethodAccessFlags::STATIC),
                JavaMethodProto::new(
                    "getSystemProperty",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    Self::get_system_property,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "setSystemProperty",
                    "(Ljava/lang/String;Ljava/lang/String;)Z",
                    Self::set_system_property,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    /// A handset that does not fold is always open, which is what a title
    /// checks this for before deciding whether it may draw.
    async fn get_flip_state(_: &Jvm, _: &mut WieJvmContext) -> JvmResult<i32> {
        tracing::debug!("org.kwis.msp.handset.HandsetProperty::getFlipState()");

        Ok(FLIP_OPEN)
    }

    /// Silences whatever the handset itself was playing before the title
    /// started. Nothing plays behind a title here, so there is nothing to
    /// silence and the call succeeds.
    async fn bgm_stop(_: &Jvm, _: &mut WieJvmContext) -> JvmResult<i32> {
        tracing::debug!("org.kwis.msp.handset.HandsetProperty::bgmStop()");

        Ok(0)
    }

    async fn get_system_property(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> JvmResult<ClassInstanceRef<String>> {
        let name = JavaLangString::to_rust_string(jvm, &name).await?;

        // The subscriber number is recovered from the archive, the same way and
        // from the same files as the WIPI-C `MC_knlGetSystemProperty` path. A
        // title that reads it through both - one to decrypt its certificate,
        // the other to fill a form - would otherwise be told two different
        // numbers and reject itself.
        let recovered;
        let value = match name.as_ref() {
            "VIBRATORLEVEL" => "0",
            "DS_LOCK" => "0",
            "PHONENUMBER" | "MIN" => {
                recovered = Self::subscriber_number(jvm).await;
                recovered.as_str()
            }
            _ => {
                tracing::warn!("stub org.kwis.msp.handset.HandsetProperty::getSystemProperty({name})");
                ""
            }
        };

        tracing::debug!("org.kwis.msp.handset.HandsetProperty::getSystemProperty({name}) -> {value:?}");

        let result = JavaLangString::from_rust_string(jvm, value).await?;
        Ok(result.into())
    }

    /// The subscriber number the archive names, or the shared fallback.
    async fn subscriber_number(jvm: &Jvm) -> RustString {
        let cert = Self::resource(jvm, "cert.c2s").await;
        let certification = Self::resource(jvm, "certification").await;
        let app_info = Self::resource(jvm, "app_info").await;

        subscriber::subscriber_number(cert.as_deref(), certification.as_deref(), app_info.as_deref())
    }

    /// One of the archive's own files, or `None` when it has no such file.
    ///
    /// Read through the class loader, which is where these archives' files are:
    /// the WIPI-C side reaches them the same way.
    async fn resource(jvm: &Jvm, name: &str) -> Option<Vec<u8>> {
        let class_loader = jvm.current_class_loader().await.ok()?;
        let stream = JavaLangClassLoader::get_resource_as_stream(jvm, &class_loader, name).await.ok()??;

        JavaIoInputStream::read_until_end(jvm, &stream).await.ok()
    }

    async fn set_system_property(_: &Jvm, _: &mut WieJvmContext, id: ClassInstanceRef<String>, value: ClassInstanceRef<String>) -> JvmResult<bool> {
        tracing::warn!("stub org.kwis.msp.handset.HandsetProperty::setSystemProperty({id:?}, {value:?})");

        Ok(false)
    }
}

#[cfg(test)]
mod test {
    use alloc::boxed::Box;

    use java_runtime::classes::java::lang::String;
    use jvm::{ClassInstanceRef, runtime::JavaLangString};
    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::get_protos;

    #[test]
    fn test_set_system_property_returns_false() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let id: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "storage.test").await?.into();
            let value: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "value").await?.into();
            let result: bool = jvm
                .invoke_static(
                    "org/kwis/msp/handset/HandsetProperty",
                    "setSystemProperty",
                    "(Ljava/lang/String;Ljava/lang/String;)Z",
                    (id, value),
                )
                .await?;

            assert!(!result);
            Ok(())
        })
    }
}
