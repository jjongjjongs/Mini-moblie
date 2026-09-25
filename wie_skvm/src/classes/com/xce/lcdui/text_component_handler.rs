use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{ClassAccessFlags, MethodAccessFlags};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_backend::InputMethodOutput;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_midp::classes::net::wie::MIDPKeyCode;

/// Tells the input method to finish the syllable it is holding and hand it
/// over, adding nothing new - what a directional or mode key does when it ends
/// the composition.
const IME_FLUSH: i8 = -99;

/// Tells the input method to step one stroke back inside the syllable it is
/// composing, which is what CLEAR means while a syllable is still open.
const IME_BACKSPACE: i8 = -16;

/// A key press, as the input method numbers its events.
const IME_PRESS: u32 = 2;

/// The input method's Hangul mode, which the platform's field opens in.
const KOREAN_MODE: u32 = 3;

/// How many modes `*` cycles through: small letters, capitals, digits, Hangul.
const INPUT_MODES: u32 = 4;

// interface com.xce.lcdui.TextComponent
//
// The vendor's own text component: the interface a title implements when it
// keeps the text itself and lets the platform's input method drive it. It has
// `insert`, `replace`, `delete` and `moveCursor` and no way to read a character
// back, so the handset's input method never held the text - it sent the edits
// and the title kept them. That is what a multi-tap cycle needs: `insert` for a
// new character and `replace` for the next letter on the same key.
pub struct TextComponent;

impl TextComponent {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/xce/lcdui/TextComponent",
            parent_class: None,
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new_abstract("getCaretPosition", "()I", Default::default()),
                JavaMethodProto::new_abstract("getConstraints", "()I", Default::default()),
                JavaMethodProto::new_abstract("getMaxSize", "()I", Default::default()),
                JavaMethodProto::new_abstract("size", "()I", Default::default()),
                JavaMethodProto::new_abstract("insert", "(C)V", Default::default()),
                JavaMethodProto::new_abstract("delete", "()V", Default::default()),
                JavaMethodProto::new_abstract("clear", "()V", Default::default()),
                JavaMethodProto::new_abstract("replace", "(C)V", Default::default()),
                JavaMethodProto::new_abstract("moveCursor", "(I)V", Default::default()),
                JavaMethodProto::new_abstract("setCaretPosition", "(I)V", Default::default()),
                JavaMethodProto::new_abstract("setCaretVisible", "(Z)V", Default::default()),
                JavaMethodProto::new_abstract("repaint", "()V", Default::default()),
                JavaMethodProto::new_abstract("repaintIM", "()V", Default::default()),
            ],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}

// class com.xce.lcdui.TextComponentHandler
//
// SK-VM's keypad input method. A title draws its own field and reaches this two
// ways: it hands the handler a `TextComponent` and routes keys through
// `keyPressed` (서울타이쿤), or it focuses an `XTextField` and routes keys the
// same way without handing over a component (댄스배틀오디션). Either way the
// press becomes an edit on the field, and the handler answers whether it took
// the key - what it does not take reaches the game, so an unclaimed OK still
// confirms the name. Without this class both titles threw NoClassDefFoundError
// the moment their name screen opened.
//
// The composition is the shared keypad editor every text field in this runtime
// types with - the same `MC_uicHandleInput` an XTextField uses - so a syllable
// builds up over several keys and Hangul composes here as it does there. A
// component holds the text and cannot be read back, so the handler keeps how
// many of its trailing characters are the syllable still open, and replaces
// exactly those on the next key.
pub struct TextComponentHandler;

impl TextComponentHandler {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/xce/lcdui/TextComponentHandler",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new(
                    "getTextComponentHandler",
                    "()Lcom/xce/lcdui/TextComponentHandler;",
                    Self::get_text_component_handler,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "setTextComponent",
                    "(Lcom/xce/lcdui/TextComponent;)V",
                    Self::set_text_component,
                    Default::default(),
                ),
                JavaMethodProto::new("getInputMode", "()I", Self::get_input_mode, Default::default()),
                JavaMethodProto::new("clear", "()V", Self::clear, Default::default()),
                JavaMethodProto::new("keyPressed", "(I)Z", Self::key_pressed, Default::default()),
                JavaMethodProto::new("keyReleased", "(I)Z", Self::key_released, Default::default()),
            ],
            fields: vec![
                // The one handler a handset has, kept so a title that asks
                // twice gets the same object.
                JavaFieldProto::new(
                    "__wieInstance",
                    "Lcom/xce/lcdui/TextComponentHandler;",
                    java_constants::FieldAccessFlags::STATIC,
                ),
                // The XTextField that last took focus, or null. A title that
                // draws the platform's own field routes its keys through the
                // handler without ever handing it a component - the field it
                // focused is the one the keys are for. 댄스배틀오디션 is one.
                JavaFieldProto::new(
                    "__wieFocusedField",
                    "Lcom/xce/lcdui/XTextField;",
                    java_constants::FieldAccessFlags::STATIC,
                ),
                // The component the input method edits, or null when a title
                // has turned its field off.
                JavaFieldProto::new("__wieComponent", "Ljava/lang/Object;", Default::default()),
                // How many characters at the caret are the syllable still being
                // composed, and so are replaced rather than added to by the
                // next key.
                JavaFieldProto::new("__wieComposition", "I", Default::default()),
            ],
            access_flags: ClassAccessFlags::FINAL,
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        Ok(())
    }

    /// The handler singleton, made on the first call and kept in the static so
    /// every later caller gets the same one.
    async fn get_text_component_handler(jvm: &Jvm, _context: &mut WieJvmContext) -> JvmResult<ClassInstanceRef<Self>> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::getTextComponentHandler()");

        let existing: ClassInstanceRef<Self> = jvm
            .get_static_field(
                "com/xce/lcdui/TextComponentHandler",
                "__wieInstance",
                "Lcom/xce/lcdui/TextComponentHandler;",
            )
            .await?;
        if !existing.is_null() {
            return Ok(existing);
        }

        let handler: ClassInstanceRef<Self> = jvm.new_class("com/xce/lcdui/TextComponentHandler", "()V", ()).await?.into();
        jvm.put_static_field(
            "com/xce/lcdui/TextComponentHandler",
            "__wieInstance",
            "Lcom/xce/lcdui/TextComponentHandler;",
            handler.clone(),
        )
        .await?;

        Ok(handler)
    }

    /// Attaches the component the input method edits, or detaches it when a
    /// title passes null - which is how a title turns its field off. Attaching
    /// opens the field in Hangul, the mode a handset's name screen started in,
    /// and either way ends the composition in progress.
    async fn set_text_component(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        component: ClassInstanceRef<TextComponent>,
    ) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::setTextComponent({this:?}, {component:?})");

        let attaching = !component.is_null();
        jvm.put_field(&mut this, "__wieComponent", "Ljava/lang/Object;", component).await?;
        jvm.put_field(&mut this, "__wieComposition", "I", 0).await?;

        if attaching {
            context.system().set_current_input_mode(KOREAN_MODE);
        } else {
            context.system().reset_input_method_composition();
        }

        Ok(())
    }

    /// Which mode the input method is in, as the bits a title reads to draw the
    /// indicator a handset showed beside a field: 16 Hangul, 1 capitals, 2 small
    /// letters, 8 digits - the order one local title's own switch names them in.
    async fn get_input_mode(_jvm: &Jvm, context: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        Ok(match context.system().current_input_mode() {
            1 => 1,
            2 => 8,
            KOREAN_MODE => 16,
            _ => 2,
        })
    }

    /// Ends the composition in progress, leaving the component and its text
    /// alone. A title calls this from its own `moveCursor` so a cycle that kept
    /// running would not write the next letter over what the caret moved to.
    async fn clear(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::clear({this:?})");

        jvm.put_field(&mut this, "__wieComposition", "I", 0).await?;
        context.system().reset_input_method_composition();

        Ok(())
    }

    /// Types one key into the field and answers whether the input method took
    /// it. With a component attached the key edits it; with none, the key is
    /// for the XTextField a title focused. What the field has no use for - OK,
    /// the soft keys, up and down - is left for the game so its name screen can
    /// confirm and navigate.
    async fn key_pressed(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<bool> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::keyPressed({this:?}, {key_code})");

        let component: ClassInstanceRef<TextComponent> = jvm.get_field(&this, "__wieComponent", "Ljava/lang/Object;").await?;
        if component.is_null() {
            // No component was handed over, so the keys are for the field a
            // title focused. Its own keyPressed is the same shared editor, so
            // it types (Hangul and all) directly.
            let field: ClassInstanceRef<()> = jvm
                .get_static_field("com/xce/lcdui/TextComponentHandler", "__wieFocusedField", "Lcom/xce/lcdui/XTextField;")
                .await?;
            if !field.is_null() && Self::is_field_key(key_code) {
                let _: () = jvm.invoke_virtual(&field, "keyPressed", "(I)V", (key_code,)).await?;
                return Ok(true);
            }
            return Ok(false);
        }

        let composition: i32 = jvm.get_field(&this, "__wieComposition", "I").await?;

        // The edit turns the shared editor's output into calls on the
        // component: `updated` is the new composing length, `None` a key the
        // component was too full to take, and a bare `Ok(false)` return a key
        // the field does not want.
        let updated = match MIDPKeyCode::from_raw(key_code) {
            // CLEAR steps back inside an open syllable, or deletes a finished
            // character when there is none.
            Some(MIDPKeyCode::CLEAR) => {
                let result = if composition > 0 {
                    let output = context.system().handle_input_method(IME_BACKSPACE, IME_PRESS);
                    Self::apply_output(jvm, &component, composition, &output).await?
                } else {
                    let _: () = jvm.invoke_virtual(&component, "delete", "()V", ()).await?;
                    let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                    Some(0)
                };
                // Nothing is composing now, so drop any half-built state the
                // step-back left, and the next key starts a fresh character.
                if matches!(result, Some(0)) {
                    context.system().reset_input_method_composition();
                }
                result
            }
            // `*` switches the mode, finishing whatever was being composed
            // first, in the mode it was typed in.
            Some(MIDPKeyCode::KEY_STAR) => {
                let output = context.system().handle_input_method(IME_FLUSH, IME_PRESS);
                let committed = Self::apply_output(jvm, &component, composition, &output).await?.unwrap_or(0);
                let mode = context.system().current_input_mode();
                context.system().set_current_input_mode((mode + 1) % INPUT_MODES);
                let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                Some(committed)
            }
            // Moving off the field ends the syllable it was holding. Up and
            // down are the game's, so the field commits and lets them by.
            Some(MIDPKeyCode::UP | MIDPKeyCode::DOWN) => {
                let output = context.system().handle_input_method(IME_FLUSH, IME_PRESS);
                Self::apply_output(jvm, &component, composition, &output).await?;
                jvm.put_field(&mut this, "__wieComposition", "I", 0).await?;
                return Ok(false);
            }
            // Left and right commit the syllable and move the component's own
            // caret, which the component decides the meaning of.
            Some(MIDPKeyCode::LEFT | MIDPKeyCode::RIGHT) => {
                let output = context.system().handle_input_method(IME_FLUSH, IME_PRESS);
                Self::apply_output(jvm, &component, composition, &output).await?;
                let _: () = jvm.invoke_virtual(&component, "moveCursor", "(I)V", (key_code,)).await?;
                let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                jvm.put_field(&mut this, "__wieComposition", "I", 0).await?;
                return Ok(true);
            }
            Some(
                MIDPKeyCode::KEY_NUM0
                | MIDPKeyCode::KEY_NUM1
                | MIDPKeyCode::KEY_NUM2
                | MIDPKeyCode::KEY_NUM3
                | MIDPKeyCode::KEY_NUM4
                | MIDPKeyCode::KEY_NUM5
                | MIDPKeyCode::KEY_NUM6
                | MIDPKeyCode::KEY_NUM7
                | MIDPKeyCode::KEY_NUM8
                | MIDPKeyCode::KEY_NUM9
                | MIDPKeyCode::KEY_POUND,
            ) => {
                let output = context.system().handle_input_method(key_code as i8, IME_PRESS);
                Self::apply_output(jvm, &component, composition, &output).await?
            }
            // Soft keys, CALL, OK - the game's, not the field's.
            _ => return Ok(false),
        };

        match updated {
            Some(new_composition) => {
                jvm.put_field(&mut this, "__wieComposition", "I", new_composition).await?;
                Ok(true)
            }
            // The key would have run the field past its limit, so it is refused
            // whole: the composition it advanced is dropped and the text left
            // as it was, the same as a full field taking nothing.
            None => {
                context.system().reset_input_method_composition();
                jvm.put_field(&mut this, "__wieComposition", "I", 0).await?;
                Ok(true)
            }
        }
    }

    /// Composition happens on the press, so a title that gets the release is
    /// free to act on it: the input method did not take it.
    async fn key_released(_jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<bool> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::keyReleased({this:?}, {key_code})");

        Ok(false)
    }

    /// Replays the shared editor's output onto the component: the syllable it
    /// was composing (`composition` characters at the caret) is replaced by
    /// what the editor now reports finished and still open, both as EUC-KR
    /// bytes. Answers the new composing length, or `None` when the result would
    /// run the component past `getMaxSize` - the whole key is refused then,
    /// rather than half a syllable.
    async fn apply_output(
        jvm: &Jvm,
        component: &ClassInstanceRef<TextComponent>,
        composition: i32,
        output: &InputMethodOutput,
    ) -> JvmResult<Option<i32>> {
        let finished = encoding_rs::EUC_KR.decode(&output.output0[..output.output0_len]).0;
        let composing = encoding_rs::EUC_KR.decode(&output.output1[..output.output1_len]).0;
        let finished_len = finished.chars().count() as i32;
        let composing_len = composing.chars().count() as i32;

        let size: i32 = jvm.invoke_virtual(component, "size", "()I", ()).await?;
        let max_size: i32 = jvm.invoke_virtual(component, "getMaxSize", "()I", ()).await?;
        if max_size > 0 && size - composition + finished_len + composing_len > max_size {
            return Ok(None);
        }

        for _ in 0..composition {
            let _: () = jvm.invoke_virtual(component, "delete", "()V", ()).await?;
        }
        for character in finished.chars().chain(composing.chars()) {
            let _: () = jvm.invoke_virtual(component, "insert", "(C)V", (character as i32,)).await?;
        }
        let _: () = jvm.invoke_virtual(component, "repaint", "()V", ()).await?;

        Ok(Some(composing_len))
    }

    /// Whether the key is one the input method types with, rather than one the
    /// game reads for itself. The digits, `*`, `#`, CLEAR and the two side keys
    /// are the field's; OK, the soft keys, and up and down are the game's, so a
    /// name screen can still confirm and move between rows.
    fn is_field_key(key_code: i32) -> bool {
        (48..=57).contains(&key_code)
            || matches!(
                MIDPKeyCode::from_raw(key_code),
                Some(MIDPKeyCode::CLEAR | MIDPKeyCode::LEFT | MIDPKeyCode::RIGHT | MIDPKeyCode::KEY_STAR | MIDPKeyCode::KEY_POUND)
            )
    }

    /// Records the field a title focused, or clears it when that field loses
    /// focus, so keys routed through the handler with no component attached
    /// reach the field the title is drawing. Called from `XTextField.setFocus`.
    pub(crate) async fn set_focused_field<T>(jvm: &Jvm, field: &ClassInstanceRef<T>, focused: bool) -> JvmResult<()> {
        const CLASS: &str = "com/xce/lcdui/TextComponentHandler";
        const NAME: &str = "__wieFocusedField";
        const DESC: &str = "Lcom/xce/lcdui/XTextField;";

        if focused {
            return jvm.put_static_field(CLASS, NAME, DESC, field.clone()).await;
        }

        // Only the field that is still the registered one clears it: a field
        // losing focus after a newer one took it must not unregister the newer.
        let current: ClassInstanceRef<()> = jvm.get_static_field(CLASS, NAME, DESC).await?;
        if !current.is_null() && current.identity() == field.identity() {
            jvm.put_static_field(CLASS, NAME, DESC, ClassInstanceRef::<()>::new(None)).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use alloc::{boxed::Box, string::String as RustString, vec};

    use java_class_proto::{JavaFieldProto, JavaMethodProto};
    use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

    use test_utils::run_jvm_test;
    use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
    use wie_util::Result;

    use crate::get_protos;

    const KEY_1: i32 = 49;
    const KEY_2: i32 = 50;
    const KEY_3: i32 = 51;
    const KEY_4: i32 = 52;
    const KEY_5: i32 = 53;
    const STAR: i32 = 42;
    const CLEAR: i32 = 8;
    const LEFT: i32 = 142;
    const OK: i32 = 148;

    /// A minimal `TextComponent`: a title's own field, keeping the text the
    /// handler sends it edits for. It holds the string and a caret, and applies
    /// each interface call the way a real field would, so a test can read back
    /// what the handler typed.
    struct TestTextComponent;

    impl TestTextComponent {
        fn as_proto() -> WieJavaClassProto {
            WieJavaClassProto {
                name: "TestTextComponent",
                parent_class: Some("java/lang/Object"),
                interfaces: vec!["com/xce/lcdui/TextComponent"],
                methods: vec![
                    JavaMethodProto::new("<init>", "(I)V", Self::init, Default::default()),
                    JavaMethodProto::new("insert", "(C)V", Self::insert, Default::default()),
                    JavaMethodProto::new("replace", "(C)V", Self::replace, Default::default()),
                    JavaMethodProto::new("delete", "()V", Self::delete, Default::default()),
                    JavaMethodProto::new("moveCursor", "(I)V", Self::move_cursor, Default::default()),
                    JavaMethodProto::new("size", "()I", Self::size, Default::default()),
                    JavaMethodProto::new("getMaxSize", "()I", Self::get_max_size, Default::default()),
                    JavaMethodProto::new("getCaretPosition", "()I", Self::get_caret_position, Default::default()),
                    JavaMethodProto::new("getConstraints", "()I", Self::zero, Default::default()),
                    JavaMethodProto::new("clear", "()V", Self::noop, Default::default()),
                    JavaMethodProto::new("setCaretPosition", "(I)V", Self::noop_int, Default::default()),
                    JavaMethodProto::new("setCaretVisible", "(Z)V", Self::noop_int, Default::default()),
                    JavaMethodProto::new("repaint", "()V", Self::noop, Default::default()),
                    JavaMethodProto::new("repaintIM", "()V", Self::noop, Default::default()),
                ],
                fields: vec![
                    JavaFieldProto::new("text", "Ljava/lang/String;", Default::default()),
                    JavaFieldProto::new("caret", "I", Default::default()),
                    JavaFieldProto::new("max", "I", Default::default()),
                ],
                access_flags: Default::default(),
            }
        }

        async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, max: i32) -> JvmResult<()> {
            let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;
            let empty = JavaLangString::from_rust_string(jvm, "").await?;
            jvm.put_field(&mut this, "text", "Ljava/lang/String;", empty).await?;
            jvm.put_field(&mut this, "caret", "I", 0).await?;
            jvm.put_field(&mut this, "max", "I", max).await?;
            Ok(())
        }

        async fn chars(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> JvmResult<(alloc::vec::Vec<char>, usize)> {
            let text = jvm.get_field(this, "text", "Ljava/lang/String;").await?;
            let text = JavaLangString::to_rust_string(jvm, &text).await?;
            let caret: i32 = jvm.get_field(this, "caret", "I").await?;
            Ok((text.chars().collect(), caret as usize))
        }

        async fn store(jvm: &Jvm, this: &mut ClassInstanceRef<Self>, chars: &[char], caret: usize) -> JvmResult<()> {
            let text: RustString = chars.iter().collect();
            let text = JavaLangString::from_rust_string(jvm, &text).await?;
            jvm.put_field(this, "text", "Ljava/lang/String;", text).await?;
            jvm.put_field(this, "caret", "I", caret as i32).await?;
            Ok(())
        }

        async fn insert(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, character: i32) -> JvmResult<()> {
            let (mut chars, caret) = Self::chars(jvm, &this).await?;
            chars.insert(caret, char::from_u32(character as u32).unwrap());
            Self::store(jvm, &mut this, &chars, caret + 1).await
        }

        async fn replace(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, character: i32) -> JvmResult<()> {
            let (mut chars, caret) = Self::chars(jvm, &this).await?;
            if caret > 0 {
                chars[caret - 1] = char::from_u32(character as u32).unwrap();
            }
            Self::store(jvm, &mut this, &chars, caret).await
        }

        async fn delete(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
            let (mut chars, caret) = Self::chars(jvm, &this).await?;
            if caret > 0 {
                chars.remove(caret - 1);
                return Self::store(jvm, &mut this, &chars, caret - 1).await;
            }
            Ok(())
        }

        async fn move_cursor(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<()> {
            let (chars, caret) = Self::chars(jvm, &this).await?;
            let caret = match key_code {
                LEFT => caret.saturating_sub(1),
                _ => (caret + 1).min(chars.len()),
            };
            Self::store(jvm, &mut this, &chars, caret).await
        }

        async fn size(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
            Ok(Self::chars(jvm, &this).await?.0.len() as i32)
        }

        async fn get_max_size(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
            jvm.get_field(&this, "max", "I").await
        }

        async fn get_caret_position(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
            jvm.get_field(&this, "caret", "I").await
        }

        async fn zero(_: &Jvm, _: &mut WieJvmContext, _: ClassInstanceRef<Self>) -> JvmResult<i32> {
            Ok(0)
        }

        async fn noop(_: &Jvm, _: &mut WieJvmContext, _: ClassInstanceRef<Self>) -> JvmResult<()> {
            Ok(())
        }

        async fn noop_int(_: &Jvm, _: &mut WieJvmContext, _: ClassInstanceRef<Self>, _: i32) -> JvmResult<()> {
            Ok(())
        }
    }

    fn protos() -> Box<[Box<[WieJavaClassProto]>]> {
        Box::new([
            wie_midp::get_protos().into(),
            get_protos().into(),
            vec![TestTextComponent::as_proto()].into(),
        ])
    }

    /// Makes the handler, attaches a fresh component, and hands both back.
    async fn attach(jvm: &Jvm, max: i32) -> JvmResult<(ClassInstanceRef<()>, ClassInstanceRef<()>)> {
        let handler: ClassInstanceRef<()> = jvm
            .invoke_static(
                "com/xce/lcdui/TextComponentHandler",
                "getTextComponentHandler",
                "()Lcom/xce/lcdui/TextComponentHandler;",
                (),
            )
            .await?;
        let component: ClassInstanceRef<()> = jvm.new_class("TestTextComponent", "(I)V", (max,)).await?.into();
        let _: () = jvm
            .invoke_virtual(&handler, "setTextComponent", "(Lcom/xce/lcdui/TextComponent;)V", (component.clone(),))
            .await?;
        Ok((handler, component))
    }

    async fn press(jvm: &Jvm, handler: &ClassInstanceRef<()>, key: i32) -> JvmResult<bool> {
        jvm.invoke_virtual(handler, "keyPressed", "(I)Z", (key,)).await
    }

    async fn mode(jvm: &Jvm, handler: &ClassInstanceRef<()>) -> JvmResult<i32> {
        jvm.invoke_virtual(handler, "getInputMode", "()I", ()).await
    }

    async fn text(jvm: &Jvm, component: &ClassInstanceRef<()>) -> JvmResult<RustString> {
        let text = jvm.get_field(component, "text", "Ljava/lang/String;").await?;
        JavaLangString::to_rust_string(jvm, &text).await
    }

    /// The field opens in Hangul, so 4 then 1 compose 기 into the component -
    /// the syllable built up over the two keys and replaced in place.
    #[test]
    fn a_component_composes_hangul() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;
            assert_eq!(mode(&jvm, &handler).await?, 16);

            assert!(press(&jvm, &handler, KEY_4).await?);
            assert!(press(&jvm, &handler, KEY_1).await?);
            assert_eq!(text(&jvm, &component).await?, "기");

            Ok(())
        })
    }

    /// `*` walks the modes - Hangul to small letters to capitals to digits -
    /// and `getInputMode` reports each as its bit.
    #[test]
    fn star_cycles_the_mode() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;

            assert_eq!(mode(&jvm, &handler).await?, 16); // Hangul
            press(&jvm, &handler, STAR).await?;
            assert_eq!(mode(&jvm, &handler).await?, 2); // small letters
            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "a");

            press(&jvm, &handler, STAR).await?;
            assert_eq!(mode(&jvm, &handler).await?, 1); // capitals
            press(&jvm, &handler, STAR).await?;
            assert_eq!(mode(&jvm, &handler).await?, 8); // digits
            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "a2");

            Ok(())
        })
    }

    /// In the letter modes the same key cycles the character it just produced.
    #[test]
    fn tapping_the_same_key_cycles_in_place() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;
            press(&jvm, &handler, STAR).await?; // small letters

            press(&jvm, &handler, KEY_2).await?;
            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "b");

            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "c");

            Ok(())
        })
    }

    /// CLEAR deletes the character before the caret; LEFT commits and moves it
    /// so the next letter lands inside the text.
    #[test]
    fn clear_deletes_and_the_caret_moves() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;
            press(&jvm, &handler, STAR).await?; // small letters

            press(&jvm, &handler, KEY_2).await?;
            press(&jvm, &handler, KEY_3).await?;
            assert_eq!(text(&jvm, &component).await?, "ad");

            assert!(press(&jvm, &handler, CLEAR).await?);
            assert_eq!(text(&jvm, &component).await?, "a");

            press(&jvm, &handler, KEY_3).await?; // "ad"
            assert!(press(&jvm, &handler, LEFT).await?);
            press(&jvm, &handler, KEY_5).await?; // 'j' inserted before 'd'
            assert_eq!(text(&jvm, &component).await?, "ajd");

            Ok(())
        })
    }

    /// A full field takes no more, and a key the input method has no use for is
    /// left for the game.
    #[test]
    fn a_full_field_refuses_and_other_keys_pass_through() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 1).await?;
            press(&jvm, &handler, STAR).await?; // small letters

            assert!(press(&jvm, &handler, KEY_2).await?);
            assert!(press(&jvm, &handler, KEY_3).await?); // refused: field is full
            assert_eq!(text(&jvm, &component).await?, "a");

            // OK is the game's, not the field's.
            assert!(!press(&jvm, &handler, OK).await?);

            Ok(())
        })
    }

    /// A title that focuses an XTextField and routes its keys through the
    /// handler without attaching a component - 댄스배틀오디션 - types into that
    /// focused field, Hangul and all: 4 then 1 make 기 in the field it opened.
    #[test]
    fn keys_reach_the_focused_xtextfield_with_no_component() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let empty = JavaLangString::from_rust_string(&jvm, "").await?;
            let field: ClassInstanceRef<()> = jvm
                .new_class(
                    "com/xce/lcdui/XTextField",
                    "(Ljava/lang/String;IILjavax/microedition/lcdui/Canvas;)V",
                    (empty, 8i32, 0i32, ClassInstanceRef::<()>::new(None)),
                )
                .await?
                .into();
            let _: () = jvm.invoke_virtual(&field, "setFocus", "(Z)V", (true,)).await?;

            let handler: ClassInstanceRef<()> = jvm
                .invoke_static(
                    "com/xce/lcdui/TextComponentHandler",
                    "getTextComponentHandler",
                    "()Lcom/xce/lcdui/TextComponentHandler;",
                    (),
                )
                .await?;

            assert!(press(&jvm, &handler, KEY_4).await?);
            assert!(press(&jvm, &handler, KEY_1).await?);

            let typed = jvm.invoke_virtual(&field, "getText", "()Ljava/lang/String;", ()).await?;
            assert_eq!(JavaLangString::to_rust_string(&jvm, &typed).await?, "기");

            // OK is still the game's to confirm the name with.
            assert!(!press(&jvm, &handler, OK).await?);

            Ok(())
        })
    }
}
