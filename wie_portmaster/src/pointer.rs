//! The mouse, on the list and in the settings.
//!
//! Every screen is drawn whole on a canvas of the settings' size and shown
//! scaled into the window (see `App::show`), so the pointer is put back into
//! that canvas's pixels before anything looks at it. A screen says where its
//! rows and controls are as it draws them - a [`Hit`] each - and the pointer
//! is matched against those: over a row it moves the cursor there, a click
//! picks it, the right button goes back and the wheel moves up and down. The
//! screens themselves only ever see the cursor move and the buttons they
//! already answer to, so the mouse needs nothing of them but the hits.

use std::cell::RefCell;

use crate::{App, controls::Button};

/// What the mouse did, in the pixels of the canvas last shown.
pub enum Mouse {
    /// The pointer moved here; whether the left button is held.
    Move(i32, i32, bool),
    /// The left button went down here.
    Press(i32, i32),
    /// The left button came up.
    Release,
    /// The right button: back.
    Back,
    /// The wheel turned, up positive.
    Wheel(i32),
}

/// What a part of a screen is to the mouse.
#[derive(Clone, Copy)]
pub enum Target {
    /// A row of a list: the cursor goes there, a click picks it.
    Row(usize),
    /// One cell of a row in a table, by row and column.
    Cell(usize, usize),
    /// The ◀ or ▶ beside a row's value: forward when it is ▶.
    Step(usize, bool),
    /// The speed ruler under a row, from `x`, `width` pixels wide.
    Ruler(usize, i32, i32),
    /// Clicking it is pressing this button.
    Press(Button),
}

pub struct Hit {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    target: Target,
}

/// Where the canvas last shown sits in the window's pixels, to put the
/// pointer back into its own.
#[derive(Clone, Copy, Default)]
pub struct Shown {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub shown_width: i32,
    pub shown_height: i32,
}

#[derive(Default)]
pub struct Pointer {
    /// The parts of the screen on show, the last drawn on top.
    hits: RefCell<Vec<Hit>>,
    pub shown: Shown,
    /// The row - and the column, in a table - the pointer last went to.
    pointed: Option<(usize, Option<usize>)>,
    /// The speed the ruler was dragged or clicked to, in tenths.
    ruler: Option<i32>,
    /// The ruler being dragged: where it is, to follow the pointer along it
    /// even off its height.
    dragging: Option<(i32, i32)>,
}

impl Shown {
    /// A point in the window's pixels as one in the canvas's.
    pub fn to_canvas(self, x: i32, y: i32) -> (i32, i32) {
        if self.shown_width <= 0 || self.shown_height <= 0 {
            return (-1, -1);
        }
        (
            ((x - self.x) as i64 * self.width as i64 / self.shown_width as i64) as i32,
            ((y - self.y) as i64 * self.height as i64 / self.shown_height as i64) as i32,
        )
    }
}

/// The speed, in tenths, at `x` along a ruler from `left`, `width` wide.
pub fn ruler_tenths(x: i32, left: i32, width: i32) -> i32 {
    let span = (crate::settings::SPEED_MAX - crate::settings::SPEED_MIN) as i32;
    let along = (x - left).clamp(0, width.max(1));
    crate::settings::SPEED_MIN as i32 + ((along * span) as f32 / width.max(1) as f32).round() as i32
}

impl App {
    /// Forgets the screen's parts, for a screen about to be drawn anew.
    pub(crate) fn clear_hits(&self) {
        self.pointer.hits.borrow_mut().clear();
    }

    /// Marks a part of the screen being drawn.
    pub(crate) fn hit(&self, x: i32, y: i32, width: i32, height: i32, target: Target) {
        self.pointer.hits.borrow_mut().push(Hit { x, y, width, height, target });
    }

    /// The row - and column - the pointer moved the cursor to since the last
    /// call, if it did.
    pub(crate) fn take_pointed(&mut self) -> Option<(usize, Option<usize>)> {
        self.pointer.pointed.take()
    }

    /// The speed the ruler was set to since the last call, in tenths.
    pub(crate) fn take_ruler(&mut self) -> Option<i32> {
        self.pointer.ruler.take()
    }

    fn target_at(&self, x: i32, y: i32) -> Option<Target> {
        self.pointer
            .hits
            .borrow()
            .iter()
            .rev()
            .find(|hit| x >= hit.x && x < hit.x + hit.width && y >= hit.y && y < hit.y + hit.height)
            .map(|hit| hit.target)
    }

    /// What the mouse did, as the buttons it stands for, the cursor moved
    /// and the ruler set.
    pub(crate) fn mouse_presses(&mut self, mouse: Mouse, pressed: &mut Vec<Button>) {
        match mouse {
            Mouse::Move(x, y, held) => {
                if let (true, Some((left, width))) = (held, self.pointer.dragging) {
                    self.pointer.ruler = Some(ruler_tenths(x, left, width));
                    return;
                }
                match self.target_at(x, y) {
                    Some(Target::Row(row) | Target::Step(row, _) | Target::Ruler(row, ..)) => self.pointer.pointed = Some((row, None)),
                    Some(Target::Cell(row, column)) => self.pointer.pointed = Some((row, Some(column))),
                    Some(Target::Press(_)) | None => {}
                }
            }
            Mouse::Press(x, y) => match self.target_at(x, y) {
                Some(Target::Row(row)) => {
                    self.pointer.pointed = Some((row, None));
                    pressed.push(Button::A);
                }
                Some(Target::Cell(row, column)) => {
                    self.pointer.pointed = Some((row, Some(column)));
                    pressed.push(Button::A);
                }
                Some(Target::Step(row, forward)) => {
                    self.pointer.pointed = Some((row, None));
                    pressed.push(if forward { Button::Right } else { Button::Left });
                }
                Some(Target::Ruler(row, left, width)) => {
                    self.pointer.pointed = Some((row, None));
                    self.pointer.ruler = Some(ruler_tenths(x, left, width));
                    self.pointer.dragging = Some((left, width));
                }
                Some(Target::Press(button)) => pressed.push(button),
                None => {}
            },
            Mouse::Release => self.pointer.dragging = None,
            Mouse::Back => pressed.push(Button::B),
            Mouse::Wheel(turn) => {
                let button = if turn > 0 { Button::Up } else { Button::Down };
                for _ in 0..turn.unsigned_abs().min(3) {
                    pressed.push(button);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Shown, ruler_tenths};

    #[test]
    fn the_pointer_is_put_back_into_the_canvas() {
        // A 320x240 canvas shown at 3x, 40 pixels in from the left.
        let shown = Shown {
            width: 320,
            height: 240,
            x: 40,
            y: 0,
            shown_width: 960,
            shown_height: 720,
        };
        assert_eq!(shown.to_canvas(40, 0), (0, 0));
        assert_eq!(shown.to_canvas(40 + 300, 150), (100, 50));
    }

    #[test]
    fn the_ruler_reads_tenths_along_its_length() {
        // 0.1x at the left end, 4x at the right, 1x a tenth of the way less
        // than a quarter along.
        assert_eq!(ruler_tenths(10, 10, 390), 1);
        assert_eq!(ruler_tenths(400, 10, 390), 40);
        assert_eq!(ruler_tenths(10 + 90, 10, 390), 10);
        // Off either end, the end.
        assert_eq!(ruler_tenths(-50, 10, 390), 1);
        assert_eq!(ruler_tenths(900, 10, 390), 40);
    }
}
