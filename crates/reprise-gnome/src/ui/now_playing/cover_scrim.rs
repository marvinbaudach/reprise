use std::cell::RefCell;

use gtk4::cairo;

use crate::ui::style::tokens;

/// The vertical fade that hands the artwork band back to the title.
const SCRIM_MID_SHARE: f64 = 2.0 / 3.0;
const SCRIM_MID_ALPHA: f64 = 0.15;

pub(super) fn scrim_alpha(y_px: f64, title_top: f64) -> f64 {
    if y_px <= 0.0 || title_top <= 0.0 {
        return 0.0;
    }
    if y_px >= title_top {
        return 1.0;
    }
    let midpoint = SCRIM_MID_SHARE * title_top;
    if y_px <= midpoint {
        return SCRIM_MID_ALPHA * (y_px / midpoint);
    }
    let across = (y_px - midpoint) / (title_top - midpoint);
    SCRIM_MID_ALPHA + (1.0 - SCRIM_MID_ALPHA) * across
}

pub(super) fn left_fade_alpha(x_px: f64, width: f64) -> f64 {
    if width <= 0.0 {
        return 0.0;
    }
    if x_px <= 0.0 {
        return 1.0;
    }
    let fade_width = tokens::NOW_PLAYING_LEFT_FADE_SHARE * width;
    (1.0 - x_px / fade_width).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrimCacheKey {
    theme: crate::ui::style::theme::Theme,
    dark: bool,
    width: f64,
}

pub(super) struct ScrimCache {
    key: ScrimCacheKey,
    vertical: cairo::LinearGradient,
    left: cairo::LinearGradient,
}

fn scrim_cache_needs_rebuild(cached: Option<ScrimCacheKey>, current: ScrimCacheKey) -> bool {
    cached != Some(current)
}

pub(super) fn cached_scrim(
    cache: &RefCell<Option<ScrimCache>>,
    dark: bool,
    width: f64,
) -> (cairo::LinearGradient, cairo::LinearGradient) {
    let key = ScrimCacheKey {
        theme: crate::ui::style::current_theme(),
        dark,
        width,
    };
    let mut cache = cache.borrow_mut();
    if scrim_cache_needs_rebuild(cache.as_ref().map(|cached| cached.key), key) {
        *cache = Some(ScrimCache {
            key,
            vertical: build_scrim(f64::from(tokens::NOW_PLAYING_ARTWORK_BAND)),
            left: build_left_fade(width),
        });
    }
    let cache = cache.as_ref().expect("scrim cache was populated");
    (cache.vertical.clone(), cache.left.clone())
}

/// Builds the fade back to the panel in the current panel colour.
pub(super) fn build_scrim(title_top: f64) -> cairo::LinearGradient {
    let [r, g, b] = crate::ui::style::accent::sidebar_background_rgb();
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let fade = cairo::LinearGradient::new(0.0, 0.0, 0.0, title_top);
    for step in 0..=STOPS {
        let share = f64::from(step) / f64::from(STOPS);
        fade.add_color_stop_rgba(share, r, g, b, scrim_alpha(share * title_top, title_top));
    }
    fade
}

fn build_left_fade(width: f64) -> cairo::LinearGradient {
    let [r, g, b] = crate::ui::style::accent::sidebar_background_rgb();
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let end = tokens::NOW_PLAYING_LEFT_FADE_SHARE * width;
    // Cairo samples image pixels at their centres. Starting at the first
    // centre keeps the outermost panel pixel fully settled instead of
    // leaking a fraction of the cloud through it.
    let fade = cairo::LinearGradient::new(0.5, 0.0, end, 0.0);
    fade.add_color_stop_rgba(0.0, r, g, b, left_fade_alpha(0.0, width));
    fade.add_color_stop_rgba(1.0, r, g, b, left_fade_alpha(end, width));
    fade
}

pub(super) fn paint(
    cr: &cairo::Context,
    cache: &RefCell<Option<ScrimCache>>,
    dark: bool,
    width: f64,
    band: f64,
) {
    let (vertical, left) = cached_scrim(cache, dark, width);
    paint_scrim(cr, width, band, &vertical);
    paint_scrim(cr, width, band, &left);
}

pub(super) fn paint_scrim(
    cr: &cairo::Context,
    width: f64,
    band: f64,
    fade: &cairo::LinearGradient,
) {
    cr.set_source(fade).ok();
    cr.rectangle(0.0, 0.0, width, band);
    cr.fill().ok();
}

/// The scrim is a bend, not a line, so it is handed to Cairo as stops along it.
const STOPS: i32 = 24;

#[cfg(test)]
#[path = "cover_scrim_tests.rs"]
mod tests;
