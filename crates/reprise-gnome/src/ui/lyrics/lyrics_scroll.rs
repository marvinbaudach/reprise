//! User-pause state and injectable timing for synchronized-lyrics scrolling.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub(in crate::ui) const USER_PAUSE_MS: u64 = 4_000;
const MIN_CONTENT_MARGIN: i32 = 18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum ScrollMode {
    Auto,
    UserPause,
    Returning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct PauseHandle {
    generation: u64,
}

#[derive(Debug)]
pub(in crate::ui) struct LyricsScrollState {
    mode: ScrollMode,
    generation: u64,
    resume_at_ms: u64,
}

impl Default for LyricsScrollState {
    fn default() -> Self {
        Self {
            mode: ScrollMode::Auto,
            generation: 0,
            resume_at_ms: 0,
        }
    }
}

impl LyricsScrollState {
    #[cfg(test)]
    pub(in crate::ui) fn mode(&self) -> ScrollMode {
        self.mode
    }

    pub(in crate::ui) fn user_scroll(&mut self, now_ms: u64) -> PauseHandle {
        self.generation = self.generation.wrapping_add(1);
        self.resume_at_ms = now_ms.saturating_add(USER_PAUSE_MS);
        self.mode = ScrollMode::UserPause;
        PauseHandle {
            generation: self.generation,
        }
    }

    pub(in crate::ui) fn timer_elapsed(&mut self, handle: PauseHandle, now_ms: u64) -> bool {
        if self.mode != ScrollMode::UserPause
            || handle.generation != self.generation
            || now_ms < self.resume_at_ms
        {
            return false;
        }
        self.mode = ScrollMode::Returning;
        true
    }

    pub(in crate::ui) fn remaining_pause_ms(&self, now_ms: u64) -> u64 {
        self.resume_at_ms.saturating_sub(now_ms)
    }

    pub(in crate::ui) fn should_follow_active_line(&self) -> bool {
        self.mode != ScrollMode::UserPause
    }

    pub(in crate::ui) fn external_seek(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.mode = ScrollMode::Auto;
    }

    pub(in crate::ui) fn return_finished(&mut self) {
        if self.mode == ScrollMode::Returning {
            self.mode = ScrollMode::Auto;
        }
    }
}

pub(in crate::ui) fn centered_scroll_value(
    row_y: f64,
    row_height: f64,
    page_size: f64,
    upper: f64,
) -> f64 {
    if !row_y.is_finite() || !row_height.is_finite() || !page_size.is_finite() || !upper.is_finite()
    {
        return 0.0;
    }
    let maximum = (upper - page_size).max(0.0);
    (row_y + row_height / 2.0 - page_size / 2.0).clamp(0.0, maximum)
}

/// Keeps real leading context honest: the top edge has only its normal inset,
/// while the trailing inset still lets the final line reach the centre.
pub(in crate::ui) fn content_margins(viewport_height: i32, row_height: i32) -> (i32, i32) {
    let trailing = ((viewport_height - row_height) / 2).max(MIN_CONTENT_MARGIN);
    (MIN_CONTENT_MARGIN, trailing)
}

pub(in crate::ui) trait ScrollTimerHandle {
    fn cancel(&self);
}

pub(in crate::ui) trait ScrollTimer {
    fn now_ms(&self) -> u64;
    fn schedule(&self, delay_ms: u64, callback: Box<dyn FnOnce()>) -> Box<dyn ScrollTimerHandle>;
}

pub(in crate::ui) struct GlibScrollTimer;

struct GlibTimerHandle {
    source: Rc<RefCell<Option<gtk4::glib::SourceId>>>,
}

impl ScrollTimerHandle for GlibTimerHandle {
    fn cancel(&self) {
        if let Some(source) = self.source.borrow_mut().take() {
            source.remove();
        }
    }
}

impl ScrollTimer for GlibScrollTimer {
    fn now_ms(&self) -> u64 {
        u64::try_from(gtk4::glib::monotonic_time()).unwrap_or(0) / 1_000
    }

    fn schedule(&self, delay_ms: u64, callback: Box<dyn FnOnce()>) -> Box<dyn ScrollTimerHandle> {
        let callback = Rc::new(RefCell::new(Some(callback)));
        let source = Rc::new(RefCell::new(None));
        let callback_for_timer = callback.clone();
        let source_for_timer = source.clone();
        let source_id =
            gtk4::glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
                source_for_timer.borrow_mut().take();
                let callback = callback_for_timer.borrow_mut().take();
                if let Some(callback) = callback {
                    callback();
                }
            });
        *source.borrow_mut() = Some(source_id);
        Box::new(GlibTimerHandle { source })
    }
}

#[cfg(test)]
pub(in crate::ui) struct ManualScrollTimer {
    now_ms: std::cell::Cell<u64>,
    entries: RefCell<Vec<ManualTimerEntry>>,
}

#[cfg(test)]
struct ManualTimerEntry {
    deadline_ms: u64,
    cancelled: Rc<std::cell::Cell<bool>>,
    callback: Option<Box<dyn FnOnce()>>,
}

#[cfg(test)]
struct ManualTimerHandle {
    cancelled: Rc<std::cell::Cell<bool>>,
}

#[cfg(test)]
impl ScrollTimerHandle for ManualTimerHandle {
    fn cancel(&self) {
        self.cancelled.set(true);
    }
}

#[cfg(test)]
impl ManualScrollTimer {
    pub(in crate::ui) fn new() -> Rc<Self> {
        Rc::new(Self {
            now_ms: std::cell::Cell::new(0),
            entries: RefCell::new(Vec::new()),
        })
    }

    pub(in crate::ui) fn advance(&self, elapsed_ms: u64) {
        self.now_ms
            .set(self.now_ms.get().saturating_add(elapsed_ms));
        loop {
            let entry = {
                let mut entries = self.entries.borrow_mut();
                let next = entries.iter().position(|entry| {
                    !entry.cancelled.get() && entry.deadline_ms <= self.now_ms.get()
                });
                next.map(|index| entries.remove(index))
            };
            let Some(mut entry) = entry else {
                break;
            };
            if !entry.cancelled.get() {
                if let Some(callback) = entry.callback.take() {
                    callback();
                }
            }
        }
    }

    pub(in crate::ui) fn active_timer_count(&self) -> usize {
        self.entries
            .borrow()
            .iter()
            .filter(|entry| !entry.cancelled.get())
            .count()
    }
}

#[cfg(test)]
impl ScrollTimer for ManualScrollTimer {
    fn now_ms(&self) -> u64 {
        self.now_ms.get()
    }

    fn schedule(&self, delay_ms: u64, callback: Box<dyn FnOnce()>) -> Box<dyn ScrollTimerHandle> {
        let cancelled = Rc::new(std::cell::Cell::new(false));
        self.entries.borrow_mut().push(ManualTimerEntry {
            deadline_ms: self.now_ms.get().saturating_add(delay_ms),
            cancelled: cancelled.clone(),
            callback: Some(callback),
        });
        Box::new(ManualTimerHandle { cancelled })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_pause_handles_cannot_end_a_newer_user_pause() {
        let mut state = LyricsScrollState::default();
        let first = state.user_scroll(0);
        let second = state.user_scroll(2_000);

        assert!(!state.timer_elapsed(first, 4_000));
        assert_eq!(state.mode(), ScrollMode::UserPause);
        assert!(!state.timer_elapsed(second, 5_999));
        assert!(state.timer_elapsed(second, 6_000));
        assert_eq!(state.mode(), ScrollMode::Returning);
    }

    #[test]
    fn external_seek_cancels_pause_without_waiting_for_the_timer() {
        let mut state = LyricsScrollState::default();
        state.user_scroll(0);
        assert!(!state.should_follow_active_line());

        state.external_seek();
        assert_eq!(state.mode(), ScrollMode::Auto);
        assert!(state.should_follow_active_line());
    }
}
