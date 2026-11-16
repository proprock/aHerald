//! Decides what the resident card shows now. Pure: queue state and a caller-supplied
//! clock in, the visible state out, so the Win32 adapter only draws what changed.
use crate::queue::Entry;

/// Everything about the selected card that is visible and changes with time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub frame: usize,
    pub alpha: u8,
    /// Whole seconds left for a timed card; `None` for a persistent one.
    pub seconds: Option<u64>,
}

pub struct Input<'a> {
    pub entry: &'a Entry,
    pub durations: &'a [u64],
    pub looping: bool,
    /// Milliseconds since the animation started.
    pub animation_ms: u64,
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
}

impl Input<'_> {
    pub fn shown(&self) -> Shown {
        let display = self.entry.display_duration(self.fade_out_ms);
        Shown {
            frame: crate::animation::frame_index(self.durations, self.looping, self.animation_ms),
            alpha: crate::notification::presentation(
                self.entry.visible_ms,
                display,
                self.fade_in_ms,
                self.fade_out_ms,
                None,
            )
            .0,
            seconds: crate::card::remaining_seconds(display, self.entry.visible_ms),
        }
    }
}

/// Remembers what was last put on screen, so an unchanged card is not presented again.
#[derive(Default)]
pub struct Presenter {
    last: Option<Shown>,
}

impl Presenter {
    /// `Some` when the window must be redrawn: the frame, alpha or countdown second
    /// differs from what is on screen, or the screen state was invalidated.
    pub fn next(&mut self, input: &Input) -> Option<Shown> {
        let shown = input.shown();
        (self.last.replace(shown) != Some(shown)).then_some(shown)
    }

    /// A fully opaque single-frame card without countdown, such as the usage card:
    /// `Some` once, until the screen state is invalidated.
    pub fn next_static(&mut self) -> Option<Shown> {
        let shown = Shown {
            frame: 0,
            alpha: 255,
            seconds: None,
        };
        (self.last.replace(shown) != Some(shown)).then_some(shown)
    }

    /// Forget the screen state: the surface, placement or visibility changed.
    pub fn invalidate(&mut self) {
        self.last = None;
    }
}

/// Sounds that may play at once; further events wait unannounced in the queue.
pub const MAX_SOUNDS: usize = 4;

/// How many new sounds may start while `playing` are still running.
pub fn sound_slots(playing: usize) -> usize {
    MAX_SOUNDS.saturating_sub(playing)
}

/// The sounds one resident pass starts. The caller cancels what is playing first when
/// `cancel` is set, so every sound in `play` starts with the fresh cancel flag.
#[derive(Clone, Debug, Default)]
pub struct Announce {
    pub play: Vec<Entry>,
    pub cancel: bool,
}

/// Decides which sounds start in each pass of a resident, the same on every OS. It runs
/// before anything decides what to draw, so the usage card, a hidden window or switched
/// off pop-ups never hold a sound back.
#[derive(Debug, Default)]
pub struct Announcer {
    /// The Apply + Test sample that sounded last, so each sample sounds once per visit.
    sample: Option<u64>,
}

impl Announcer {
    /// `sample` is the Apply + Test card on screen, if any; `sounds` is the Sound switch;
    /// `playing` counts the sounds still running.
    ///
    /// Paused, nothing is taken, so new events and the sample sound when the pause ends.
    /// With sound off, new events are still consumed, so switching it on replays nothing.
    /// The sample silences what plays and ignores `MAX_SOUNDS`; queue events wait for a slot.
    pub fn pass(
        &mut self,
        queue: &mut crate::queue::Queue,
        sample: Option<&Entry>,
        paused: bool,
        sounds: bool,
        playing: usize,
    ) -> Announce {
        let mut announce = Announce::default();
        if paused {
            return announce;
        }
        if let Some(sample) = sample
            && self.sample != Some(sample.id)
        {
            self.sample = Some(sample.id);
            announce.cancel = true;
            announce.play.push(sample.clone());
        }
        let room = if sounds {
            sound_slots(playing)
        } else {
            usize::MAX
        };
        announce.play.extend(queue.take_unannounced(room));
        announce.play.retain(|entry| sounds && entry.outputs.sound);
        announce
    }
}
