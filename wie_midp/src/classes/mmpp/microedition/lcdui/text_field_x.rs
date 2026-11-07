use alloc::{
    string::{String as RustString, ToString},
    vec,
};

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_backend::{Event, InputMethodOutput};
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::{
    javax::microedition::lcdui::{Canvas, Graphics},
    net::wie::{MIDPKeyCode, STD_KEY_CLEAR, STD_KEY_DOWN, STD_KEY_FIRE, STD_KEY_LEFT, STD_KEY_RIGHT, STD_KEY_UP},
};

/// Tells the input method to finish the syllable it is holding and hand it
/// over, adding nothing new.
const IME_FLUSH: i8 = -99;

/// Tells the input method to take one step back inside the syllable it is
/// composing, which is what CLEAR means while a syllable is still open.
const IME_BACKSPACE: i8 = -16;

/// A key press, as the input method numbers its events.
const IME_PRESS: u32 = 2;

/// The input method's own modes.
const IME_LOWER: u32 = 0;
const IME_UPPER: u32 = 1;
const IME_NUMBER: u32 = 2;
const IME_KOREAN: u32 = 3;

/// The modes as `getInputMode` reports them - the numbers 다운타운
/// 미니게임천국2 switches on to label its field: 32 is 한글, 1 대문자, 2
/// 소문자, 4 숫자 (and 8 기호, which this field does not offer).
const MODE_KOREAN: i32 = 32;
const MODE_UPPER: i32 = 1;
const MODE_LOWER: i32 = 2;
const MODE_NUMBER: i32 = 4;

/// MIDP's `TextField.NUMERIC`, the constraint that keeps a field to digits.
const NUMERIC: i32 = 2;

/// Inset of the text from the field's border, in pixels.
const TEXT_INSET: i32 = 2;

/// What a key means to the field.
enum Key {
    Clear,
    /// Ends the syllable in progress without typing anything.
    Flush,
    /// A key the input method types with.
    Type,
    /// Not the field's - soft keys, CALL and the like.
    Other,
}

// class mmpp.microedition.lcdui.TextFieldX
//
// SKT's on-screen text field for a Canvas. A title builds one, gives it the
// Canvas it lives on, forwards that Canvas's key events to it and paints it
// from its own `paint`, translated to where it wants it; the field composes
// the presses into text. 다운타운 미니게임천국2 asks for the ranking name
// this way, and without the class the name screen died on a
// `NoClassDefFoundError` and sat there frozen.
//
// The composition is the platform input method's, held the way
// `com.xce.lcdui.XTextField` holds it: what is finished goes into the text,
// and the syllable still open sits at its end and is replaced on each key.
pub struct TextFieldX;

impl TextFieldX {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/microedition/lcdui/TextFieldX",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljava/lang/String;Ljava/lang/String;II)V", Self::init, Default::default()),
                JavaMethodProto::new("setOwner", "(Ljavax/microedition/lcdui/Canvas;)V", Self::set_owner, Default::default()),
                JavaMethodProto::new("setWidth", "(I)V", Self::set_width, Default::default()),
                JavaMethodProto::new("setMaxRow", "(I)V", Self::set_max_row, Default::default()),
                JavaMethodProto::new("setFocus", "(Z)V", Self::set_focus, Default::default()),
                JavaMethodProto::new("getInputMode", "()I", Self::get_input_mode, Default::default()),
                JavaMethodProto::new("nextInputMode", "()I", Self::next_input_mode, Default::default()),
                JavaMethodProto::new("keyPressed", "(I)V", Self::key_pressed, Default::default()),
                JavaMethodProto::new("keyRepeated", "(I)V", Self::key_repeated, Default::default()),
                JavaMethodProto::new("keyReleased", "(I)V", Self::key_released, Default::default()),
                JavaMethodProto::new("paint", "(Ljavax/microedition/lcdui/Graphics;)V", Self::paint, Default::default()),
                JavaMethodProto::new("getString", "()Ljava/lang/String;", Self::get_string, Default::default()),
                JavaMethodProto::new("setString", "(Ljava/lang/String;)V", Self::set_string, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("__wieText", "Ljava/lang/String;", Default::default()),
                // How many chars at the end of the text are the syllable still
                // being composed, and so are replaced rather than added to.
                JavaFieldProto::new("__wieComposition", "I", Default::default()),
                JavaFieldProto::new("__wieMaxSize", "I", Default::default()),
                JavaFieldProto::new("__wieConstraints", "I", Default::default()),
                JavaFieldProto::new("__wieFocused", "Z", Default::default()),
                JavaFieldProto::new("__wieWidth", "I", Default::default()),
                JavaFieldProto::new("__wieOwner", "Ljavax/microedition/lcdui/Canvas;", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        label: ClassInstanceRef<String>,
        text: ClassInstanceRef<String>,
        max_size: i32,
        constraints: i32,
    ) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::<init>({this:?}, {label:?}, {text:?}, {max_size}, {constraints})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        let text = if text.is_null() {
            JavaLangString::from_rust_string(jvm, "").await?.into()
        } else {
            text
        };

        jvm.put_field(&mut this, "__wieText", "Ljava/lang/String;", text).await?;
        jvm.put_field(&mut this, "__wieComposition", "I", 0).await?;
        jvm.put_field(&mut this, "__wieMaxSize", "I", max_size).await?;
        jvm.put_field(&mut this, "__wieConstraints", "I", constraints).await?;
        jvm.put_field(&mut this, "__wieFocused", "Z", true).await?;

        // A Korean handset opens a text field in Hangul, and a numeric one in
        // digits.
        let mode = if constraints & 0xffff == NUMERIC { IME_NUMBER } else { IME_KOREAN };
        context.system().set_current_input_mode(mode);

        Ok(())
    }

    async fn set_owner(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, owner: ClassInstanceRef<Canvas>) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::setOwner({this:?}, {owner:?})");

        jvm.put_field(&mut this, "__wieOwner", "Ljavax/microedition/lcdui/Canvas;", owner).await
    }

    async fn set_width(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, width: i32) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::setWidth({this:?}, {width})");

        jvm.put_field(&mut this, "__wieWidth", "I", width).await
    }

    /// The field is drawn one row high whatever is asked, which is all a
    /// title has asked for.
    async fn set_max_row(_jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, rows: i32) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::setMaxRow({this:?}, {rows})");

        Ok(())
    }

    async fn set_focus(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, focus: bool) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::setFocus({this:?}, {focus})");

        jvm.put_field(&mut this, "__wieFocused", "Z", focus).await
    }

    async fn get_input_mode(_jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::getInputMode({this:?})");

        Ok(Self::reported_mode(context.system().current_input_mode()))
    }

    /// Moves on to the next input mode - 한글, 대문자, 소문자, 숫자 and round
    /// again, a numeric field staying in digits - finishing what was being
    /// composed first, in the mode it was typed in.
    async fn next_input_mode(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::nextInputMode({this:?})");

        Self::finish_composition(jvm, context, &mut this).await?;

        let constraints: i32 = jvm.get_field(&this, "__wieConstraints", "I").await?;
        let next = if constraints & 0xffff == NUMERIC {
            IME_NUMBER
        } else {
            match context.system().current_input_mode() {
                IME_KOREAN => IME_UPPER,
                IME_UPPER => IME_LOWER,
                IME_LOWER => IME_NUMBER,
                _ => IME_KOREAN,
            }
        };
        context.system().set_current_input_mode(next);

        Ok(Self::reported_mode(next))
    }

    fn reported_mode(mode: u32) -> i32 {
        match mode {
            IME_UPPER => MODE_UPPER,
            IME_LOWER => MODE_LOWER,
            IME_NUMBER => MODE_NUMBER,
            _ => MODE_KOREAN,
        }
    }

    async fn key_pressed(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::keyPressed({this:?}, {key_code})");

        Self::handle_key(jvm, context, this, key_code).await
    }

    /// A held key types again, which is what holding a key on a keypad does.
    async fn key_repeated(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::keyRepeated({this:?}, {key_code})");

        Self::handle_key(jvm, context, this, key_code).await
    }

    /// The input method works off presses, so a release changes nothing.
    async fn key_released(_jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::keyReleased({this:?}, {key_code})");

        Ok(())
    }

    /// What a key is, in whichever of the two key conventions the platform
    /// delivers: standard MIDP's negative navigation codes, or SK-VM's.
    fn classify(context: &mut WieJvmContext, key_code: i32) -> Key {
        if (48..=57).contains(&key_code) || key_code == 35 || key_code == 42 {
            return Key::Type;
        }

        if context.system().midp_uses_standard_key_codes() {
            return match key_code {
                STD_KEY_CLEAR => Key::Clear,
                STD_KEY_UP | STD_KEY_DOWN | STD_KEY_LEFT | STD_KEY_RIGHT | STD_KEY_FIRE => Key::Flush,
                _ => Key::Other,
            };
        }

        match MIDPKeyCode::from_raw(key_code) {
            Some(MIDPKeyCode::CLEAR) => Key::Clear,
            Some(MIDPKeyCode::UP | MIDPKeyCode::DOWN | MIDPKeyCode::LEFT | MIDPKeyCode::RIGHT | MIDPKeyCode::FIRE) => Key::Flush,
            _ => Key::Other,
        }
    }

    async fn handle_key(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<()> {
        let focused: bool = jvm.get_field(&this, "__wieFocused", "Z").await?;
        if !focused {
            return Ok(());
        }

        let text: ClassInstanceRef<String> = jvm.get_field(&this, "__wieText", "Ljava/lang/String;").await?;
        let text = JavaLangString::to_rust_string(jvm, &text).await?;
        let composition: i32 = jvm.get_field(&this, "__wieComposition", "I").await?;
        let max_size: i32 = jvm.get_field(&this, "__wieMaxSize", "I").await?;

        let committed = drop_last_chars(&text, composition);

        let updated = match Self::classify(context, key_code) {
            // CLEAR steps back inside an open syllable if there is one, and
            // deletes a finished char if there is not.
            Key::Clear => {
                if composition > 0 {
                    let output = context.system().handle_input_method(IME_BACKSPACE, IME_PRESS);

                    append(&committed, &output, max_size)
                } else {
                    Some((drop_last_chars(&committed, 1), 0))
                }
            }
            // Moving off the field, or confirming it, ends the syllable it was
            // holding - the title reads the text straight after FIRE.
            Key::Flush => {
                let output = context.system().handle_input_method(IME_FLUSH, IME_PRESS);

                append(&committed, &output, max_size)
            }
            Key::Type => {
                let output = context.system().handle_input_method(key_code as i8, IME_PRESS);

                append(&committed, &output, max_size)
            }
            Key::Other => return Ok(()),
        };

        // A refused key has still moved the input method on, so end the
        // syllable it is now holding and drop it, and keep the text as it was.
        let (text, composition) = match updated {
            Some(updated) => updated,
            None => {
                let _ = context.system().handle_input_method(IME_FLUSH, IME_PRESS);

                (text, 0)
            }
        };

        let text = JavaLangString::from_rust_string(jvm, &text).await?;
        jvm.put_field(&mut this, "__wieText", "Ljava/lang/String;", text).await?;
        jvm.put_field(&mut this, "__wieComposition", "I", composition).await?;

        context.system().event_queue().push(Event::Redraw);

        Ok(())
    }

    /// Ends the syllable in progress, leaving it in the text as typed.
    async fn finish_composition(jvm: &Jvm, context: &mut WieJvmContext, this: &mut ClassInstanceRef<Self>) -> JvmResult<()> {
        let composition: i32 = jvm.get_field(this, "__wieComposition", "I").await?;
        if composition <= 0 {
            return Ok(());
        }

        let text: ClassInstanceRef<String> = jvm.get_field(this, "__wieText", "Ljava/lang/String;").await?;
        let text = JavaLangString::to_rust_string(jvm, &text).await?;
        let max_size: i32 = jvm.get_field(this, "__wieMaxSize", "I").await?;

        let committed = drop_last_chars(&text, composition);
        let output = context.system().handle_input_method(IME_FLUSH, IME_PRESS);
        let (text, _) = append(&committed, &output, max_size).unwrap_or((text, 0));

        let text = JavaLangString::from_rust_string(jvm, &text).await?;
        jvm.put_field(this, "__wieText", "Ljava/lang/String;", text).await?;
        jvm.put_field(this, "__wieComposition", "I", 0).await?;

        Ok(())
    }

    /// Draws the field at the origin the title translated to: a white box as
    /// wide as `setWidth` asked and one line high, framed, with the text inside
    /// and a caret after it while the field has focus.
    ///
    /// The colour the title was drawing in is put back afterwards.
    async fn paint(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, graphics: ClassInstanceRef<Graphics>) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::paint({this:?}, {graphics:?})");

        if graphics.is_null() {
            return Ok(());
        }

        let width: i32 = jvm.get_field(&this, "__wieWidth", "I").await?;
        let font = jvm.invoke_virtual(&graphics, "getFont", "()Ljavax/microedition/lcdui/Font;", ()).await?;
        let font_height: i32 = jvm.invoke_virtual(&font, "getHeight", "()I", ()).await?;

        let text: ClassInstanceRef<String> = jvm.get_field(&this, "__wieText", "Ljava/lang/String;").await?;
        let text_width: i32 = jvm.invoke_virtual(&font, "stringWidth", "(Ljava/lang/String;)I", (text.clone(),)).await?;

        // A title that never set a width gets one that fits what is typed.
        let width = if width > 0 { width } else { text_width + 2 * TEXT_INSET + 2 };
        let height = font_height + 2 * TEXT_INSET;

        let restore: i32 = jvm.invoke_virtual(&graphics, "getColor", "()I", ()).await?;

        let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (0xffffffu32 as i32,)).await?;
        let _: () = jvm.invoke_virtual(&graphics, "fillRect", "(IIII)V", (0, 0, width, height)).await?;

        let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (0i32,)).await?;
        let _: () = jvm
            .invoke_virtual(&graphics, "drawRect", "(IIII)V", (0, 0, width - 1, height - 1))
            .await?;

        // A name longer than the box shows its end, where the typing is.
        let x = if text_width > width - 2 * TEXT_INSET - 2 {
            width - TEXT_INSET - 2 - text_width
        } else {
            TEXT_INSET
        };
        let (clip_x, clip_y, clip_width, clip_height): (i32, i32, i32, i32) = (
            jvm.invoke_virtual(&graphics, "getClipX", "()I", ()).await?,
            jvm.invoke_virtual(&graphics, "getClipY", "()I", ()).await?,
            jvm.invoke_virtual(&graphics, "getClipWidth", "()I", ()).await?,
            jvm.invoke_virtual(&graphics, "getClipHeight", "()I", ()).await?,
        );
        let _: () = jvm
            .invoke_virtual(&graphics, "clipRect", "(IIII)V", (1, 1, width - 2, height - 2))
            .await?;
        let _: () = jvm
            .invoke_virtual(&graphics, "drawString", "(Ljava/lang/String;III)V", (text, x, TEXT_INSET, 0i32))
            .await?;

        let focused: bool = jvm.get_field(&this, "__wieFocused", "Z").await?;
        if focused {
            let caret = x + text_width;
            let _: () = jvm
                .invoke_virtual(&graphics, "drawLine", "(IIII)V", (caret, TEXT_INSET, caret, height - TEXT_INSET - 1))
                .await?;
        }

        let _: () = jvm
            .invoke_virtual(&graphics, "setClip", "(IIII)V", (clip_x, clip_y, clip_width, clip_height))
            .await?;
        let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (restore,)).await?;

        Ok(())
    }

    async fn get_string(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<String>> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::getString({this:?})");

        jvm.get_field(&this, "__wieText", "Ljava/lang/String;").await
    }

    /// Replaces the text outright, which also ends any syllable in progress.
    async fn set_string(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, text: ClassInstanceRef<String>) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.TextFieldX::setString({this:?}, {text:?})");

        let text = if text.is_null() {
            JavaLangString::from_rust_string(jvm, "").await?.into()
        } else {
            text
        };

        // Setting the mode again is how the input method is told to drop
        // whatever it was composing.
        let mode = context.system().current_input_mode();
        context.system().set_current_input_mode(mode);

        jvm.put_field(&mut this, "__wieText", "Ljava/lang/String;", text).await?;
        jvm.put_field(&mut this, "__wieComposition", "I", 0).await?;

        Ok(())
    }
}

/// Adds what the input method produced to `committed`, answering the new text
/// and how much of its tail is still being composed, or `None` when it would
/// run past `max_size` - the whole key is refused rather than half a syllable.
fn append(committed: &str, output: &InputMethodOutput, max_size: i32) -> Option<(RustString, i32)> {
    let finished = decode_euc_kr(&output.output0[..output.output0_len]);
    let composing = decode_euc_kr(&output.output1[..output.output1_len]);

    let mut text = committed.to_string();
    text.push_str(&finished);
    text.push_str(&composing);

    if max_size > 0 && text.chars().count() as i32 > max_size {
        return None;
    }

    Some((text, composing.chars().count() as i32))
}

fn decode_euc_kr(bytes: &[u8]) -> RustString {
    encoding_rs::EUC_KR.decode(bytes).0.into_owned()
}

fn drop_last_chars(text: &str, count: i32) -> RustString {
    if count <= 0 {
        return text.to_string();
    }

    let keep = text.chars().count().saturating_sub(count as usize);

    text.chars().take(keep).collect()
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, string::String as RustString};

    use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::{classes::javax::microedition::lcdui::Image, get_protos};

    const KEY_1: i32 = 49;
    const KEY_2: i32 = 50;
    const KEY_4: i32 = 52;
    /// FIRE and CLEAR as the SK-VM table numbers them, which is what the test
    /// platform delivers.
    const FIRE: i32 = 148;
    const CLEAR: i32 = 8;

    /// A field as 다운타운 미니게임천국2 makes it: no label, the current name,
    /// eight chars and any text.
    async fn field(jvm: &Jvm, text: &str) -> JvmResult<ClassInstanceRef<()>> {
        let label = JavaLangString::from_rust_string(jvm, "").await?;
        let text = JavaLangString::from_rust_string(jvm, text).await?;

        Ok(jvm
            .new_class(
                "mmpp/microedition/lcdui/TextFieldX",
                "(Ljava/lang/String;Ljava/lang/String;II)V",
                (label, text, 8i32, 0i32),
            )
            .await?
            .into())
    }

    async fn press(jvm: &Jvm, field: &ClassInstanceRef<()>, key: i32) -> JvmResult<()> {
        jvm.invoke_virtual(field, "keyPressed", "(I)V", (key,)).await
    }

    async fn text(jvm: &Jvm, field: &ClassInstanceRef<()>) -> JvmResult<RustString> {
        let text = jvm.invoke_virtual(field, "getString", "()Ljava/lang/String;", ()).await?;

        JavaLangString::to_rust_string(jvm, &text).await
    }

    /// The field opens on the name it was given, in Hangul, and types onto
    /// the end of it: ㄱ on 4 and ㅣ on 1 make 기.
    #[test]
    fn a_field_types_hangul_after_the_name_it_opened_on() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let field = field(&jvm, "컴투스").await?;

            let mode: i32 = jvm.invoke_virtual(&field, "getInputMode", "()I", ()).await?;
            assert_eq!(mode, 32);

            press(&jvm, &field, KEY_4).await?;
            press(&jvm, &field, KEY_1).await?;
            press(&jvm, &field, FIRE).await?;

            assert_eq!(text(&jvm, &field).await?, "컴투스기");

            Ok(())
        })
    }

    /// CLEAR takes the name back a char at a time.
    #[test]
    fn clear_deletes_from_the_end() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let field = field(&jvm, "컴투스").await?;

            press(&jvm, &field, CLEAR).await?;
            press(&jvm, &field, CLEAR).await?;

            assert_eq!(text(&jvm, &field).await?, "컴");

            Ok(())
        })
    }

    /// The modes go round 한글, 대문자, 소문자, 숫자 under the numbers the title
    /// labels them by, and each types its own.
    #[test]
    fn the_input_mode_goes_round_under_the_titles_numbers() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let field = field(&jvm, "").await?;

            let mut modes = alloc::vec::Vec::new();
            for _ in 0..4 {
                let mode: i32 = jvm.invoke_virtual(&field, "nextInputMode", "()I", ()).await?;
                modes.push(mode);
            }
            assert_eq!(modes, [1, 2, 4, 32]);

            let _: i32 = jvm.invoke_virtual(&field, "nextInputMode", "()I", ()).await?;
            press(&jvm, &field, KEY_2).await?;
            press(&jvm, &field, FIRE).await?;
            assert_eq!(text(&jvm, &field).await?, "A");

            Ok(())
        })
    }

    /// The field paints where the title translated to, as wide as it was
    /// told and framed, and leaves the clip and the colour as it found them.
    #[test]
    fn the_field_paints_a_framed_box_at_the_origin() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let image: ClassInstanceRef<Image> = jvm
                .invoke_static(
                    "javax/microedition/lcdui/Image",
                    "createImage",
                    "(II)Ljavax/microedition/lcdui/Image;",
                    (176, 60),
                )
                .await?;
            let graphics: ClassInstanceRef<()> = jvm
                .new_class(
                    "javax/microedition/lcdui/Graphics",
                    "(Ljavax/microedition/lcdui/Image;)V",
                    (image.clone(),),
                )
                .await?
                .into();
            let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (0x123456i32,)).await?;
            let _: () = jvm.invoke_virtual(&graphics, "fillRect", "(IIII)V", (0, 0, 176, 60)).await?;
            let _: () = jvm.invoke_virtual(&graphics, "translate", "(II)V", (29, 10)).await?;

            let field = field(&jvm, "컴투스").await?;
            let _: () = jvm.invoke_virtual(&field, "setWidth", "(I)V", (118i32,)).await?;
            let _: () = jvm
                .invoke_virtual(&field, "paint", "(Ljavax/microedition/lcdui/Graphics;)V", (graphics.clone(),))
                .await?;

            let colour: i32 = jvm.invoke_virtual(&graphics, "getColor", "()I", ()).await?;
            assert_eq!(colour, 0x123456);
            let clip_width: i32 = jvm.invoke_virtual(&graphics, "getClipWidth", "()I", ()).await?;
            assert_eq!(clip_width, 176);

            let drawn = Image::image(&jvm, &image).await?;
            let frame = drawn.get_pixel(29, 10);
            assert_eq!((frame.r, frame.g, frame.b), (0, 0, 0), "the frame's corner");
            let inside = drawn.get_pixel(29 + 116, 12);
            assert_eq!((inside.r, inside.g, inside.b), (0xff, 0xff, 0xff), "the box's right end");
            let outside = drawn.get_pixel(29 + 119, 12);
            assert_eq!((outside.r, outside.g, outside.b), (0x12, 0x34, 0x56), "past the box");

            Ok(())
        })
    }

    /// More than the field holds is refused whole.
    #[test]
    fn a_full_field_takes_no_more() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let field = field(&jvm, "abcdefgh").await?;

            press(&jvm, &field, KEY_4).await?;
            press(&jvm, &field, FIRE).await?;

            assert_eq!(text(&jvm, &field).await?, "abcdefgh");

            Ok(())
        })
    }
}
