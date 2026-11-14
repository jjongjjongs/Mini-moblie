use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use jvm::{Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// The handset's external indicator lights.
///
/// A phone had lamps around its keypad that a title could set a mask of. There
/// are none to light here, so the count is zero and the mask is kept only so
/// that reading it back answers what was written - which is the whole of what a
/// title can observe of a lamp this host cannot show.
///
/// 치킨타이쿤 asks for the class in its `startApp`. Not having it at all made
/// the lookup a `NoClassDefFoundError`, which the title turned into a bare
/// `java.lang.Error` and died on before drawing anything - over lamps it would
/// not have missed.
// class org.kwis.msp.handset.LED
pub struct LED;

impl LED {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msp/handset/LED",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<clinit>", "()V", Self::cl_init, MethodAccessFlags::STATIC),
                JavaMethodProto::new("set", "(I)V", Self::set, MethodAccessFlags::STATIC),
                JavaMethodProto::new("get", "()I", Self::get, MethodAccessFlags::STATIC),
                JavaMethodProto::new("getCount", "()I", Self::get_count, MethodAccessFlags::STATIC),
            ],
            fields: vec![JavaFieldProto::new("leds", "I", FieldAccessFlags::STATIC)],
            access_flags: Default::default(),
        }
    }

    async fn cl_init(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        jvm.put_static_field("org/kwis/msp/handset/LED", "leds", "I", 0i32).await?;

        Ok(())
    }

    async fn set(jvm: &Jvm, _: &mut WieJvmContext, leds: i32) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.handset.LED::set({leds})");

        jvm.put_static_field("org/kwis/msp/handset/LED", "leds", "I", leds).await?;

        Ok(())
    }

    async fn get(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<i32> {
        let leds: i32 = jvm.get_static_field("org/kwis/msp/handset/LED", "leds", "I").await?;

        tracing::debug!("org.kwis.msp.handset.LED::get() -> {leds}");

        Ok(leds)
    }

    /// No lamps, so none to count.
    async fn get_count(_: &Jvm, _: &mut WieJvmContext) -> JvmResult<i32> {
        tracing::debug!("org.kwis.msp.handset.LED::getCount() -> 0");

        Ok(0)
    }
}

#[cfg(test)]
mod test {
    use alloc::boxed::Box;

    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::get_protos;

    /// A mask reads back as it was set, and there are no lamps to count.
    #[test]
    fn test_led_keeps_what_it_was_set_to() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let initial: i32 = jvm.invoke_static("org/kwis/msp/handset/LED", "get", "()I", ()).await?;
            assert_eq!(initial, 0, "nothing is lit before a title asks for it");

            let _: () = jvm.invoke_static("org/kwis/msp/handset/LED", "set", "(I)V", (0b1011i32,)).await?;

            let leds: i32 = jvm.invoke_static("org/kwis/msp/handset/LED", "get", "()I", ()).await?;
            assert_eq!(leds, 0b1011);

            let count: i32 = jvm.invoke_static("org/kwis/msp/handset/LED", "getCount", "()I", ()).await?;
            assert_eq!(count, 0, "this host has no lamps");

            Ok(())
        })
    }
}
