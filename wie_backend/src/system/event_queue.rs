use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};
use core::{
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use wie_util::Result;

use crate::Instant;

#[allow(clippy::upper_case_acronyms, non_camel_case_types)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum KeyCode {
    UP,
    DOWN,
    LEFT,
    RIGHT,
    OK,
    LEFT_SOFT_KEY,
    RIGHT_SOFT_KEY,
    CLEAR,
    CALL,
    HANGUP,
    VOLUME_UP,
    VOLUME_DOWN,

    NUM0,
    NUM1,
    NUM2,
    NUM3,
    NUM4,
    NUM5,
    NUM6,
    NUM7,
    NUM8,
    NUM9,
    HASH,
    STAR,
}

impl KeyCode {
    // TODO we can use libraries like strum
    pub fn parse(string: &str) -> KeyCode {
        match string {
            "UP" => KeyCode::UP,
            "DOWN" => KeyCode::DOWN,
            "LEFT" => KeyCode::LEFT,
            "RIGHT" => KeyCode::RIGHT,
            "OK" => KeyCode::OK,
            "0" => KeyCode::NUM0,
            "1" => KeyCode::NUM1,
            "2" => KeyCode::NUM2,
            "3" => KeyCode::NUM3,
            "4" => KeyCode::NUM4,
            "5" => KeyCode::NUM5,
            "6" => KeyCode::NUM6,
            "7" => KeyCode::NUM7,
            "8" => KeyCode::NUM8,
            "9" => KeyCode::NUM9,
            "#" => KeyCode::HASH,
            "*" => KeyCode::STAR,
            "CLR" => KeyCode::CLEAR,
            _ => unimplemented!("Unknown key: {string}"),
        }
    }
}

type TimerCallback = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

/// What a finger did on the screen.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PointerKind {
    Pressed,
    Released,
    Dragged,
}

impl PointerKind {
    /// The `org.kwis.msp.lcdui.EventQueue` `POINT_*` value for this, which is
    /// what a WIPI card's `pointerNotify(type, x, y)` is handed.
    ///
    /// Read off the LGT firmware's own `EventQueue`, whose static finals share
    /// one constant slot per distinct value: `POINT_PRESSED` shares `UP`'s and
    /// `KEY_PRESSED`'s (1), `POINT_RELEASED` `LEFT`'s and `KEY_RELEASED`'s (2),
    /// and `POINT_DRAGGED` `RIGHT`'s - the MIDP game action, 5. Its
    /// `dispatchEvent` passes the type through to `Display.pointerNotify`
    /// untouched.
    pub fn wipi_type(self) -> i32 {
        match self {
            Self::Pressed => 1,
            Self::Released => 2,
            Self::Dragged => 5,
        }
    }

    pub fn from_wipi_type(value: i32) -> Option<Self> {
        Some(match value {
            1 => Self::Pressed,
            2 => Self::Released,
            5 => Self::Dragged,
            _ => return None,
        })
    }
}

/// Whether touches on the screen reach the title, which the player turns on
/// for a title made for a touch handset. Off, a title is told the handset has
/// no touch screen and hears none, as on the keypad handsets most were made
/// for - a title that sees a touch screen may lay itself out for one.
///
/// Held here rather than per emulator so the host can flip it while a title
/// runs, from its UI thread.
static TOUCH_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_touch_enabled(enabled: bool) {
    TOUCH_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn touch_enabled() -> bool {
    TOUCH_ENABLED.load(Ordering::Relaxed)
}

/// How many frames have been put on the screen, counted by
/// [`crate::screen::present`], so a key let go can be held until the title
/// has drawn a frame with it down - see [`EventQueue::pop_at`].
static FRAMES_PRESENTED: AtomicU64 = AtomicU64::new(0);

pub(crate) fn note_frame_presented() {
    FRAMES_PRESENTED.fetch_add(1, Ordering::Relaxed);
}

/// The longest a release waits for that frame, so a title that draws nothing
/// while a key is down still hears it let go.
const RELEASE_HOLD_MAX_MS: u64 = 250;

pub enum Event {
    Redraw,
    Keydown(KeyCode),
    Keyup(KeyCode),
    Keyrepeat(KeyCode),
    /// A touch at `x`, `y` on the frame the host was shown.
    Pointer {
        kind: PointerKind,
        x: i32,
        y: i32,
    },
    Timer {
        id: u32,
        generation: u64,
        due: Instant,
        callback: TimerCallback,
    },
    Notify {
        r#type: i32,
        param1: i32,
        param2: i32,
    }, // wipi notifyEvent
}

impl Event {
    fn timer<F, Fut>(id: u32, generation: u64, due: Instant, callback: F) -> Self
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        Event::Timer {
            id,
            generation,
            due,
            callback: Box::new(move || Box::pin(callback())),
        }
    }
}

#[derive(Default)]
pub struct EventQueue {
    events: VecDeque<Event>,
    timer_generations: BTreeMap<u32, u64>,
    next_timer_generation: u64,
    /// Each key handed out pressed and not yet released: the frame count and
    /// the time it went down at.
    held: Vec<(KeyCode, u64, Instant)>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `event` behind everything already waiting.
    ///
    /// A redraw replaces one still waiting rather than queueing beside it. A
    /// paint draws the whole screen as it is when it runs, so two waiting say
    /// nothing one does not, and a host asks for one whenever the title
    /// repaints, which can be more often than the title serves them: a title
    /// that paints once per 100ms frame from its own loop - 사고뭉치트윈즈 -
    /// served one a frame while a backlog of four stood in front of every key,
    /// and each press reached it 400ms late. The one kept is the newest, at
    /// the back, so a repaint asked for after a key still paints after it.
    pub fn push(&mut self, event: Event) {
        if matches!(event, Event::Redraw) {
            self.events.retain(|x| !matches!(x, Event::Redraw));
        }

        // A finger moving reports far more often than a title reads; only
        // where it is now matters, so a drag still waiting is moved rather
        // than queued behind.
        if let Event::Pointer {
            kind: PointerKind::Dragged, ..
        } = event
            && let Some(Event::Pointer {
                kind: PointerKind::Dragged, ..
            }) = self.events.back()
        {
            self.events.pop_back();
        }

        self.events.push_back(event);
    }

    pub fn pop(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// The next event at `now`, a key's release held back until the title has
    /// drawn a frame since that key went down, or `RELEASE_HOLD_MAX_MS` has
    /// passed.
    ///
    /// A title that keeps a key's state from its events and reads it once a
    /// frame sees nothing of a tap that goes down and up between two frames.
    /// A handset's frames came quicker than a thumb; here they can come
    /// slower. 일지매영웅전기2 draws its menus at seven frames a second, 140ms
    /// apart, against a tap of 65 to 90ms, and its 장비 and 스탯 keys took one
    /// press in two. Everything else behind a held release goes on, the
    /// timers that draw that frame among them; only what is for the same key
    /// waits with it, so the key's own events keep their order.
    pub fn pop_at(&mut self, now: Instant) -> Option<Event> {
        let frames = FRAMES_PRESENTED.load(Ordering::Relaxed);
        let mut waiting: Vec<KeyCode> = Vec::new();

        let index = self.events.iter().position(|event| {
            let key = match event {
                Event::Keydown(key) | Event::Keyup(key) | Event::Keyrepeat(key) => *key,
                _ => return true,
            };
            if waiting.contains(&key) {
                return false;
            }
            if let Event::Keyup(_) = event
                && let Some(&(_, down_frames, down_at)) = self.held.iter().find(|x| x.0 == key)
                && down_frames == frames
                && now.raw().saturating_sub(down_at.raw()) < RELEASE_HOLD_MAX_MS
            {
                waiting.push(key);
                return false;
            }
            true
        })?;

        let event = self.events.remove(index)?;
        match &event {
            Event::Keydown(key) => {
                self.held.retain(|x| x.0 != *key);
                self.held.push((*key, frames, now));
            }
            Event::Keyup(key) => self.held.retain(|x| x.0 != *key),
            _ => {}
        }
        Some(event)
    }

    pub fn push_timer<F, Fut>(&mut self, id: u32, due: Instant, callback: F)
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.cancel_timer(id);

        self.next_timer_generation = self.next_timer_generation.wrapping_add(1);
        if self.next_timer_generation == 0 {
            self.next_timer_generation = 1;
        }

        let generation = self.next_timer_generation;
        self.timer_generations.insert(id, generation);
        self.events.push_back(Event::timer(id, generation, due, callback));
    }

    pub fn cancel_timer(&mut self, id: u32) {
        self.timer_generations.remove(&id);
        self.events
            .retain(|event| !matches!(event, Event::Timer { id: event_id, .. } if *event_id == id));
    }

    pub fn has_timer(&self, id: u32) -> bool {
        self.timer_generations.contains_key(&id)
    }

    pub fn is_timer_current(&self, id: u32, generation: u64) -> bool {
        self.timer_generations.get(&id).copied() == Some(generation)
    }

    pub fn take_timer(&mut self, id: u32, generation: u64) -> bool {
        if !self.is_timer_current(id, generation) {
            return false;
        }

        self.timer_generations.remove(&id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, EventQueue};
    use crate::Instant;

    fn timer_identity(event: &Event) -> (u32, u64) {
        match event {
            Event::Timer { id, generation, .. } => (*id, *generation),
            _ => panic!("expected timer event"),
        }
    }

    /// A tap that goes down and up before the title draws a frame is let go
    /// only once it has drawn one - or after the longest hold - and what is
    /// behind the release, a timer that draws that frame, goes first.
    #[test]
    fn a_release_waits_for_a_frame_drawn_with_the_key_down() {
        use super::{KeyCode, note_frame_presented};

        let at = Instant::from_epoch_millis;
        let mut queue = EventQueue::new();
        queue.push(Event::Keydown(KeyCode::NUM1));
        queue.push(Event::Keyup(KeyCode::NUM1));
        queue.push(Event::Keydown(KeyCode::NUM1));
        queue.push_timer(7, at(1000), || async { Ok(()) });

        assert!(matches!(queue.pop_at(at(1000)), Some(Event::Keydown(KeyCode::NUM1))));
        // The release, and the second press behind it, wait; the timer does not.
        assert_eq!(timer_identity(&queue.pop_at(at(1010)).unwrap()).0, 7);
        assert!(queue.pop_at(at(1020)).is_none());

        note_frame_presented();
        assert!(matches!(queue.pop_at(at(1030)), Some(Event::Keyup(KeyCode::NUM1))));
        assert!(matches!(queue.pop_at(at(1030)), Some(Event::Keydown(KeyCode::NUM1))));

        // No frame at all: let go after the longest hold.
        queue.push(Event::Keyup(KeyCode::NUM1));
        assert!(queue.pop_at(at(1100)).is_none());
        assert!(matches!(queue.pop_at(at(1300)), Some(Event::Keyup(KeyCode::NUM1))));
    }

    /// A redraw asked for while one is waiting leaves one, behind the input
    /// that came between them.
    #[test]
    fn a_waiting_redraw_is_replaced_rather_than_queued_beside() {
        use super::KeyCode;

        let mut queue = EventQueue::new();
        queue.push(Event::Redraw);
        queue.push(Event::Keydown(KeyCode::OK));
        queue.push(Event::Redraw);
        queue.push(Event::Redraw);

        assert!(matches!(queue.pop(), Some(Event::Keydown(KeyCode::OK))));
        assert!(matches!(queue.pop(), Some(Event::Redraw)));
        assert!(queue.pop().is_none());
    }

    /// A drag still waiting is moved to where the finger is now, and a press
    /// or release is never folded into one - nor is a drag behind a press.
    #[test]
    fn a_waiting_drag_is_moved_rather_than_queued_behind() {
        use super::PointerKind;

        let point = |event: Option<Event>| match event {
            Some(Event::Pointer { kind, x, y }) => (kind, x, y),
            _ => panic!("expected a pointer event"),
        };

        let mut queue = EventQueue::new();
        queue.push(Event::Pointer {
            kind: PointerKind::Pressed,
            x: 1,
            y: 1,
        });
        for step in 2..6 {
            queue.push(Event::Pointer {
                kind: PointerKind::Dragged,
                x: step,
                y: step,
            });
        }
        queue.push(Event::Pointer {
            kind: PointerKind::Released,
            x: 5,
            y: 5,
        });

        assert_eq!(point(queue.pop()), (PointerKind::Pressed, 1, 1));
        assert_eq!(point(queue.pop()), (PointerKind::Dragged, 5, 5));
        assert_eq!(point(queue.pop()), (PointerKind::Released, 5, 5));
        assert!(queue.pop().is_none());
    }

    /// The `POINT_*` values a card is handed read back as what they were.
    #[test]
    fn pointer_kinds_round_trip_through_their_wipi_types() {
        use super::PointerKind;

        for kind in [PointerKind::Pressed, PointerKind::Released, PointerKind::Dragged] {
            assert_eq!(PointerKind::from_wipi_type(kind.wipi_type()), Some(kind));
        }
        assert_eq!(PointerKind::from_wipi_type(3), None);
    }

    #[test]
    fn timer_generation_invalidates_cancelled_and_replaced_events() {
        let mut queue = EventQueue::new();

        queue.push_timer(7, Instant::from_epoch_millis(10), || async { Ok(()) });
        let first = queue.pop().expect("first timer");
        let (first_id, first_generation) = timer_identity(&first);

        assert!(queue.is_timer_current(first_id, first_generation));

        queue.cancel_timer(first_id);
        assert!(!queue.is_timer_current(first_id, first_generation));
        assert!(!queue.take_timer(first_id, first_generation));

        queue.push_timer(7, Instant::from_epoch_millis(20), || async { Ok(()) });
        let second = queue.pop().expect("second timer");
        let (second_id, second_generation) = timer_identity(&second);

        queue.push_timer(7, Instant::from_epoch_millis(30), || async { Ok(()) });
        let third = queue.pop().expect("third timer");
        let (third_id, third_generation) = timer_identity(&third);

        assert_eq!(second_id, third_id);
        assert_ne!(second_generation, third_generation);
        assert!(!queue.is_timer_current(second_id, second_generation));
        assert!(!queue.take_timer(second_id, second_generation));

        assert!(queue.is_timer_current(third_id, third_generation));
        assert!(queue.take_timer(third_id, third_generation));
        assert!(!queue.is_timer_current(third_id, third_generation));
        assert!(!queue.take_timer(third_id, third_generation));
    }
}
