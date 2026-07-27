//! Bar-only entrance motion and period-value tweening for My Stats.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::{glib, prelude::*};
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

use super::stats_genre_card::StatsGenreCard;
use super::stats_ribbon::StatsRibbon;
use crate::ui::motion;
use crate::ui::motion_reveal::HorizontalReveal;

#[derive(Clone)]
pub(super) struct HorizontalBarGroup {
    bars: Vec<gtk4::LevelBar>,
    reveals: Vec<HorizontalReveal>,
}

impl HorizontalBarGroup {
    pub(super) fn new(bars: Vec<gtk4::LevelBar>, reveals: Vec<HorizontalReveal>) -> Self {
        Self { bars, reveals }
    }

    fn target_values(&self) -> Vec<f64> {
        self.bars
            .iter()
            .map(gtk4::LevelBar::value)
            .chain(std::iter::repeat_n(1.0, self.reveals.len()))
            .collect()
    }

    fn len(&self) -> usize {
        self.bars.len() + self.reveals.len()
    }

    fn set_value(&self, index: usize, value: f64) {
        if let Some(bar) = self.bars.get(index) {
            bar.set_value(value);
            return;
        }
        if let Some(reveal) = self.reveals.get(index.saturating_sub(self.bars.len())) {
            reveal.set_reveal_fraction(value as f32);
        }
    }

    fn widget(&self, index: usize) -> Option<gtk4::Widget> {
        self.bars
            .get(index)
            .map(|bar| bar.clone().upcast())
            .or_else(|| {
                self.reveals
                    .get(index.saturating_sub(self.bars.len()))
                    .map(|reveal| reveal.clone().upcast())
            })
    }
}

#[derive(Clone, Default)]
pub(super) struct StatsEntrance {
    animations: Rc<RefCell<Vec<adw::TimedAnimation>>>,
    generation: Rc<Cell<u64>>,
    initialized: Rc<Cell<bool>>,
    previous_ribbon_values: Rc<RefCell<Vec<f64>>>,
    previous_group_values: Rc<RefCell<Vec<Vec<f64>>>>,
    previous_genre_shares: Rc<RefCell<Vec<f64>>>,
}

impl StatsEntrance {
    pub(super) fn update(
        &self,
        ribbon: &StatsRibbon,
        groups: &[HorizontalBarGroup],
        genres: &StatsGenreCard,
        entrance: bool,
    ) {
        self.next_generation();
        let ribbon_targets = ribbon.target_values();
        let group_targets = groups
            .iter()
            .map(HorizontalBarGroup::target_values)
            .collect::<Vec<_>>();
        let previous_ribbon = self.previous_ribbon_values.replace(ribbon_targets.clone());
        let previous_groups = self.previous_group_values.replace(group_targets.clone());
        let genre_targets = genres.target_segment_shares();
        let previous_genres = self.previous_genre_shares.replace(genre_targets.clone());
        let was_initialized = self.initialized.replace(true);

        if !motion::animations_enabled() {
            land_in_end_state(ribbon, groups, &group_targets, genres, &genre_targets);
            return;
        }

        if entrance {
            self.run_entrance(ribbon, groups, &group_targets);
        } else if was_initialized {
            self.run_period_tween(
                ribbon,
                groups,
                &previous_ribbon,
                &ribbon_targets,
                &previous_groups,
                &group_targets,
                genres,
                &previous_genres,
                &genre_targets,
            );
        } else {
            land_in_end_state(ribbon, groups, &group_targets, genres, &genre_targets);
        }
    }

    fn run_entrance(
        &self,
        ribbon: &StatsRibbon,
        groups: &[HorizontalBarGroup],
        group_targets: &[Vec<f64>],
    ) {
        ribbon.prepare_entrance();
        if ribbon.is_sparse() {
            let best_index = ribbon.best_week_index();
            for index in 0..ribbon.bar_count() {
                let delay = motion::STATS_ENTRANCE_DELAY_MS
                    .saturating_add(motion::STATS_CHART_STAGGER_MS.saturating_mul(index as u32));
                self.animate_ribbon_bar_at(ribbon, index, delay, Some(index) == best_index);
            }
            if best_index.is_none() {
                ribbon.set_best_week_label_opacity(1.0);
            }
        }

        for (group, targets) in groups.iter().zip(group_targets) {
            self.animate_group_at(
                group,
                &vec![0.0; group.len()],
                targets,
                motion::STATS_HORIZONTAL_BAR,
                motion::STATS_ENTRANCE_DELAY_MS,
                true,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_period_tween(
        &self,
        ribbon: &StatsRibbon,
        groups: &[HorizontalBarGroup],
        previous_ribbon: &[f64],
        ribbon_targets: &[f64],
        previous_groups: &[Vec<f64>],
        group_targets: &[Vec<f64>],
        genres: &StatsGenreCard,
        previous_genres: &[f64],
        genre_targets: &[f64],
    ) {
        ribbon.land_in_end_state();
        if ribbon.is_sparse() {
            let from = align_from_right(previous_ribbon, ribbon_targets.len());
            ribbon.set_display_values(&from);
            self.animate_ribbon_values(ribbon, &from, ribbon_targets);
        }

        for (index, (group, targets)) in groups.iter().zip(group_targets).enumerate() {
            let from = previous_groups.get(index).map_or_else(
                || vec![0.0; targets.len()],
                |values| align_from_left(values, targets.len()),
            );
            self.animate_group_at(group, &from, targets, motion::STATS_TWEEN, 0, false);
        }

        let from = align_from_left(previous_genres, genre_targets.len());
        genres.set_segment_shares(&from);
        self.animate_genre_shares(genres, &from, genre_targets);
    }

    fn animate_ribbon_bar_at(
        &self,
        ribbon: &StatsRibbon,
        index: usize,
        delay_ms: u32,
        reveal_label_on_done: bool,
    ) {
        let ribbon = ribbon.clone();
        let animations = self.animations.clone();
        let generation = self.generation.get();
        let live_generation = self.generation.clone();
        self.schedule(delay_ms, move || {
            let target_ribbon = ribbon.clone();
            let animation =
                motion::stats_timed(ribbon.widget(), motion::STATS_CHART_BAR, move |progress| {
                    target_ribbon.set_bar_fraction(index, progress);
                });
            if reveal_label_on_done {
                let label_ribbon = ribbon.clone();
                let label_animations = Rc::downgrade(&animations);
                let live_generation = live_generation.clone();
                animation.connect_done(move |_| {
                    if live_generation.get() != generation {
                        return;
                    }
                    let Some(label_animations) = label_animations.upgrade() else {
                        return;
                    };
                    label_ribbon.set_best_week_label_opacity(0.0);
                    let target_ribbon = label_ribbon.clone();
                    play_and_keep(
                        &label_animations,
                        motion::stats_timed(
                            label_ribbon.widget(),
                            motion::STATS_LABEL,
                            move |progress| {
                                target_ribbon.set_best_week_label_opacity(progress);
                            },
                        ),
                    );
                });
            }
            play_and_keep(&animations, animation);
        });
    }

    fn animate_ribbon_values(&self, ribbon: &StatsRibbon, from: &[f64], targets: &[f64]) {
        let ribbon = ribbon.clone();
        let from = from.to_vec();
        let targets = targets.to_vec();
        let animations = self.animations.clone();
        let target_ribbon = ribbon.clone();
        play_and_keep(
            &animations,
            motion::stats_timed(ribbon.widget(), motion::STATS_TWEEN, move |progress| {
                let values = interpolate_values(&from, &targets, progress);
                target_ribbon.set_display_values(&values);
            }),
        );
    }

    fn animate_genre_shares(&self, genres: &StatsGenreCard, from: &[f64], targets: &[f64]) {
        let genres = genres.clone();
        let from = from.to_vec();
        let targets = targets.to_vec();
        let animations = self.animations.clone();
        let target_genres = genres.clone();
        play_and_keep(
            &animations,
            motion::stats_timed(genres.widget(), motion::STATS_TWEEN, move |progress| {
                let values = interpolate_values(&from, &targets, progress);
                target_genres.set_segment_shares(&values);
            }),
        );
    }

    fn animate_group_at(
        &self,
        group: &HorizontalBarGroup,
        from_values: &[f64],
        targets: &[f64],
        token: motion::MotionToken,
        delay_ms: u32,
        stagger: bool,
    ) {
        for index in 0..group.len() {
            let target = targets.get(index).copied().unwrap_or(1.0);
            let from = from_values.get(index).copied().unwrap_or(0.0);
            group.set_value(index, from);
            let Some(widget) = group.widget(index) else {
                continue;
            };
            let group = group.clone();
            let animations = self.animations.clone();
            let delay = delay_ms.saturating_add(if stagger {
                motion::STATS_HORIZONTAL_STAGGER_MS.saturating_mul(index as u32)
            } else {
                0
            });
            self.schedule(delay, move || {
                let target_group = group.clone();
                play_and_keep(
                    &animations,
                    motion::stats_timed(&widget, token, move |progress| {
                        target_group.set_value(index, interpolate(from, target, progress));
                    }),
                );
            });
        }
    }

    fn schedule(&self, delay_ms: u32, callback: impl FnOnce() + 'static) {
        let generation = self.generation.get();
        let live_generation = self.generation.clone();
        glib::timeout_add_local_once(Duration::from_millis(u64::from(delay_ms)), move || {
            if live_generation.get() == generation {
                callback();
            }
        });
    }

    fn next_generation(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        let animations = self.animations.borrow_mut().drain(..).collect::<Vec<_>>();
        for animation in animations {
            animation.skip();
        }
    }
}

fn land_in_end_state(
    ribbon: &StatsRibbon,
    groups: &[HorizontalBarGroup],
    targets: &[Vec<f64>],
    genres: &StatsGenreCard,
    genre_targets: &[f64],
) {
    ribbon.land_in_end_state();
    for (group, targets) in groups.iter().zip(targets) {
        for (index, target) in targets.iter().enumerate() {
            group.set_value(index, *target);
        }
    }
    genres.set_segment_shares(genre_targets);
}

fn align_from_left(values: &[f64], target_len: usize) -> Vec<f64> {
    (0..target_len)
        .map(|index| values.get(index).copied().unwrap_or(0.0))
        .collect()
}

fn align_from_right(values: &[f64], target_len: usize) -> Vec<f64> {
    if values.len() >= target_len {
        return values[values.len() - target_len..].to_vec();
    }
    let mut aligned = vec![0.0; target_len - values.len()];
    aligned.extend_from_slice(values);
    aligned
}

fn interpolate_values(from: &[f64], targets: &[f64], progress: f64) -> Vec<f64> {
    from.iter()
        .zip(targets)
        .map(|(from, target)| interpolate(*from, *target, progress))
        .collect()
}

fn interpolate(from: f64, target: f64, progress: f64) -> f64 {
    from + (target - from) * progress
}

fn play_and_keep(animations: &RefCell<Vec<adw::TimedAnimation>>, animation: adw::TimedAnimation) {
    animation.play();
    animations.borrow_mut().push(animation);
}
