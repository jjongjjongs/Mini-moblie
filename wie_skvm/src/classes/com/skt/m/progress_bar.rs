use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class com.skt.m.ProgressBar
pub struct ProgressBar;

impl ProgressBar {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/skt/m/ProgressBar",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljava/lang/String;)V", Self::init, Default::default()),
                JavaMethodProto::new("getValue", "()I", Self::get_value, Default::default()),
                JavaMethodProto::new("setValue", "(I)V", Self::set_value, Default::default()),
                JavaMethodProto::new("getMaxValue", "()I", Self::get_max_value, Default::default()),
                JavaMethodProto::new("setMaxValue", "(I)V", Self::set_max_value, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("value", "I", Default::default()),
                JavaFieldProto::new("maxValue", "I", Default::default()),
                JavaFieldProto::new("title", "Ljava/lang/String;", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, title: ClassInstanceRef<String>) -> JvmResult<()> {
        tracing::debug!("com.skt.m.ProgressBar::<init>({this:?}, {title:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        jvm.put_field(&mut this, "title", "Ljava/lang/String;", title).await?;
        jvm.put_field(&mut this, "maxValue", "I", 100).await?;
        jvm.put_field(&mut this, "value", "I", 0).await?;

        Ok(())
    }

    async fn get_value(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("com.skt.m.ProgressBar::getValue({this:?})");

        jvm.get_field(&this, "value", "I").await
    }

    async fn set_value(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, value: i32) -> JvmResult<()> {
        tracing::debug!("com.skt.m.ProgressBar::setValue({this:?}, {value})");

        let max_value: i32 = jvm.get_field(&this, "maxValue", "I").await?;
        jvm.put_field(&mut this, "value", "I", value.clamp(0, max_value)).await?;

        Ok(())
    }

    async fn get_max_value(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("com.skt.m.ProgressBar::getMaxValue({this:?})");

        jvm.get_field(&this, "maxValue", "I").await
    }

    async fn set_max_value(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, value: i32) -> JvmResult<()> {
        tracing::debug!("com.skt.m.ProgressBar::setMaxValue({this:?}, {value})");

        if value <= 0 {
            return Err(jvm
                .exception("java/lang/IllegalArgumentException", &alloc::format!("max value {value}"))
                .await);
        }

        jvm.put_field(&mut this, "maxValue", "I", value).await?;
        let current: i32 = jvm.get_field(&this, "value", "I").await?;
        jvm.put_field(&mut this, "value", "I", current.min(value)).await?;

        Ok(())
    }
}
