//! One-shot entrance choreography and period-value tweening for My Stats.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::{glib, prelude::*};
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;

use super::stats_ribbon::StatsRibbon;
use crate::ui::motion_slide::SlideBin;
use crate::ui::{motion, strings};

#[derive(Clone)]
pub(super) struct EntranceCard {
    pub(super) slide: SlideBin,
    pub(super) bars: Vec<gtk4::LevelBar>,
    pub(super) reveals: Vec<SlideBin>,
}

impl EntranceCard {
    pub(super) fn new(slide: &SlideBin, bars: Vec<gtk4::LevelBar>, reveals: Vec<SlideBin>) -> Self {
        Self {
            slide: slide.clone(),
            bars,
            reveals,
        }
    }
}

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
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update(
        &self,
        total_ms: i64,
        hero_number: &gtk4::Label,
        ribbon: &StatsRibbon,
        hero_slides: &[SlideBin],
        kpi_slide: &SlideBin,
        chart_slide: &SlideBin,
        cards: &[EntranceCard],
        viewport: &gtk4::ScrolledWindow,
        scroll_content: &gtk4::Widget,
        entrance: bool,
    ) {
        self.next_generation();
        let bars = card_bars(cards);
        let targets = bars.iter().map(gtk4::LevelBar::value).collect::<Vec<_>>();
        let previous_total_ms = self.previous_total_ms.replace(total_ms);
        let previous_bar_values = self.previous_bar_values.replace(targets.clone());
        let was_initialized = self.initialized.replace(true);

        if !motion::animations_enabled() {
            land_in_end_state(
                total_ms,
                hero_number,
                ribbon,
                hero_slides,
                kpi_slide,
                chart_slide,
                cards,
                &targets,
            );
            return;
        }

        if entrance {
            #[cfg(test)]
            self.entrance_runs
                .set(self.entrance_runs.get().saturating_add(1));
            self.run_entrance(
                total_ms,
                hero_number,
                ribbon,
                hero_slides,
                kpi_slide,
                chart_slide,
                cards,
                viewport,
                scroll_content,
                &targets,
            );
        } else if was_initialized {
            #[cfg(test)]
            self.tween_runs.set(self.tween_runs.get().saturating_add(1));
            land_motion_in_end_state(ribbon, hero_slides, kpi_slide, chart_slide, cards);
            hero_number.set_label(&strings::stats_duration(previous_total_ms));
            self.animate_count_at(
                previous_total_ms,
                total_ms,
                hero_number,
                motion::STATS_TWEEN,
                0,
            );
            self.animate_bars_at(
                &bars,
                &previous_bar_values,
                &targets,
                motion::STATS_TWEEN,
                0,
                false,
            );
        } else {
            land_in_end_state(
                total_ms,
                hero_number,
                ribbon,
                hero_slides,
                kpi_slide,
                chart_slide,
                cards,
                &targets,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_entrance(
        &self,
        total_ms: i64,
        hero_number: &gtk4::Label,
        ribbon: &StatsRibbon,
        hero_slides: &[SlideBin],
        kpi_slide: &SlideBin,
        chart_slide: &SlideBin,
        cards: &[EntranceCard],
        viewport: &gtk4::ScrolledWindow,
        scroll_content: &gtk4::Widget,
        targets: &[f64],
    ) {
        for slide in hero_slides {
            prepare_slide(slide, motion::STATS_SLIDE_HERO_PX);
            self.animate_slide_at(slide, motion::STATS_HERO, 0);
        }

        hero_number.set_label(&strings::stats_duration(0));
        self.animate_count_at(
            0,
            total_ms,
            hero_number,
            motion::STATS_COUNT,
            motion::STATS_COUNT_DELAY_MS,
        );

        prepare_slide(kpi_slide, motion::STATS_SLIDE_KPI_PX);
        self.animate_slide_at(kpi_slide, motion::STATS_KPI, motion::STATS_KPI_DELAY_MS);

        chart_slide.set_opacity(1.0);
        chart_slide.set_offset_y(0.0);
        chart_slide.set_reveal_fraction(1.0);
        ribbon.set_reveal_fraction(0.0);
        ribbon.set_marker_opacity(0.0);
        self.animate_ribbon_at(ribbon, motion::STATS_CHART_DELAY_MS);
        self.animate_marker_at(ribbon, motion::STATS_MARKER_DELAY_MS);

        let mut target_index: usize = 0;
        for (card_index, card) in cards.iter().enumerate() {
            let next_target_index = target_index.saturating_add(card.bars.len());
            let card_targets = &targets[target_index..next_target_index];
            target_index = next_target_index;
            if !card_is_in_viewport(&card.slide, viewport, scroll_content) {
                land_card_in_end_state(card, card_targets);
                continue;
            }
            prepare_slide(&card.slide, motion::STATS_SLIDE_CARD_PX);
            let card_delay = motion::STATS_CARDS_DELAY_MS
                .saturating_add(motion::STATS_CARD_STAGGER_MS.saturating_mul(card_index as u32));
            self.animate_slide_at(&card.slide, motion::STATS_CARD, card_delay);

            let bar_delay = card_delay.saturating_add(motion::STATS_BAR_DELAY_MS);
            self.animate_bars_at(
                &card.bars,
                &vec![0.0; card.bars.len()],
                card_targets,
                motion::STATS_BAR,
                bar_delay,
                true,
            );
            self.animate_reveals_at(&card.reveals, bar_delay);
        }
    }

    fn animate_slide_at(&self, slide: &SlideBin, token: motion::MotionToken, delay_ms: u32) {
        let slide = slide.clone();
        let animations = self.animations.clone();
        self.schedule(delay_ms, move || {
            let opacity = adw::PropertyAnimationTarget::new(&slide, "opacity");
            play_and_keep(&animations, motion::timed(&slide, 0.0, 1.0, token, opacity));
            let offset = adw::PropertyAnimationTarget::new(&slide, "offset-y");
            play_and_keep(
                &animations,
                motion::timed(&slide, f64::from(slide.offset_y()), 0.0, token, offset),
            );
        });
    }

    fn animate_count_at(
        &self,
        from_ms: i64,
        to_ms: i64,
        label: &gtk4::Label,
        token: motion::MotionToken,
        delay_ms: u32,
    ) {
        let label = label.clone();
        let animations = self.animations.clone();
        self.schedule(delay_ms, move || {
            let target_label = label.clone();
            let target = adw::CallbackAnimationTarget::new(move |value| {
                target_label.set_label(&strings::stats_duration(value.round() as i64));
            });
            play_and_keep(
                &animations,
                motion::timed(
                    label.upcast_ref::<gtk4::Widget>(),
                    from_ms as f64,
                    to_ms as f64,
                    token,
                    target,
                ),
            );
        });
    }

    fn animate_ribbon_at(&self, ribbon: &StatsRibbon, delay_ms: u32) {
        let ribbon = ribbon.clone();
        let animations = self.animations.clone();
        self.schedule(delay_ms, move || {
            let target_ribbon = ribbon.clone();
            let target = adw::CallbackAnimationTarget::new(move |value| {
                target_ribbon.set_reveal_fraction(value);
            });
            play_and_keep(
                &animations,
                motion::timed(ribbon.widget(), 0.0, 1.0, motion::STATS_REVEAL, target),
            );
        });
    }

    fn animate_marker_at(&self, ribbon: &StatsRibbon, delay_ms: u32) {
        let ribbon = ribbon.clone();
        let animations = self.animations.clone();
        self.schedule(delay_ms, move || {
            let target_ribbon = ribbon.clone();
            let target = adw::CallbackAnimationTarget::new(move |value| {
                target_ribbon.set_marker_opacity(value);
            });
            play_and_keep(
                &animations,
                motion::timed(ribbon.widget(), 0.0, 1.0, motion::STATS_MARKER, target),
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn animate_bars_at(
        &self,
        bars: &[gtk4::LevelBar],
        from_values: &[f64],
        targets: &[f64],
        token: motion::MotionToken,
        delay_ms: u32,
        stagger: bool,
    ) {
        for (index, bar) in bars.iter().enumerate() {
            let target_value = targets.get(index).copied().unwrap_or_else(|| bar.value());
            let from_value = from_values.get(index).copied().unwrap_or(target_value);
            bar.set_value(from_value);
            let bar = bar.clone();
            let animations = self.animations.clone();
            let delay = delay_ms.saturating_add(if stagger {
                motion::STATS_BAR_STAGGER_MS.saturating_mul(index as u32)
            } else {
                0
            });
            self.schedule(delay, move || {
                let target = adw::PropertyAnimationTarget::new(&bar, "value");
                play_and_keep(
                    &animations,
                    motion::timed(&bar, from_value, target_value, token, target),
                );
            });
        }
    }

    fn animate_reveals_at(&self, reveals: &[SlideBin], delay_ms: u32) {
        for (index, reveal) in reveals.iter().enumerate() {
            reveal.set_reveal_fraction(0.0);
            let reveal = reveal.clone();
            let animations = self.animations.clone();
            let delay =
                delay_ms.saturating_add(motion::STATS_BAR_STAGGER_MS.saturating_mul(index as u32));
            self.schedule(delay, move || {
                let target = adw::PropertyAnimationTarget::new(&reveal, "reveal-fraction");
                play_and_keep(
                    &animations,
                    motion::timed(&reveal, 0.0, 1.0, motion::STATS_BAR, target),
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

fn prepare_slide(slide: &SlideBin, offset_y: f32) {
    slide.set_opacity(0.0);
    slide.set_offset_y(offset_y);
    slide.set_reveal_fraction(1.0);
}

fn card_is_in_viewport(
    card: &SlideBin,
    viewport: &gtk4::ScrolledWindow,
    scroll_content: &gtk4::Widget,
) -> bool {
    let adjustment = viewport.vadjustment();
    let Some(bounds) = card.compute_bounds(scroll_content) else {
        return true;
    };
    if bounds.height() <= 0.0 || adjustment.page_size() <= 0.0 {
        return true;
    }
    let card_top = f64::from(bounds.y());
    let card_bottom = f64::from(bounds.y() + bounds.height());
    let viewport_top = adjustment.value();
    let viewport_bottom = viewport_top + adjustment.page_size();
    card_bottom > viewport_top && card_top < viewport_bottom
}

fn land_card_in_end_state(card: &EntranceCard, targets: &[f64]) {
    card.slide.set_opacity(1.0);
    card.slide.set_offset_y(0.0);
    card.slide.set_reveal_fraction(1.0);
    for (bar, target) in card.bars.iter().zip(targets) {
        bar.set_value(*target);
    }
    for reveal in &card.reveals {
        reveal.set_reveal_fraction(1.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn land_in_end_state(
    total_ms: i64,
    hero_number: &gtk4::Label,
    ribbon: &StatsRibbon,
    hero_slides: &[SlideBin],
    kpi_slide: &SlideBin,
    chart_slide: &SlideBin,
    cards: &[EntranceCard],
    targets: &[f64],
) {
    hero_number.set_label(&strings::stats_duration(total_ms));
    land_motion_in_end_state(ribbon, hero_slides, kpi_slide, chart_slide, cards);
    for (bar, value) in card_bars(cards).iter().zip(targets) {
        bar.set_value(*value);
    }
}

fn land_motion_in_end_state(
    ribbon: &StatsRibbon,
    hero_slides: &[SlideBin],
    kpi_slide: &SlideBin,
    chart_slide: &SlideBin,
    cards: &[EntranceCard],
) {
    ribbon.set_reveal_fraction(1.0);
    ribbon.set_marker_opacity(1.0);
    for slide in hero_slides
        .iter()
        .chain(std::iter::once(kpi_slide))
        .chain(std::iter::once(chart_slide))
        .chain(cards.iter().map(|card| &card.slide))
    {
        slide.set_opacity(1.0);
        slide.set_offset_y(0.0);
        slide.set_reveal_fraction(1.0);
    }
    for reveal in cards.iter().flat_map(|card| &card.reveals) {
        reveal.set_reveal_fraction(1.0);
    }
}

fn card_bars(cards: &[EntranceCard]) -> Vec<gtk4::LevelBar> {
    cards
        .iter()
        .flat_map(|card| card.bars.iter().cloned())
        .collect()
}

fn play_and_keep(animations: &RefCell<Vec<adw::TimedAnimation>>, animation: adw::TimedAnimation) {
    animation.play();
    animations.borrow_mut().push(animation);
}
