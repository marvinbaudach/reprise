//! One-shot entrance choreography and period-value tweening for My Stats.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::{glib, prelude::*};
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

use super::stats_ribbon::StatsRibbon;
use crate::ui::{motion, strings};

#[derive(Clone, Default)]
pub(super) struct StatsEntrance {
    animations: Rc<RefCell<Vec<adw::TimedAnimation>>>,
    generation: Rc<Cell<u64>>,
    initialized: Rc<Cell<bool>>,
    previous_total_ms: Rc<Cell<i64>>,
    previous_bar_values: Rc<RefCell<Vec<f64>>>,
    #[cfg(test)]
    entrance_runs: Rc<Cell<u32>>,
    #[cfg(test)]
    tween_runs: Rc<Cell<u32>>,
}

impl StatsEntrance {
    pub(super) fn update(
        &self,
        total_ms: i64,
        hero_number: &gtk4::Label,
        ribbon: &StatsRibbon,
        cards: &[gtk4::Widget],
        bars: &[gtk4::LevelBar],
        entrance: bool,
    ) {
        self.next_generation();
        let targets = bars.iter().map(gtk4::LevelBar::value).collect::<Vec<_>>();
        let previous_total_ms = self.previous_total_ms.replace(total_ms);
        let previous_bar_values = self.previous_bar_values.replace(targets.clone());
        let was_initialized = self.initialized.replace(true);

        if !motion::animations_enabled() {
            land_in_end_state(total_ms, hero_number, ribbon, cards, bars, &targets);
            return;
        }

        if entrance {
            #[cfg(test)]
            self.entrance_runs
                .set(self.entrance_runs.get().saturating_add(1));
            self.run_entrance(total_ms, hero_number, ribbon, cards, bars, &targets);
        } else if was_initialized {
            #[cfg(test)]
            self.tween_runs.set(self.tween_runs.get().saturating_add(1));
            hero_number.set_label(&strings::stats_duration(previous_total_ms));
            self.animate_count(
                previous_total_ms,
                total_ms,
                hero_number,
                motion::STATS_TWEEN,
            );
            self.animate_bars(bars, &previous_bar_values, &targets, motion::STATS_TWEEN);
            ribbon.set_reveal_fraction(1.0);
            for card in cards {
                card.set_opacity(1.0);
            }
        } else {
            land_in_end_state(total_ms, hero_number, ribbon, cards, bars, &targets);
        }
    }

    fn run_entrance(
        &self,
        total_ms: i64,
        hero_number: &gtk4::Label,
        ribbon: &StatsRibbon,
        cards: &[gtk4::Widget],
        bars: &[gtk4::LevelBar],
        targets: &[f64],
    ) {
        let generation = self.generation.get();
        hero_number.set_label(&strings::stats_duration(0));
        self.animate_count(0, total_ms, hero_number, motion::STATS_COUNT);

        ribbon.set_reveal_fraction(0.0);
        let ribbon = ribbon.clone();
        let animations = self.animations.clone();
        let live_generation = self.generation.clone();
        glib::timeout_add_local_once(
            Duration::from_millis(u64::from(motion::MICRO_MS)),
            move || {
                if live_generation.get() != generation {
                    return;
                }
                let target_ribbon = ribbon.clone();
                let target = adw::CallbackAnimationTarget::new(move |value| {
                    target_ribbon.set_reveal_fraction(value);
                });
                play_and_keep(
                    &animations,
                    motion::timed(ribbon.widget(), 0.0, 1.0, motion::STATS_REVEAL, target),
                );
            },
        );

        for (index, card) in cards.iter().enumerate() {
            card.set_opacity(0.0);
            let card = card.clone();
            let animations = self.animations.clone();
            let live_generation = self.generation.clone();
            let delay = u64::from(motion::STATS_STAGGER_MS).saturating_mul(index as u64);
            glib::timeout_add_local_once(Duration::from_millis(delay), move || {
                if live_generation.get() != generation {
                    return;
                }
                let target = adw::PropertyAnimationTarget::new(&card, "opacity");
                play_and_keep(
                    &animations,
                    motion::timed(&card, 0.0, 1.0, motion::STANDARD, target),
                );
            });
        }

        self.animate_bars(bars, &vec![0.0; targets.len()], targets, motion::STATS_BAR);
    }

    fn animate_count(
        &self,
        from_ms: i64,
        to_ms: i64,
        label: &gtk4::Label,
        token: motion::MotionToken,
    ) {
        let label = label.clone();
        let target_label = label.clone();
        let target = adw::CallbackAnimationTarget::new(move |value| {
            target_label.set_label(&strings::stats_duration(value.round() as i64));
        });
        play_and_keep(
            &self.animations,
            motion::timed(
                label.upcast_ref::<gtk4::Widget>(),
                from_ms as f64,
                to_ms as f64,
                token,
                target,
            ),
        );
    }

    fn animate_bars(
        &self,
        bars: &[gtk4::LevelBar],
        from_values: &[f64],
        targets: &[f64],
        token: motion::MotionToken,
    ) {
        for (index, bar) in bars.iter().enumerate() {
            let target_value = targets.get(index).copied().unwrap_or_else(|| bar.value());
            let from_value = from_values.get(index).copied().unwrap_or(target_value);
            bar.set_value(from_value);
            let target = adw::PropertyAnimationTarget::new(bar, "value");
            play_and_keep(
                &self.animations,
                motion::timed(bar, from_value, target_value, token, target),
            );
        }
    }

    fn next_generation(&self) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let animations = self.animations.borrow_mut().drain(..).collect::<Vec<_>>();
        for animation in animations {
            animation.skip();
        }
    }

    #[cfg(test)]
    pub(super) fn entrance_runs(&self) -> u32 {
        self.entrance_runs.get()
    }

    #[cfg(test)]
    pub(super) fn tween_runs(&self) -> u32 {
        self.tween_runs.get()
    }
}

fn land_in_end_state(
    total_ms: i64,
    hero_number: &gtk4::Label,
    ribbon: &StatsRibbon,
    cards: &[gtk4::Widget],
    bars: &[gtk4::LevelBar],
    targets: &[f64],
) {
    hero_number.set_label(&strings::stats_duration(total_ms));
    ribbon.set_reveal_fraction(1.0);
    for card in cards {
        card.set_opacity(1.0);
    }
    for (bar, value) in bars.iter().zip(targets) {
        bar.set_value(*value);
    }
}

fn play_and_keep(animations: &RefCell<Vec<adw::TimedAnimation>>, animation: adw::TimedAnimation) {
    animation.play();
    animations.borrow_mut().push(animation);
}
