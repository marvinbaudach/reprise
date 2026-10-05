//! Narrow sleep-timer seam on `PlayerController`.
//!
//! The controller owner is intentionally untouched: a weak, main-thread
//! registry connects the window-owned session state to event handling.

use std::cell::RefCell;
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
    fade: RefCell<FadeVolume>,
}

#[derive(Default)]
struct FadeVolume {
    retained: Option<f64>,
    last_applied: Option<f64>,
}

impl FadeVolume {
    fn apply(&mut self, current: f64, relative: f64) -> Option<f64> {
        if self.changed_by_user(current) {
            self.retained = Some(current);
            self.last_applied = None;
        }
        if relative >= 1.0 {
            return self.restore(current);
        }
        let retained = *self.retained.get_or_insert(current);
        let target = retained * relative;
        self.last_applied = Some(target);
        Some(target)
    }

    fn restore(&mut self, current: f64) -> Option<f64> {
        if self.changed_by_user(current) {
            self.retained = Some(current);
        }
        let retained = self.retained.take();
        self.last_applied = None;
        retained
    }

    #[cfg(test)]
    const fn retained(&self) -> Option<f64> {
        self.retained
    }

    fn changed_by_user(&self, current: f64) -> bool {
        self.last_applied
            .is_some_and(|applied| (current - applied).abs() > f64::EPSILON)
    }
}

impl SleepTimerBinding {
    pub(in crate::ui) fn new(timer: Rc<RefCell<SleepTimer>>) -> Rc<Self> {
        Rc::new(Self {
            timer,
            fade: RefCell::new(FadeVolume::default()),
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
        binding.timer.borrow_mut().arm_minutes(now, minutes);
    }

    pub(in crate::ui) fn arm_sleep_timer_end_of_track(
        self: &Rc<Self>,
        binding: &SleepTimerBinding,
    ) -> bool {
        let Some(item) = self.current_sleep_item() else {
            return false;
        };
        let armed = binding.timer.borrow_mut().arm_end_of_track(item);
        if armed {
            self.restore_sleep_timer_volume(binding);
        }
        armed
    }

    pub(in crate::ui) fn cancel_sleep_timer(&self, binding: &SleepTimerBinding) {
        binding.timer.borrow_mut().cancel();
        self.restore_sleep_timer_volume(binding);
    }

    pub(in crate::ui) fn sleep_timer_tick(
        self: &Rc<Self>,
        binding: &SleepTimerBinding,
        now: Duration,
        position_ms: i64,
        duration_ms: i64,
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
        let action = binding
            .timer
            .borrow_mut()
            .tick(now, position_ms, duration_ms);
        self.apply_sleep_timer_action(binding, action);
        self.sync_sleep_timer_button(binding, now);
    }

    pub(in crate::ui) fn sleep_timer_position_tick(
        self: &Rc<Self>,
        position_ms: i64,
        duration_ms: i64,
    ) {
        if let Some(binding) = self.sleep_timer_binding() {
            self.sleep_timer_tick(&binding, monotonic_now(), position_ms, duration_ms);
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
                crate::ui::strings::sleep_timer_pauses_in(minutes as usize),
            )
        } else if timer.armed_item().is_some() {
            (
                true,
                crate::ui::strings::text(crate::ui::strings::PAUSES_AFTER_THIS_TRACK),
            )
        } else {
            (
                false,
                crate::ui::strings::text(crate::ui::strings::SLEEP_TIMER),
            )
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

    pub(in crate::ui) fn sleep_timer_arms_finished_external(&self) -> bool {
        let Some(binding) = self.sleep_timer_binding() else {
            return false;
        };
        let armed_item = binding.timer.borrow().armed_item();
        armed_item == self.current_sleep_item()
    }

    pub(in crate::ui) fn finish_sleep_timer_after_external_completion(self: &Rc<Self>) {
        let Some(binding) = self.sleep_timer_binding() else {
            return;
        };
        let action = binding.timer.borrow_mut().on_track_finished();
        if action != SleepAction::Pause {
            return;
        }
        self.restore_sleep_timer_volume(&binding);
        self.sync_sleep_timer_button(&binding, monotonic_now());
        self.show_toast(&crate::ui::strings::sleep_timer_paused());
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

    fn apply_sleep_timer_action(self: &Rc<Self>, binding: &SleepTimerBinding, action: SleepAction) {
        match action {
            SleepAction::None => {}
            SleepAction::SetVolume(relative) => {
                let target = binding.fade.borrow_mut().apply(self.volume.get(), relative);
                if let Some(target) = target {
                    self.set_sleep_timer_volume(target);
                }
            }
            SleepAction::Pause => {
                let paused = self.pause_for_sleep_timer();
                self.restore_sleep_timer_volume(binding);
                if paused {
                    self.show_toast(&crate::ui::strings::sleep_timer_paused());
                }
            }
        }
    }

    fn pause_for_sleep_timer(self: &Rc<Self>) -> bool {
        let status = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status;
        if status != MprisPlaybackStatus::Playing {
            return false;
        }
        if self.toggle_external_pause() {
            return self
                .mpris_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .status
                == MprisPlaybackStatus::Paused;
        }
        match self.player.toggle_pause() {
            Ok(reprise_core::playback::PlaybackState::Paused) => {
                self.sync_state(reprise_core::playback::PlaybackState::Paused);
                self.update_mpris_mirror(MprisPlaybackStatus::Paused);
                true
            }
            Ok(_) => false,
            Err(error) => {
                tracing::error!(%error, "sleep timer could not pause playback");
                false
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
        if let Some(volume) = binding.fade.borrow_mut().restore(self.volume.get()) {
            self.set_sleep_timer_volume(volume);
        }
    }

    fn reset_sleep_timer_fade_volume(&self, binding: &SleepTimerBinding) {
        if let Some(volume) = binding.fade.borrow_mut().restore(self.volume.get()) {
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
}

pub(in crate::ui) fn monotonic_now() -> Duration {
    Duration::from_micros(u64::try_from(gtk4::glib::monotonic_time()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::FadeVolume;

    #[test]
    fn play_18_fade_captures_volume_only_when_it_first_moves() {
        let mut fade = FadeVolume::default();

        assert_eq!(fade.apply(0.8, 0.5), Some(0.4));
        assert_eq!(fade.retained(), Some(0.8));
    }

    #[test]
    fn play_18_user_volume_change_during_fade_rebases_the_fade() {
        let mut fade = FadeVolume::default();
        assert_eq!(fade.apply(0.8, 0.5), Some(0.4));

        let rebased = fade.apply(0.2, 0.375).unwrap();
        assert!((rebased - 0.075).abs() < f64::EPSILON);
        assert_eq!(fade.retained(), Some(0.2));
        assert_eq!(fade.restore(0.075), Some(0.2));
    }

    #[test]
    fn play_18_cancel_restores_only_after_a_fade_started() {
        let mut untouched = FadeVolume::default();
        assert_eq!(untouched.restore(0.4), None);

        let mut faded = FadeVolume::default();
        assert_eq!(faded.apply(0.6, 0.875), Some(0.525));
        assert_eq!(faded.restore(0.525), Some(0.6));
    }
}
