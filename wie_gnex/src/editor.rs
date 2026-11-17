//! The handset's own text entry, which a script opens with service 0xb4 to
//! have its player type a name or a number.
//!
//! On the handset the call hands the screen to the phone's editor and does not
//! come back until the player is done, with the text in the resource the
//! script named. So while the editor is up the script is held: its timers do
//! not run and the keys are the editor's. The editor types through the same
//! input method WIPI titles do - 천지인 for Korean, multi-tap for letters.

use alloc::{string::String, vec::Vec};

use wie_backend::{KeyCode, System};

use crate::graphics::{Graphics, RAW_COLOR, TEXT_HEIGHT, text_width};

/// What 0xb4 asks for: a prompt, the string resource the text goes into, the
/// most bytes it may hold, and whether only digits are wanted.
pub struct TextRequest {
    pub prompt: Vec<u8>,
    pub destination: i32,
    pub limit: usize,
    pub numeric: bool,
}

/// How a key left the editor.
pub enum Outcome {
    Editing,
    /// Done, with the text to store; `None` when the player backed out.
    Done(Option<Vec<u8>>),
}

/// The input method's modes, as `InputMethod` numbers them.
const LOWER: u32 = 0;
const UPPER: u32 = 1;
const NUMBER: u32 = 2;
const KOREAN: u32 = 3;

/// The input method's key that finishes the character in progress, and the
/// one that takes back the last stroke of a Korean syllable.
const FLUSH: i8 = -99;
const UNDO: i8 = -16;

pub struct Editor {
    pub destination: i32,
    prompt: String,
    limit: usize,
    numeric: bool,
    /// The finished text, EUC-KR.
    text: Vec<u8>,
    /// The character still being composed.
    composing: Vec<u8>,
    mode: u32,
    /// The screen the script last showed, which the editor is drawn over and
    /// which comes back when it closes.
    pub underneath: Vec<u8>,
}

impl Editor {
    pub fn new(system: &System, request: TextRequest, initial: Vec<u8>, underneath: Vec<u8>) -> Self {
        let mode = if request.numeric { NUMBER } else { KOREAN };
        system.set_current_input_mode(mode);
        let mut text = initial;
        truncate(&mut text, request.limit);
        Self {
            destination: request.destination,
            prompt: encoding_rs::EUC_KR.decode(&request.prompt).0.into_owned(),
            limit: request.limit,
            numeric: request.numeric,
            text,
            composing: Vec::new(),
            mode,
            underneath,
        }
    }

    pub fn key(&mut self, system: &System, code: &KeyCode) -> Outcome {
        let typed = match code {
            KeyCode::NUM0 => Some(b'0'),
            KeyCode::NUM1 => Some(b'1'),
            KeyCode::NUM2 => Some(b'2'),
            KeyCode::NUM3 => Some(b'3'),
            KeyCode::NUM4 => Some(b'4'),
            KeyCode::NUM5 => Some(b'5'),
            KeyCode::NUM6 => Some(b'6'),
            KeyCode::NUM7 => Some(b'7'),
            KeyCode::NUM8 => Some(b'8'),
            KeyCode::NUM9 => Some(b'9'),
            KeyCode::HASH if !self.numeric => Some(b'#'),
            _ => None,
        };
        if let Some(key) = typed {
            // A full field takes nothing more, though what is being composed
            // can still change.
            if self.text.len() < self.limit {
                self.feed(system, key as i8);
            }
            return Outcome::Editing;
        }

        match code {
            KeyCode::STAR if !self.numeric => {
                self.flush(system);
                self.mode = match self.mode {
                    KOREAN => LOWER,
                    LOWER => UPPER,
                    UPPER => NUMBER,
                    _ => KOREAN,
                };
                system.set_current_input_mode(self.mode);
            }
            KeyCode::LEFT | KeyCode::RIGHT | KeyCode::UP | KeyCode::DOWN => self.flush(system),
            KeyCode::CLEAR => {
                if !self.composing.is_empty() {
                    if self.mode == KOREAN {
                        self.feed(system, UNDO);
                    } else {
                        system.set_current_input_mode(self.mode);
                        self.composing.clear();
                    }
                } else if !self.text.is_empty() {
                    pop_char(&mut self.text);
                } else {
                    system.set_current_input_mode(self.mode);
                    return Outcome::Done(None);
                }
            }
            KeyCode::OK | KeyCode::LEFT_SOFT_KEY => {
                self.flush(system);
                return Outcome::Done(Some(core::mem::take(&mut self.text)));
            }
            _ => {}
        }
        Outcome::Editing
    }

    /// Gives the input method a key and takes what it finished and what it
    /// is composing.
    fn feed(&mut self, system: &System, key: i8) {
        let output = system.handle_input_method(key, 2);
        if output.output0_len > 0 {
            self.text.extend_from_slice(&output.output0[..output.output0_len]);
            truncate(&mut self.text, self.limit);
        }
        if output.handled || output.output0_len > 0 {
            self.composing = output.output1[..output.output1_len].to_vec();
        }
    }

    /// Finishes the character in progress.
    fn flush(&mut self, system: &System) {
        if !self.composing.is_empty() {
            self.feed(system, FLUSH);
            self.composing.clear();
        }
    }

    /// The editor, drawn over what the script last showed.
    pub fn draw(&self, graphics: &mut Graphics) {
        const FRAME: i32 = RAW_COLOR;
        const FACE: i32 = RAW_COLOR + 0x4e;
        const FIELD: i32 = RAW_COLOR + 0xff;
        const INK: i32 = RAW_COLOR;
        const LABEL: i32 = RAW_COLOR + 0xff;
        const CARET: i32 = RAW_COLOR + 0xe0;

        let saved = (
            core::mem::replace(&mut graphics.pixels, self.underneath.clone()),
            graphics.clip,
            graphics.bank,
            graphics.text_fg,
            graphics.text_bg,
            graphics.text_align,
        );
        graphics.clip = (0, 0, graphics.width, graphics.height);
        graphics.bank = 3;
        graphics.text_bg = crate::graphics::TRANSPARENT;
        graphics.text_align = 0;

        let line = TEXT_HEIGHT as i32 + 4;
        let (left, right) = (4, graphics.width - 5);
        let top = graphics.height / 2 - line * 2;
        let bottom = top + line * 4 + 6;
        graphics.fill_rect(left, top, right, bottom, FRAME);
        graphics.fill_rect(left + 2, top + 2, right - 2, bottom - 2, FACE);

        // The prompt, and the mode on the right.
        let label = match self.mode {
            KOREAN => "한글",
            LOWER => "abc",
            UPPER => "ABC",
            _ => "123",
        };
        graphics.text_fg = LABEL;
        graphics.text(left + 6, top + 5, &self.prompt, false);
        graphics.text_align = 2;
        graphics.text(right - 6, top + 5, label, false);
        graphics.text_align = 0;

        // The field: what is typed, the tail of it when it runs long, and the
        // character being composed after it.
        let field_top = top + line + 6;
        graphics.fill_rect(left + 6, field_top, right - 6, field_top + line, FIELD);
        let text = encoding_rs::EUC_KR.decode(&self.text).0.into_owned();
        let composing = encoding_rs::EUC_KR.decode(&self.composing).0.into_owned();
        let room = right - left - 18;
        let mut shown: String = text.clone();
        while !shown.is_empty() && text_width(&shown) + text_width(&composing) > room {
            shown.remove(0);
        }
        let x = left + 9;
        graphics.text_fg = INK;
        graphics.text(x, field_top + 2, &shown, false);
        let after = x + text_width(&shown);
        if composing.is_empty() {
            graphics.fill_rect(after, field_top + 2, after, field_top + line - 2, CARET);
        } else {
            graphics.text_fg = CARET;
            graphics.text(after, field_top + 2, &composing, false);
            let width = text_width(&composing);
            graphics.fill_rect(after, field_top + line - 2, after + width - 1, field_top + line - 2, CARET);
        }

        // What the keys do.
        let help = if self.numeric {
            "CLR:지우기 OK:확인"
        } else {
            "*:모드 CLR:지우기 OK:확인"
        };
        graphics.text_fg = LABEL;
        graphics.text_align = 1;
        graphics.text((left + right) / 2, field_top + line + 5, help, false);

        graphics.present();
        (
            graphics.pixels,
            graphics.clip,
            graphics.bank,
            graphics.text_fg,
            graphics.text_bg,
            graphics.text_align,
        ) = saved;
    }
}

/// Cuts `text` to at most `limit` bytes, never through a two-byte character.
fn truncate(text: &mut Vec<u8>, limit: usize) {
    let mut end = 0;
    while end < text.len() {
        let width = if text[end] >= 0x80 { 2 } else { 1 };
        if end + width > limit {
            break;
        }
        end += width;
    }
    text.truncate(end);
}

/// Takes the last character off `text`, one byte or a two-byte one.
fn pop_char(text: &mut Vec<u8>) {
    let mut start = 0;
    let mut last = 0;
    while start < text.len() {
        last = start;
        start += if text[start] >= 0x80 { 2 } else { 1 };
    }
    text.truncate(last);
}

#[cfg(test)]
mod tests {
    use super::{pop_char, truncate};

    #[test]
    fn two_byte_characters_are_kept_whole() {
        let mut text = encoding_rs::EUC_KR.encode("가나다").0.into_owned();
        truncate(&mut text, 5);
        assert_eq!(text, encoding_rs::EUC_KR.encode("가나").0.into_owned());

        let mut text = [b"a".as_slice(), &encoding_rs::EUC_KR.encode("가").0].concat();
        pop_char(&mut text);
        assert_eq!(text, b"a");
        pop_char(&mut text);
        assert!(text.is_empty());
    }
}
