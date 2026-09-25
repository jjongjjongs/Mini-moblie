use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{ClassAccessFlags, MethodAccessFlags};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_midp::classes::net::wie::MIDPKeyCode;

/// How long the same key waits before the next press starts a fresh character
/// rather than cycling to the next letter on the key, in guest milliseconds.
/// Measured on the guest clock so a frontend that runs ticks in batches types
/// the same text as one running live.
const COMMIT_DELAY_MS: i64 = 900;

/// The character sets `*` cycles through. A field opens in small letters
/// (mode 0); capitals and digits follow, and back round.
const MODE_UPPERCASE: i32 = 1;
const MODE_NUMERIC: i32 = 2;
const MODE_COUNT: i32 = 3;

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

/// One edit the input method makes to the attached component: which of the
/// interface's methods to call, and with what.
enum TextEdit {
    /// The key was not the input method's; the game gets it.
    None,
    Insert(u16),
    Replace(u16),
    Delete,
    MoveCursor(i32),
    /// The mode key was pressed: nothing is typed, but the indicator a title
    /// draws from `getInputMode` has changed.
    ModeChanged,
}

// class com.xce.lcdui.TextComponentHandler
//
// SK-VM's keypad input method, the half of the vendor's text input a title
// reaches when it draws its own field (a `TextComponent`) rather than using the
// platform's `XTextField`. The title gets the one handler from the static,
// hands it its component, and routes every key press through `keyPressed`; the
// handler turns the presses into `insert`/`replace`/`delete`/`moveCursor` edits
// on the component and answers whether it took the key. 서울타이쿤's name screen
// is one such field, and without this class it threw NoClassDefFoundError the
// moment it opened, so no key ever reached it.
//
// This types the Latin and numeric modes the shared keypad table carries, the
// same three `*` cycles through on the reference emulator (wfeature,
// `textInputState.press`). A syllable-composing Hangul mode is not part of it,
// there as here.
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
                // twice gets the same composition state.
                JavaFieldProto::new(
                    "__wieInstance",
                    "Lcom/xce/lcdui/TextComponentHandler;",
                    java_constants::FieldAccessFlags::STATIC,
                ),
                // The component the input method edits, or null when a title
                // has turned its field off.
                JavaFieldProto::new("__wieComponent", "Ljava/lang/Object;", Default::default()),
                JavaFieldProto::new("__wieMode", "I", Default::default()),
                // The multi-tap cycle in progress: which key it belongs to (0
                // when none), how far through that key's letters it has gone,
                // and the guest time of the last press.
                JavaFieldProto::new("__wieCycleKey", "I", Default::default()),
                JavaFieldProto::new("__wieCyclePos", "I", Default::default()),
                JavaFieldProto::new("__wieLastKey", "J", Default::default()),
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
    /// every later caller gets the same one - and so the cycle a title is in
    /// the middle of typing survives from one key to the next.
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
    /// title passes null - which is how a title turns its field off. Either way
    /// the cycle in progress ends: it belonged to the field being left.
    async fn set_text_component(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        component: ClassInstanceRef<TextComponent>,
    ) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::setTextComponent({this:?}, {component:?})");

        jvm.put_field(&mut this, "__wieComponent", "Ljava/lang/Object;", component).await?;
        Self::end_cycle(jvm, &mut this).await?;

        Ok(())
    }

    /// Which mode the input method is in, as the bits a title reads to draw the
    /// indicator a handset showed beside a field: 1 capitals, 2 small letters,
    /// 8 digits - the order one local title's own switch names them in.
    async fn get_input_mode(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        let mode: i32 = jvm.get_field(&this, "__wieMode", "I").await?;
        Ok(match mode {
            MODE_UPPERCASE => 1,
            MODE_NUMERIC => 8,
            _ => 2,
        })
    }

    /// Ends the composition in progress, leaving the component and its text
    /// alone. A title calls this from its own `moveCursor` so a cycle that kept
    /// running would not write the next letter over what the caret moved to.
    async fn clear(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::clear({this:?})");

        Self::end_cycle(jvm, &mut this).await?;

        Ok(())
    }

    /// Types one key into the attached component and answers whether the input
    /// method took it. What it does not take reaches the game: a title routes
    /// every key here first, and its pad has to keep working while a field is on
    /// screen, so an unclaimed OK or soft key still confirms the name.
    async fn key_pressed(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, key_code: i32) -> JvmResult<bool> {
        tracing::debug!("com.xce.lcdui.TextComponentHandler::keyPressed({this:?}, {key_code})");

        let component: ClassInstanceRef<TextComponent> = jvm.get_field(&this, "__wieComponent", "Ljava/lang/Object;").await?;
        if component.is_null() {
            return Ok(false);
        }

        let now = context.system().platform().now().raw() as i64;
        let edit = Self::press(jvm, &mut this, key_code, now).await?;

        match edit {
            TextEdit::None => Ok(false),
            TextEdit::ModeChanged => {
                let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                Ok(true)
            }
            TextEdit::MoveCursor(code) => {
                let _: () = jvm.invoke_virtual(&component, "moveCursor", "(I)V", (code,)).await?;
                let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                Ok(true)
            }
            TextEdit::Delete => {
                let _: () = jvm.invoke_virtual(&component, "delete", "()V", ()).await?;
                let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                Ok(true)
            }
            TextEdit::Replace(character) => {
                let _: () = jvm.invoke_virtual(&component, "replace", "(C)V", (character as i32,)).await?;
                let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                Ok(true)
            }
            TextEdit::Insert(character) => {
                // The component holds the text, so the limit it fills to is its
                // to enforce: type only while `size` is under `getMaxSize`. A
                // key that would overflow is still the input method's, taken and
                // dropped rather than passed to the game.
                if !Self::is_full(jvm, &component).await? {
                    let _: () = jvm.invoke_virtual(&component, "insert", "(C)V", (character as i32,)).await?;
                    let _: () = jvm.invoke_virtual(&component, "repaint", "()V", ()).await?;
                }
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

    /// Runs one key through the multi-tap cycle, updating the handler's stored
    /// state and returning the edit the component should be told to make.
    async fn press(jvm: &Jvm, this: &mut ClassInstanceRef<Self>, key_code: i32, now: i64) -> JvmResult<TextEdit> {
        match MIDPKeyCode::from_raw(key_code) {
            Some(MIDPKeyCode::CLEAR) => {
                Self::end_cycle(jvm, this).await?;
                Ok(TextEdit::Delete)
            }
            // The component decides what its own caret does with the key; one
            // local title inserts a space when the caret is already at the end.
            Some(MIDPKeyCode::LEFT | MIDPKeyCode::RIGHT) => {
                Self::end_cycle(jvm, this).await?;
                Ok(TextEdit::MoveCursor(key_code))
            }
            // `*` switches the mode, finishing whatever was being composed
            // first, in the mode it was typed in.
            Some(MIDPKeyCode::KEY_STAR) => {
                let mode: i32 = jvm.get_field(this, "__wieMode", "I").await?;
                jvm.put_field(this, "__wieMode", "I", (mode + 1) % MODE_COUNT).await?;
                Self::end_cycle(jvm, this).await?;
                Ok(TextEdit::ModeChanged)
            }
            Some(MIDPKeyCode::KEY_POUND) => {
                Self::end_cycle(jvm, this).await?;
                Ok(TextEdit::Delete)
            }
            _ => {
                let Some(options) = Self::keypad(key_code) else {
                    return Ok(TextEdit::None);
                };
                let mode: i32 = jvm.get_field(this, "__wieMode", "I").await?;

                if mode == MODE_NUMERIC {
                    Self::end_cycle(jvm, this).await?;
                    return Ok(TextEdit::Insert(key_code as u16));
                }

                let cycle_key: i32 = jvm.get_field(this, "__wieCycleKey", "I").await?;
                let last_key: i64 = jvm.get_field(this, "__wieLastKey", "J").await?;

                // A second press of the same key inside the commit delay writes
                // the next letter over the one it just produced.
                if cycle_key == key_code && now - last_key < COMMIT_DELAY_MS {
                    let position: i32 = jvm.get_field(this, "__wieCyclePos", "I").await?;
                    let position = (position + 1) % options.len() as i32;
                    jvm.put_field(this, "__wieCyclePos", "I", position).await?;
                    jvm.put_field(this, "__wieLastKey", "J", now).await?;
                    return Ok(TextEdit::Replace(Self::apply_mode(mode, options[position as usize])));
                }

                jvm.put_field(this, "__wieCycleKey", "I", key_code).await?;
                jvm.put_field(this, "__wieCyclePos", "I", 0).await?;
                jvm.put_field(this, "__wieLastKey", "J", now).await?;
                Ok(TextEdit::Insert(Self::apply_mode(mode, options[0])))
            }
        }
    }

    /// Commits whatever character the cycle was on, so the next press of the
    /// same key inserts rather than replaces.
    async fn end_cycle(jvm: &Jvm, this: &mut ClassInstanceRef<Self>) -> JvmResult<()> {
        jvm.put_field(this, "__wieCycleKey", "I", 0).await?;
        jvm.put_field(this, "__wieCyclePos", "I", 0).await?;

        Ok(())
    }

    /// Whether another character fits: the two numbers are the only way to
    /// know, since the interface hands out edits and never a character back.
    async fn is_full(jvm: &Jvm, component: &ClassInstanceRef<TextComponent>) -> JvmResult<bool> {
        let size: i32 = jvm.invoke_virtual(component, "size", "()I", ()).await?;
        let max_size: i32 = jvm.invoke_virtual(component, "getMaxSize", "()I", ()).await?;

        Ok(max_size > 0 && size >= max_size)
    }

    /// What one keypad key cycles through, in the order a handset produced
    /// them, or `None` when the pad does not carry the key.
    fn keypad(key: i32) -> Option<&'static [u8]> {
        Some(match key {
            49 => b".,?!'\"1-()@/:_",
            50 => b"abc2",
            51 => b"def3",
            52 => b"ghi4",
            53 => b"jkl5",
            54 => b"mno6",
            55 => b"pqrs7",
            56 => b"tuv8",
            57 => b"wxyz9",
            48 => b" 0",
            _ => return None,
        })
    }

    /// The letter a key produces in the active mode: as the keypad table
    /// carries it, or its capital.
    fn apply_mode(mode: i32, character: u8) -> u16 {
        if mode == MODE_UPPERCASE {
            character.to_ascii_uppercase() as u16
        } else {
            character as u16
        }
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

    const KEY_2: i32 = 50;
    const KEY_3: i32 = 51;
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

    async fn text(jvm: &Jvm, component: &ClassInstanceRef<()>) -> JvmResult<RustString> {
        let text = jvm.get_field(component, "text", "Ljava/lang/String;").await?;
        JavaLangString::to_rust_string(jvm, &text).await
    }

    /// A keypad press types its first letter, and a different key adds the
    /// next one after it.
    #[test]
    fn keys_type_letters_into_the_component() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;

            assert!(press(&jvm, &handler, KEY_2).await?);
            assert_eq!(text(&jvm, &component).await?, "a");

            assert!(press(&jvm, &handler, KEY_3).await?);
            assert_eq!(text(&jvm, &component).await?, "ad");

            Ok(())
        })
    }

    /// Tapping the same key again inside the commit delay writes the next
    /// letter over the one it just produced - that is multi-tap.
    #[test]
    fn tapping_the_same_key_cycles_in_place() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;

            press(&jvm, &handler, KEY_2).await?;
            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "b");

            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "c");

            Ok(())
        })
    }

    /// `*` moves to capitals, then to digits, and `getInputMode` reports each.
    #[test]
    fn star_cycles_the_mode() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;

            // Small letters to start.
            let mode: i32 = jvm.invoke_virtual(&handler, "getInputMode", "()I", ()).await?;
            assert_eq!(mode, 2);

            press(&jvm, &handler, STAR).await?;
            let mode: i32 = jvm.invoke_virtual(&handler, "getInputMode", "()I", ()).await?;
            assert_eq!(mode, 1);
            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "A");

            press(&jvm, &handler, STAR).await?;
            let mode: i32 = jvm.invoke_virtual(&handler, "getInputMode", "()I", ()).await?;
            assert_eq!(mode, 8);
            press(&jvm, &handler, KEY_2).await?;
            assert_eq!(text(&jvm, &component).await?, "A2");

            Ok(())
        })
    }

    /// CLEAR deletes the character before the caret; LEFT moves the caret so
    /// the next letter lands inside the text.
    #[test]
    fn clear_deletes_and_the_caret_moves() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let (handler, component) = attach(&jvm, 0).await?;

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

            assert!(press(&jvm, &handler, KEY_2).await?);
            assert!(press(&jvm, &handler, KEY_3).await?); // refused: field is full
            assert_eq!(text(&jvm, &component).await?, "a");

            // OK is the game's, not the field's.
            assert!(!press(&jvm, &handler, OK).await?);

            Ok(())
        })
    }
}
