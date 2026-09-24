use alloc::{format, vec};

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{ClassAccessFlags, FieldAccessFlags};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::javax::microedition::lcdui::Display;

// abstract class javax.microedition.midlet.MIDlet
pub struct MIDlet;

impl MIDlet {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/midlet/MIDlet",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new(
                    "getAppProperty",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    Self::get_app_property,
                    Default::default(),
                ),
                JavaMethodProto::new_abstract("startApp", "([Ljava/lang/String;)V", Default::default()),
                JavaMethodProto::new("notifyDestroyed", "()V", Self::notify_destroyed, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("currentMIDlet", "Ljavax/microedition/midlet/MIDlet;", FieldAccessFlags::STATIC),
                JavaFieldProto::new("display", "Ljavax/microedition/lcdui/Display;", Default::default()),
            ],
            access_flags: ClassAccessFlags::ABSTRACT,
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("javax.microedition.midlet.MIDlet::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        jvm.put_static_field(
            "javax/microedition/midlet/MIDlet",
            "currentMIDlet",
            "Ljavax/microedition/midlet/MIDlet;",
            this.clone(),
        )
        .await?;

        // A title may reach its Display before its constructor has finished:
        // 엑스피드스노보드 touches com.xce.lcdui.Toolkit while this constructor
        // is still running, and Toolkit's own <clinit> calls Display.getDisplay
        // on this MIDlet. So creating the Display here is only the common path -
        // Self::display creates it too if it is asked for first - and both guard
        // on the field so exactly one Display is ever made.
        let existing: ClassInstanceRef<Display> = jvm.get_field(&this, "display", "Ljavax/microedition/lcdui/Display;").await?;
        if existing.is_null() {
            let display = jvm.new_class("javax/microedition/lcdui/Display", "()V", ()).await?;
            jvm.put_field(&mut this, "display", "Ljavax/microedition/lcdui/Display;", display).await?;
        }

        Ok(())
    }

    async fn get_app_property(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        key: ClassInstanceRef<String>,
    ) -> JvmResult<ClassInstanceRef<String>> {
        tracing::debug!("javax.microedition.midlet.MIDlet::getAppProperty({this:?}, {key:?})");

        let key = JavaLangString::to_rust_string(jvm, &key).await?;
        let system_key = format!("wie.appProperty.{key}");
        let system_key = JavaLangString::from_rust_string(jvm, &system_key).await?;

        jvm.invoke_static("java/lang/System", "getProperty", "(Ljava/lang/String;)Ljava/lang/String;", (system_key,))
            .await
    }

    /// The title saying it is finished and asking to be shut down.
    ///
    /// This is how a title quits of its own accord, and doing nothing about it
    /// left the app sitting on whatever frame was last painted, with the title's
    /// threads gone and only the event pump still turning. 아르덴전기 answers
    /// 아니오 to its 추가다운로드 offer by tearing its own threads down and
    /// calling this, so the offer stayed on screen for good - the same hang a
    /// person reports as the game having frozen, when in fact it had ended.
    async fn notify_destroyed(_jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("javax.microedition.midlet.MIDlet::notifyDestroyed({this:?})");

        context.system().platform().exit();

        Ok(())
    }

    pub async fn display(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<Display>> {
        let display: ClassInstanceRef<Display> = jvm.get_field(this, "display", "Ljavax/microedition/lcdui/Display;").await?;
        if !display.is_null() {
            return Ok(display);
        }

        // Asked for before the constructor stored one - create it now and keep
        // it, so the constructor finds it already there and does not make a
        // second. See MIDlet::init.
        let mut this = this.clone();
        let display = jvm.new_class("javax/microedition/lcdui/Display", "()V", ()).await?;
        jvm.put_field(&mut this, "display", "Ljavax/microedition/lcdui/Display;", display.clone())
            .await?;

        Ok(display.into())
    }
}
