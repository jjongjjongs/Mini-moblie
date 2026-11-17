use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_midp::classes::javax::microedition::{
    lcdui::{Display, Font, Graphics, Image},
    midlet::MIDlet,
};

// class com.xce.lcdui.Toolkit
pub struct Toolkit;

impl Toolkit {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/xce/lcdui/Toolkit",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<clinit>", "()V", Self::cl_init, MethodAccessFlags::STATIC),
                // The input method's indicator icons - capitals, small letters,
                // digits, symbols, Hangul - and the hint drawn behind them.
                JavaMethodProto::new(
                    "ueimImg",
                    "()Ljavax/microedition/lcdui/Image;",
                    Self::indicator_image,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "leimImg",
                    "()Ljavax/microedition/lcdui/Image;",
                    Self::indicator_image,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "nimImg",
                    "()Ljavax/microedition/lcdui/Image;",
                    Self::indicator_image,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "simImg",
                    "()Ljavax/microedition/lcdui/Image;",
                    Self::indicator_image,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "kimImg",
                    "()Ljavax/microedition/lcdui/Image;",
                    Self::indicator_image,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "imHintImg",
                    "()Ljavax/microedition/lcdui/Image;",
                    Self::indicator_image,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![
                JavaFieldProto::new("graphics", "Ljavax/microedition/lcdui/Graphics;", FieldAccessFlags::STATIC),
                JavaFieldProto::new("DEFAULT_FONT", "Ljavax/microedition/lcdui/Font;", FieldAccessFlags::STATIC),
                JavaFieldProto::new("FONT_HEIGHT", "I", FieldAccessFlags::STATIC),
            ],
            access_flags: Default::default(),
        }
    }

    /// The handset's input method icons came out of its ROM, which this
    /// runtime does not have, so there is no picture to give. Null is what a
    /// title gets for one: 매지컬프린세스 draws them through a helper (`a.e`)
    /// that steps over a null image, and its name field works without them.
    async fn indicator_image(_jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<ClassInstanceRef<Image>> {
        Ok(None.into())
    }

    async fn cl_init(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.Toolkit::<clinit>");

        let font: ClassInstanceRef<Font> = jvm
            .invoke_static("javax/microedition/lcdui/Font", "getDefaultFont", "()Ljavax/microedition/lcdui/Font;", ())
            .await?;
        jvm.put_static_field("com/xce/lcdui/Toolkit", "DEFAULT_FONT", "Ljavax/microedition/lcdui/Font;", font.clone())
            .await?;

        let font_height: i32 = jvm.invoke_virtual(&font, "getHeight", "()I", ()).await?;
        jvm.put_static_field("com/xce/lcdui/Toolkit", "FONT_HEIGHT", "I", font_height).await?;

        let current_midlet: ClassInstanceRef<MIDlet> = jvm
            .get_static_field("javax/microedition/midlet/MIDlet", "currentMIDlet", "Ljavax/microedition/midlet/MIDlet;")
            .await?;

        let display: ClassInstanceRef<Display> = jvm
            .invoke_static(
                "javax/microedition/lcdui/Display",
                "getDisplay",
                "(Ljavax/microedition/midlet/MIDlet;)Ljavax/microedition/lcdui/Display;",
                (current_midlet,),
            )
            .await?;

        let graphics: ClassInstanceRef<Graphics> = Display::screen_graphics(jvm, &display).await?;

        jvm.put_static_field("com/xce/lcdui/Toolkit", "graphics", "Ljavax/microedition/lcdui/Graphics;", graphics)
            .await?;

        Ok(())
    }
}
