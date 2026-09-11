use std::cell::RefCell;

use gtk4::cairo;

/// The vertical fade that hands the panel back to the text.
///
/// Fully opaque from 55 % of the field down, so the title, the artist, the
/// lyrics and the segment control all sit on quiet ground. Dark and light run
/// the same numbers and differ only in the colour, which is what turns the
/// glow into a wash of colour on a light panel without a second set of values.
const SCRIM_MID_Y: f64 = 0.40;
const SCRIM_MID_ALPHA: f64 = 0.15;
const SCRIM_FULL_Y: f64 = 0.55;

/// Scrim opacity at `y` in [0, 1] of the field.
pub(super) fn scrim_alpha(y: f64) -> f64 {
    if y <= 0.0 {
        return 0.0;
    }
    if y >= SCRIM_FULL_Y {
        return 1.0;
    }
    if y <= SCRIM_MID_Y {
        return SCRIM_MID_ALPHA * (y / SCRIM_MID_Y);
    }
    let across = (y - SCRIM_MID_Y) / (SCRIM_FULL_Y - SCRIM_MID_Y);
    SCRIM_MID_ALPHA + (1.0 - SCRIM_MID_ALPHA) * across
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrimCacheKey {
    theme: crate::ui::style::theme::Theme,
    dark: bool,
    field_top: f64,
    field_height: f64,
}

pub(super) struct ScrimCache {
    key: ScrimCacheKey,
    gradient: cairo::LinearGradient,
}

fn scrim_cache_needs_rebuild(cached: Option<ScrimCacheKey>, current: ScrimCacheKey) -> bool {
    cached != Some(current)
}

pub(super) fn cached_scrim(
    cache: &RefCell<Option<ScrimCache>>,
    dark: bool,
    field_top: f64,
    field_height: f64,
) -> cairo::LinearGradient {
    let key = ScrimCacheKey {
        theme: crate::ui::style::current_theme(),
        dark,
        field_top,
        field_height,
    };
    let mut cache = cache.borrow_mut();
    if scrim_cache_needs_rebuild(cache.as_ref().map(|cached| cached.key), key) {
        *cache = Some(ScrimCache {
            key,
            gradient: build_scrim(field_top, field_height),
        });
    }
    cache
        .as_ref()
        .expect("scrim cache was populated")
        .gradient
        .clone()
}

/// Builds the fade back to the panel in the current panel colour.
pub(super) fn build_scrim(field_top: f64, field_height: f64) -> cairo::LinearGradient {
    let [r, g, b] = crate::ui::style::accent::sidebar_background_rgb();
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let fade = cairo::LinearGradient::new(0.0, field_top, 0.0, field_top + field_height);
    for step in 0..=STOPS {
        let y = f64::from(step) / f64::from(STOPS);
        fade.add_color_stop_rgba(y, r, g, b, scrim_alpha(y));
    }
    fade
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
