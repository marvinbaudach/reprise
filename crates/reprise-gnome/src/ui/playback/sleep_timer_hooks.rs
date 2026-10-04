//! Narrow sleep-timer seam on `PlayerController`.
//!
//! The controller owner is intentionally untouched: a weak, main-thread
//! registry connects the window-owned session state to event handling.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_view::sleep_timer::{SleepAction, SleepItem, SleepTimer};

use super::external_media_state::{ExternalMedia, ExternalSession};
use super::player_controller::PlayerController;

thread_local! {
    static BINDINGS: RefCell<HashMap<usize, Weak<SleepTimerBinding>>> = RefCell::new(HashMap::new());
}

pub(in crate::ui) struct SleepTimerBinding {
    pub(in crate::ui) timer: Rc<RefCell<SleepTimer>>,
    retained_volume: Cell<Option<f64>>,
}

impl SleepTimerBinding {
    pub(in crate::ui) fn new(timer: Rc<RefCell<SleepTimer>>) -> Rc<Self> {
        Rc::new(Self {
            timer,
            retained_volume: Cell::new(None),
        })
    }
}

impl PlayerController {
    pub(in crate::ui) fn install_sleep_timer(self: &Rc<Self>, binding: &Rc<SleepTimerBinding>) {
        let key = Rc::as_ptr(self) as usize;
        BINDINGS.with(|bindings| {
            bindings.borrow_mut().insert(key, Rc::downgrade(binding));
        });
    }

    pub(in crate::ui) fn arm_sleep_timer_minutes(
        self: &Rc<Self>,
        binding: &SleepTimerBinding,
        now: Duration,
        minutes: u32,
    ) {
        self.restore_sleep_timer_volume(binding);
        binding.retained_volume.set(Some(self.volume.get()));
        binding.timer.borrow_mut().arm_minutes(now, minutes);
    }

    pub(in crate::ui) fn arm_sleep_timer_end_of_track(
        self: &Rc<Self>,
        binding: &SleepTimerBinding,
    ) -> bool {
        let Some(item) = self.current_sleep_item() else {
            return false;
        };
        self.restore_sleep_timer_volume(binding);
        binding.retained_volume.set(Some(self.volume.get()));
        binding.timer.borrow_mut().arm_end_of_track(item)
    }

    pub(in crate::ui) fn cancel_sleep_timer(&self, binding: &SleepTimerBinding) {
        binding.timer.borrow_mut().cancel();
        self.restore_sleep_timer_volume(binding);
    }

    pub(in crate::ui) fn sleep_timer_tick(
        self: &Rc<Self>,
        binding: &SleepTimerBinding,
        now: Duration,
    ) {
        let current_item = self.current_sleep_item();
        let armed_item = binding.timer.borrow().armed_item();
        if let (Some(armed), Some(current)) = (armed_item, current_item) {
            if armed != current {
                self.reset_sleep_timer_fade_volume(binding);
                binding.timer.borrow_mut().on_manual_change(current);
                if !binding.timer.borrow().is_armed() {
                    self.restore_sleep_timer_volume(binding);
                    self.sync_sleep_timer_button(binding, now);
                    return;
                }
            }
        }
        let (position_ms, duration_ms) = self.sleep_timer_position();
        let action = binding
            .timer
            .borrow_mut()
            .tick(now, position_ms, duration_ms);
        self.apply_sleep_timer_action(binding, action);
        self.sync_sleep_timer_button(binding, now);
    }

    pub(in crate::ui) fn sleep_timer_position_tick(self: &Rc<Self>) {
        if let Some(binding) = self.sleep_timer_binding() {
            self.sleep_timer_tick(&binding, monotonic_now());
        }
    }

    pub(in crate::ui) fn sync_sleep_timer_button(
        &self,
        binding: &SleepTimerBinding,
        now: Duration,
    ) {
        let timer = binding.timer.borrow();
        let (armed, tooltip) = if let Some(minutes) = timer.remaining_minutes(now) {
            (
                true,
                crate::i18n::format_message(
                    &crate::i18n::gettext("Pauses in {minutes} min"),
                    &[("minutes", &minutes.to_string())],
                ),
            )
        } else if timer.armed_item().is_some() {
            (true, crate::i18n::gettext("Pauses after this track"))
        } else {
            (false, crate::i18n::gettext("Sleep Timer"))
        };
        drop(timer);
        let end_enabled = self
            .current_sleep_item()
            .is_some_and(|item| !matches!(item, SleepItem::Radio(_)));
        self.bar
            .set_sleep_timer_presentation(armed, &tooltip, end_enabled);
    }

    pub(in crate::ui) fn sleep_timer_track_finished(self: &Rc<Self>) -> bool {
        self.finish_sleep_timer(SleepTimer::on_track_finished)
    }

    pub(in crate::ui) fn sleep_timer_gapless_advance(self: &Rc<Self>) -> bool {
        self.finish_sleep_timer(SleepTimer::on_gapless_advance)
    }

    fn finish_sleep_timer(
        self: &Rc<Self>,
        finish: impl FnOnce(&mut SleepTimer) -> SleepAction,
    ) -> bool {
        let Some(binding) = self.sleep_timer_binding() else {
            return false;
        };
        let action = {
            let mut timer = binding.timer.borrow_mut();
            finish(&mut timer)
        };
        let pauses = action == SleepAction::Pause;
        self.apply_sleep_timer_action(&binding, action);
        self.sync_sleep_timer_button(&binding, monotonic_now());
        pauses
    }

    fn sleep_timer_binding(&self) -> Option<Rc<SleepTimerBinding>> {
        let key = self as *const Self as usize;
        BINDINGS.with(|bindings| bindings.borrow().get(&key).and_then(Weak::upgrade))
    }

    fn apply_sleep_timer_action(&self, binding: &SleepTimerBinding, action: SleepAction) {
        match action {
            SleepAction::None => {}
            SleepAction::SetVolume(relative) => {
                let retained = binding.retained_volume.get().unwrap_or(self.volume.get());
                self.set_sleep_timer_volume(retained * relative);
            }
            SleepAction::Pause => {
                let status = self
                    .mpris_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .status;
                if status == MprisPlaybackStatus::Playing {
                    if let Err(error) = self.player.toggle_pause() {
                        tracing::error!(%error, "sleep timer could not pause playback");
                    }
                }
                self.restore_sleep_timer_volume(binding);
                self.show_toast(&crate::i18n::gettext("Paused by sleep timer"));
            }
        }
    }

    fn set_sleep_timer_volume(&self, volume: f64) {
        self.player.set_volume(volume);
        self.volume.set(volume);
        self.sync_volume_indicator(volume);
        self.update_mpris_volume(volume);
    }

    fn restore_sleep_timer_volume(&self, binding: &SleepTimerBinding) {
        if let Some(volume) = binding.retained_volume.take() {
            self.set_sleep_timer_volume(volume);
        }
    }

    fn reset_sleep_timer_fade_volume(&self, binding: &SleepTimerBinding) {
        if let Some(volume) = binding.retained_volume.get() {
            self.set_sleep_timer_volume(volume);
        }
    }

    fn current_sleep_item(&self) -> Option<SleepItem> {
        let external = self.external.borrow();
        match external.session.as_ref() {
            Some(ExternalSession::Podcast(session)) => match session.media {
                ExternalMedia::Podcast { episode_id, .. } => Some(SleepItem::Episode(episode_id)),
                ExternalMedia::Radio { .. } => None,
            },
            Some(ExternalSession::Radio(session)) => match session.media {
                ExternalMedia::Radio { station_id, .. } => Some(SleepItem::Radio(station_id)),
                ExternalMedia::Podcast { .. } => None,
            },
            None => self
                .current_track
                .get()
                .map(|(track_id, _)| SleepItem::Track(track_id)),
        }
    }

    fn sleep_timer_position(&self) -> (i64, i64) {
        let external = self.external.borrow();
        match external.session.as_ref() {
            Some(ExternalSession::Podcast(session)) => {
                let duration = match session.media {
                    ExternalMedia::Podcast { duration_ms, .. } => duration_ms.unwrap_or(0),
                    ExternalMedia::Radio { .. } => 0,
                };
                (session.position_ms, duration)
            }
            Some(ExternalSession::Radio(_)) => (0, 0),
            None => (
                self.max_position_ms.get(),
                self.current_track.get().map_or(0, |(_, duration)| duration),
            ),
        }
    }
}

pub(in crate::ui) fn monotonic_now() -> Duration {
    Duration::from_micros(u64::try_from(gtk4::glib::monotonic_time()).unwrap_or(0))
}
