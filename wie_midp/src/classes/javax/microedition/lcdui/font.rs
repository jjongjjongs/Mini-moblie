use alloc::{string::String as RustString, vec};

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use java_runtime::classes::java::lang::String;
use jvm::{Array, ClassInstanceRef, JavaChar, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_backend::canvas;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class javax.microedition.lcdui.Font
pub struct Font;

impl Font {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/lcdui/Font",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<clinit>", "()V", Self::cl_init, MethodAccessFlags::STATIC),
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("getHeight", "()I", Self::get_height, Default::default()),
                JavaMethodProto::new("stringWidth", "(Ljava/lang/String;)I", Self::string_width, Default::default()),
                JavaMethodProto::new("substringWidth", "(Ljava/lang/String;II)I", Self::substring_width, Default::default()),
                JavaMethodProto::new("charWidth", "(C)I", Self::char_width, Default::default()),
                JavaMethodProto::new("charsWidth", "([CII)I", Self::chars_width, Default::default()),
                JavaMethodProto::new(
                    "getFont",
                    "(III)Ljavax/microedition/lcdui/Font;",
                    Self::get_font,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "getDefaultFont",
                    "()Ljavax/microedition/lcdui/Font;",
                    Self::get_default_font,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![
                JavaFieldProto::new("face", "I", Default::default()),
                JavaFieldProto::new("style", "I", Default::default()),
                JavaFieldProto::new("size", "I", Default::default()),
                JavaFieldProto::new("FACE_SYSTEM", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("FACE_MONOSPACE", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("FACE_PROPORTIONAL", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("STYLE_PLAIN", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("STYLE_BOLD", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("STYLE_ITALIC", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("STYLE_UNDERLINED", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("SIZE_SMALL", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("SIZE_MEDIUM", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("SIZE_LARGE", "I", FieldAccessFlags::STATIC),
            ],
            access_flags: Default::default(),
        }
    }

    /// How tall a line of this size is.
    ///
    /// The system font is made of pixel faces, and a face only draws cleanly at
    /// whole multiples of the height it was drawn at - 11, 14 and 16. So the
    /// sizes a title actually asks for are the heights a face can draw: small
    /// and medium land on 11 rather than 10 and 12, and large already sat on
    /// 14. The three larger flags keep the heights they had, 20 and 24 among
    /// them, which no face divides; they draw the way they always have.
    pub(crate) fn pixel_height(size: i32) -> i32 {
        match size {
            8 => 11,  // SIZE_SMALL
            0 => 11,  // SIZE_MEDIUM
            16 => 14, // SIZE_LARGE
            4096 => 16,
            8192 => 20,
            16384 => 22,
            32768 => 24,
            _ => 11,
        }
    }

    /// Where the baseline sits in that line, taken from the face that draws it
    /// so the metrics a title lays out with and the glyphs it gets agree.
    pub(crate) fn baseline(size: i32) -> i32 {
        wie_backend::canvas::baseline_px(Self::pixel_height(size) as f32) as i32
    }

    async fn cl_init(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        tracing::debug!("javax.microedition.lcdui.Font::<clinit>");

        jvm.put_static_field("javax/microedition/lcdui/Font", "FACE_SYSTEM", "I", 0).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "FACE_MONOSPACE", "I", 32).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "FACE_PROPORTIONAL", "I", 64)
            .await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "STYLE_PLAIN", "I", 0).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "STYLE_BOLD", "I", 1).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "STYLE_ITALIC", "I", 2).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "STYLE_UNDERLINED", "I", 4).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "SIZE_MEDIUM", "I", 0).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "SIZE_SMALL", "I", 8).await?;
        jvm.put_static_field("javax/microedition/lcdui/Font", "SIZE_LARGE", "I", 16).await?;

        Ok(())
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Font>) -> JvmResult<()> {
        tracing::debug!("javax.microedition.lcdui.Font::<init>({this:?})");

        jvm.put_field(&mut this, "face", "I", 0).await?;
        jvm.put_field(&mut this, "style", "I", 0).await?;
        jvm.put_field(&mut this, "size", "I", 0).await?;

        Ok(())
    }

    async fn get_height(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.lcdui.Font::getHeight({this:?})");

        let size: i32 = jvm.get_field(&this, "size", "I").await?;
        Ok(Self::pixel_height(size))
    }

    /// Not a stub: the no-argument constructor sets FACE_SYSTEM, STYLE_PLAIN
    /// and SIZE_MEDIUM, which is what MIDP's default font is. It logged at warn
    /// as though it were one, and titles that measure text call it per string -
    /// 액션퍼즐패밀리1 reaches it about 2,800 times a second, and in a capture
    /// taken to find something else that was a quarter of every line logged at
    /// warn or above.
    async fn get_default_font(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<ClassInstanceRef<Self>> {
        tracing::debug!("javax.microedition.lcdui.Font::getDefaultFont");

        let instance = jvm.new_class("javax/microedition/lcdui/Font", "()V", []).await?;

        Ok(instance.into())
    }

    async fn get_font(jvm: &Jvm, _: &mut WieJvmContext, face: i32, style: i32, size: i32) -> JvmResult<ClassInstanceRef<Font>> {
        tracing::debug!("javax.microedition.lcdui.Font::getFont({face:?}, {style:?}, {size:?})");

        let mut instance: ClassInstanceRef<Font> = jvm.new_class("javax/microedition/lcdui/Font", "()V", []).await?.into();

        jvm.put_field(&mut instance, "face", "I", face).await?;
        jvm.put_field(&mut instance, "style", "I", style).await?;
        jvm.put_field(&mut instance, "size", "I", size).await?;

        Ok(instance)
    }

    async fn string_width(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, string: ClassInstanceRef<String>) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.lcdui.Font::stringWidth({string:?})");

        let string = JavaLangString::to_rust_string(jvm, &string).await?;
        let size: i32 = jvm.get_field(&this, "size", "I").await?;

        Ok(canvas::string_width_px(&string, Self::pixel_height(size) as f32) as _)
    }

    async fn substring_width(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        string: ClassInstanceRef<String>,
        offset: i32,
        len: i32,
    ) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.lcdui.Font::substringWidth({string:?}, {offset:?}, {len:?})");

        let string = JavaLangString::to_rust_string(jvm, &string).await?;
        let substring = string.chars().skip(offset as usize).take(len as usize).collect::<RustString>();
        let size: i32 = jvm.get_field(&this, "size", "I").await?;

        Ok(canvas::string_width_px(&substring, Self::pixel_height(size) as f32) as _)
    }

    async fn char_width(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, char: JavaChar) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.lcdui.Font::charWidth({char:?})");

        let string = RustString::from_utf16(&[char]).unwrap();
        let size: i32 = jvm.get_field(&this, "size", "I").await?;

        Ok(canvas::string_width_px(&string, Self::pixel_height(size) as f32) as _)
    }

    async fn chars_width(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        chars: ClassInstanceRef<Array<JavaChar>>,
        offset: i32,
        len: i32,
    ) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.lcdui.Font::charsWidth({chars:?}, {offset:?}, {len:?})");

        let chars = jvm.load_array(&chars, offset as _, len as _).await?;
        let string = RustString::from_utf16(&chars).unwrap();
        let size: i32 = jvm.get_field(&this, "size", "I").await?;

        Ok(canvas::string_width_px(&string, Self::pixel_height(size) as f32) as _)
    }
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, vec};

    use jvm::{Array, ClassInstanceRef, JavaChar};
    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::{classes::javax::microedition::lcdui::Font, get_protos};

    /// A character measured on its own is the same width as the same character
    /// measured in a string, at whatever size the font was asked for.
    ///
    /// charWidth used to ignore the font it was called on and answer for a
    /// 14-pixel face every time, so a title laying its own text out a character
    /// at a time - 치킨타이쿤 measures every one it draws - was told the default
    /// font was wider than it is, and wrapped early.
    #[test]
    fn a_character_measures_the_same_alone_as_in_a_string() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            // SIZE_SMALL, SIZE_MEDIUM and SIZE_LARGE, and the sizes above them
            // that map to their own faces.
            for size in [8, 0, 16, 4096, 8192, 16384, 32768] {
                let font: ClassInstanceRef<Font> = jvm
                    .invoke_static(
                        "javax/microedition/lcdui/Font",
                        "getFont",
                        "(III)Ljavax/microedition/lcdui/Font;",
                        (0, 0, size),
                    )
                    .await?;

                for character in ['가', 'A', 'W', ' '] {
                    let mut chars: ClassInstanceRef<Array<JavaChar>> = jvm.instantiate_array("C", 1).await?.into();
                    jvm.store_array(&mut chars, 0, vec![character as JavaChar]).await?;

                    let alone: i32 = jvm.invoke_virtual(&font, "charWidth", "(C)I", (character as JavaChar,)).await?;
                    let in_a_string: i32 = jvm.invoke_virtual(&font, "charsWidth", "([CII)I", (chars, 0, 1)).await?;

                    assert_eq!(alone, in_a_string, "{character:?} at size {size}");
                }
            }

            Ok(())
        })
    }
}
