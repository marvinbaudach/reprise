//! Session-only sleep-timer state shared by native frontends.

use std::time::Duration;

const FADE_DURATION_MS: u64 = 4_000;
const FADE_STEPS: u8 = 8;

/// Stable identity of the item the end-of-track arm follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepItem {
    Track(i64),
    Episode(i64),
    Radio(i64),
}

/// One effect for the platform adapter to apply.
///
/// `SetVolume` is relative to the volume retained when the timer was armed.
/// The adapter restores that retained volume immediately after `Pause`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SleepAction {
    None,
    SetVolume(f64),
    Pause,
}

/// Pure, session-only timer state. Timestamps are monotonic durations supplied
/// by the frontend, so tests and non-GLib frontends can inject their own clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepTimer {
    Off,
    Minutes { deadline: Duration, fade_step: u8 },
    EndOfTrack { item: SleepItem, fade_step: u8 },
}

impl SleepTimer {
    pub const fn off() -> Self {
        Self::Off
    }

    pub fn arm_minutes(&mut self, now: Duration, minutes: u32) {
        let delay = Duration::from_secs(u64::from(minutes).saturating_mul(60));
        *self = Self::Minutes {
            deadline: now.saturating_add(delay),
            fade_step: 0,
        };
    }

    /// Arms the current finite item. A live station deliberately cannot be an
    /// end-of-track target because it has no duration or finish boundary.
    pub fn arm_end_of_track(&mut self, item: SleepItem) -> bool {
        if matches!(item, SleepItem::Radio(_)) {
            return false;
        }
        *self = Self::EndOfTrack { item, fade_step: 0 };
        true
    }

    pub fn cancel(&mut self) {
        *self = Self::Off;
    }

    pub const fn is_armed(&self) -> bool {
        !matches!(self, Self::Off)
    }

    pub const fn armed_item(&self) -> Option<SleepItem> {
        match self {
            Self::EndOfTrack { item, .. } => Some(*item),
            Self::Off | Self::Minutes { .. } => None,
        }
    }

    pub fn remaining_minutes(&self, now: Duration) -> Option<u64> {
        let Self::Minutes { deadline, .. } = self else {
            return None;
        };
        let remaining = deadline.saturating_sub(now).as_secs();
        Some(remaining.div_ceil(60).max(1))
    }

    pub fn tick(&mut self, now: Duration, position_ms: i64, duration_ms: i64) -> SleepAction {
        let elapsed_ms = match self {
            Self::Off => return SleepAction::None,
            Self::Minutes { deadline, .. } => {
                if now >= *deadline {
                    FADE_DURATION_MS
                } else {
                    let remaining_ms = deadline.saturating_sub(now).as_millis();
                    FADE_DURATION_MS.saturating_sub(u64::try_from(remaining_ms).unwrap_or(u64::MAX))
                }
            }
            Self::EndOfTrack { .. } => {
                if duration_ms <= 0 {
                    return SleepAction::None;
                }
                let remaining_ms = duration_ms.saturating_sub(position_ms.max(0));
                FADE_DURATION_MS.saturating_sub(u64::try_from(remaining_ms).unwrap_or(u64::MAX))
            }
        };

        if elapsed_ms >= FADE_DURATION_MS {
            if matches!(self, Self::Minutes { .. }) {
                *self = Self::Off;
                return SleepAction::Pause;
            }
            if self.fade_step() < FADE_STEPS {
                self.set_fade_step(FADE_STEPS);
                return SleepAction::SetVolume(0.0);
            }
            return SleepAction::None;
        }

        let step =
            u8::try_from(elapsed_ms.saturating_mul(u64::from(FADE_STEPS)) / FADE_DURATION_MS)
                .unwrap_or(FADE_STEPS);
        if step == 0 || step <= self.fade_step() {
            return SleepAction::None;
        }
        self.set_fade_step(step);
        SleepAction::SetVolume(f64::from(FADE_STEPS - step) / f64::from(FADE_STEPS))
    }

    pub fn on_track_finished(&mut self) -> SleepAction {
        self.finish_end_of_track()
    }

    pub fn on_gapless_advance(&mut self) -> SleepAction {
        self.finish_end_of_track()
    }

    pub fn on_manual_change(&mut self, item: SleepItem) {
        if !matches!(self, Self::EndOfTrack { .. }) {
            return;
        }
        if matches!(item, SleepItem::Radio(_)) {
            *self = Self::Off;
        } else {
            *self = Self::EndOfTrack { item, fade_step: 0 };
        }
    }

    fn finish_end_of_track(&mut self) -> SleepAction {
        if !matches!(self, Self::EndOfTrack { .. }) {
            return SleepAction::None;
        }
        *self = Self::Off;
        SleepAction::Pause
    }

    const fn fade_step(&self) -> u8 {
        match self {
            Self::Minutes { fade_step, .. } | Self::EndOfTrack { fade_step, .. } => *fade_step,
            Self::Off => 0,
        }
    }

    fn set_fade_step(&mut self, step: u8) {
        match self {
            Self::Minutes { fade_step, .. } | Self::EndOfTrack { fade_step, .. } => {
                *fade_step = step;
            }
            Self::Off => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{SleepAction, SleepItem, SleepTimer};

    const START: Duration = Duration::from_secs(100);

    #[test]
    fn a_minute_timer_pauses_at_its_deadline() {
        let mut timer = SleepTimer::off();
        timer.arm_minutes(START, 15);

        assert_eq!(
            timer.tick(START + Duration::from_secs(15 * 60), 0, 0),
            SleepAction::Pause
        );
        assert!(!timer.is_armed());
    }

    #[test]
    fn end_of_track_pauses_without_waiting_for_an_advance() {
        let mut timer = SleepTimer::off();
        assert!(timer.arm_end_of_track(SleepItem::Track(7)));

        assert_eq!(timer.on_track_finished(), SleepAction::Pause);
        assert!(!timer.is_armed());
    }

    #[test]
    fn a_gapless_handoff_pauses_on_the_new_item() {
        let mut timer = SleepTimer::off();
        assert!(timer.arm_end_of_track(SleepItem::Track(7)));

        assert_eq!(timer.on_gapless_advance(), SleepAction::Pause);
        assert!(!timer.is_armed());
    }

    #[test]
    fn a_manual_change_rearms_end_of_track_on_the_new_item() {
        let mut timer = SleepTimer::off();
        assert!(timer.arm_end_of_track(SleepItem::Track(7)));

        timer.on_manual_change(SleepItem::Episode(11));

        assert_eq!(timer.armed_item(), Some(SleepItem::Episode(11)));
        assert_eq!(timer.tick(START, 1_000, 90_000), SleepAction::None);
    }

    #[test]
    fn radio_cannot_arm_end_of_track() {
        let mut timer = SleepTimer::off();

        assert!(!timer.arm_end_of_track(SleepItem::Radio(3)));
        assert!(!timer.is_armed());
    }

    #[test]
    fn the_last_four_seconds_use_eight_equal_fade_steps() {
        let mut timer = SleepTimer::off();
        timer.arm_minutes(START, 15);
        let fade_start = START + Duration::from_secs(15 * 60 - 4);

        let actions = (1..=8)
            .map(|step| timer.tick(fade_start + Duration::from_millis(step * 500), 0, 0))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                SleepAction::SetVolume(0.875),
                SleepAction::SetVolume(0.75),
                SleepAction::SetVolume(0.625),
                SleepAction::SetVolume(0.5),
                SleepAction::SetVolume(0.375),
                SleepAction::SetVolume(0.25),
                SleepAction::SetVolume(0.125),
                SleepAction::Pause,
            ]
        );
    }

    #[test]
    fn end_of_track_fade_uses_position_and_duration() {
        let mut timer = SleepTimer::off();
        assert!(timer.arm_end_of_track(SleepItem::Episode(11)));

        assert_eq!(timer.tick(START, 55_500, 60_000), SleepAction::None);
        assert_eq!(
            timer.tick(START, 56_500, 60_000),
            SleepAction::SetVolume(0.875)
        );
    }

    #[test]
    fn end_of_track_stays_armed_after_the_zero_volume_step() {
        let mut timer = SleepTimer::off();
        assert!(timer.arm_end_of_track(SleepItem::Track(7)));

        assert_eq!(
            timer.tick(START, 60_000, 60_000),
            SleepAction::SetVolume(0.0)
        );
        assert!(timer.is_armed());
        assert_eq!(timer.on_track_finished(), SleepAction::Pause);
    }

    #[test]
    fn cancelling_clears_an_armed_timer() {
        let mut timer = SleepTimer::off();
        timer.arm_minutes(START, 30);

        timer.cancel();

        assert_eq!(timer.tick(START, 0, 0), SleepAction::None);
        assert!(!timer.is_armed());
    }
}
