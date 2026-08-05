use gtk4::cairo::{Context, Format, ImageSurface};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SurfaceKey {
    width: i32,
    height: i32,
    scale_factor: i32,
    bar_count: usize,
    colour: [u64; 3],
    desaturation: u64,
    /// The colouring the cached bars were painted in. Without it, switching
    /// colouring keeps every other dimension identical and the stale surface
    /// stays on screen.
    colouring: SeekColouring,
}

impl SurfaceKey {
    fn new(
        width: i32,
        height: i32,
        scale_factor: i32,
        bar_count: usize,
        colour: (f64, f64, f64),
        desaturation: f64,
        colouring: SeekColouring,
    ) -> Self {
        Self {
            width,
            height,
            scale_factor,
            bar_count,
            colour: [colour.0.to_bits(), colour.1.to_bits(), colour.2.to_bits()],
            desaturation: desaturation.to_bits(),
            colouring,
        }
    }
}

pub(super) fn invalidate(state: &mut State) {
    state.mask_surface = None;
    state.colour_surface = None;
    state.surface_key = None;
}

pub(super) fn cache_is_live(state: &State) -> bool {
    cache_is_live_for(
        state.build_progress,
        state.crossfade_progress,
        state.desaturation_progress,
        state.desaturation_target,
    )
}

/// Whether the cached surfaces may be used, as a pure function of the three
/// animations that repaint the bars themselves.
///
/// During build-up, crossfade and desaturation every frame genuinely differs,
/// so the cache would be rebuilt per frame and buy nothing; those states fall
/// back to the direct drawing path. Split out from `cache_is_live` because a
/// forgotten condition here silently shows stale bars, and that is worth a
/// test that needs no display.
pub(super) fn cache_is_live_for(
    build_progress: f64,
    crossfade_progress: f64,
    desaturation_progress: f64,
    desaturation_target: f64,
) -> bool {
    build_progress >= 1.0
        && crossfade_progress >= 1.0
        && (desaturation_progress - desaturation_target).abs() < f64::EPSILON
}

pub(super) fn ensure_cache(
    state: &mut State,
    width: i32,
    height: i32,
    scale_factor: i32,
    widget_colour: (f64, f64, f64),
) -> bool {
    let key = SurfaceKey::new(
        width,
        height,
        scale_factor,
        state.display_peaks.len(),
        widget_colour,
        state.desaturation_progress,
        state.colouring,
    );
    if state.surface_key == Some(key)
        && state.mask_surface.is_some()
        && state.colour_surface.is_some()
    {
        return true;
    }

    let Some((mask, colour)) = build_surfaces(state, width, height, scale_factor, widget_colour)
    else {
        invalidate(state);
        return false;
    };
    state.mask_surface = Some(mask);
    state.colour_surface = Some(colour);
    state.surface_key = Some(key);
    true
}

fn build_surfaces(
    state: &State,
    width: i32,
    height: i32,
    scale_factor: i32,
    widget_colour: (f64, f64, f64),
) -> Option<(ImageSurface, ImageSurface)> {
    let scale_factor = scale_factor.max(1);
    let pixel_width = width.checked_mul(scale_factor)?;
    let pixel_height = height.checked_mul(scale_factor)?;
    let mask = ImageSurface::create(Format::A8, pixel_width, pixel_height).ok()?;
    let colour = ImageSurface::create(Format::ARgb32, pixel_width, pixel_height).ok()?;
    mask.set_device_scale(f64::from(scale_factor), f64::from(scale_factor));
    colour.set_device_scale(f64::from(scale_factor), f64::from(scale_factor));
    let mask_cr = Context::new(&mask).ok()?;
    let colour_cr = Context::new(&colour).ok()?;

    let count = state.display_peaks.len();
    if count == 0 {
        return Some((mask, colour));
    }
    let width = f64::from(width);
    let height = f64::from(height);
    let slot = width / count as f64;
    let bar_width = bar_slot_width(slot, state.fill_bars, MINI_BAR_GAP);
    let bar_radius = if state.fill_bars {
        MINI_BAR_RADIUS
    } else {
        BAR_RADIUS
    };
    let accent = scale_chroma(
        widget_colour.0,
        widget_colour.1,
        widget_colour.2,
        1.0 - 0.55 * state.desaturation_progress,
    );

    for (index, bar) in state.display_peaks.iter().copied().enumerate() {
        let bar_height = match bar {
            DisplayBar::Silence => SILENCE_DOT_HEIGHT,
            DisplayBar::Level(level) => {
                render::bar_height(f64::from(level), state.min_bar_height, state.max_bar_height)
            }
        };
        let x = index as f64 * slot + (slot - bar_width) / 2.0;
        let y = (height - bar_height) / 2.0;

        mask_cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        rounded_bar(&mask_cr, x, y, bar_width, bar_height, bar_radius);
        mask_cr.fill().ok()?;

        let spectral = state.shaped_centroid.get(index).map_or(accent, |value| {
            let value = spectral_colour(f64::from(*value));
            scale_chroma(
                value.0,
                value.1,
                value.2,
                1.0 - 0.55 * state.desaturation_progress,
            )
        });
        colour_cr.set_source_rgba(spectral.0, spectral.1, spectral.2, 1.0);
        rounded_bar(&colour_cr, x, y, bar_width, bar_height, bar_radius);
        colour_cr.fill().ok()?;
    }
    mask.flush();
    colour.flush();
    Some((mask, colour))
}

/// Paints the cached bars in three horizontal bands: coming, seek preview, and
/// played. All three read from the *same* colour surface in the frequency
/// colouring and differ only in the alpha they are painted at — the cached
/// counterpart of `render::bar_fill`, and the reason the colour survives ahead
/// of the playhead.
pub(super) fn draw_cached_bars(
    cr: &Context,
    state: &State,
    width: f64,
    height: f64,
    head_x: f64,
    accent: (f64, f64, f64),
) {
    let (Some(mask), Some(colour)) = (&state.mask_surface, &state.colour_surface) else {
        return;
    };
    let hover_x = state
        .hover_fraction
        .map_or(head_x, |hover| (hover * width).clamp(head_x, width));

    match state.colouring {
        SeekColouring::Frequency => {
            paint_surface(cr, colour, UNPLAYED_ALPHA, hover_x, width, height);
            paint_surface(cr, colour, HOVER_PREVIEW_ALPHA, head_x, hover_x, height);
            paint_surface(cr, colour, 1.0, 0.0, head_x, height);
        }
        SeekColouring::Solid => {
            paint_mask(cr, mask, SOLID_UNPLAYED, 1.0, hover_x, width, height);
            paint_mask(cr, mask, SOLID_HOVER_PREVIEW, 1.0, head_x, hover_x, height);
            // The colour surface already carries the accent here: with no
            // curve, `build_surfaces` paints every bar in it.
            paint_surface(cr, colour, 1.0, 0.0, head_x, height);
        }
    }

    render::draw_section_marks(cr, width, height, state);

    if let Some(drag) = state.drag_fraction {
        let drag_x = (drag * width).clamp(0.0, width);
        paint_mask(
            cr,
            mask,
            accent,
            GHOST_ALPHA,
            head_x.min(drag_x),
            head_x.max(drag_x),
            height,
        );
    }
}

fn paint_mask(
    cr: &Context,
    mask: &ImageSurface,
    colour: (f64, f64, f64),
    alpha: f64,
    from_x: f64,
    to_x: f64,
    height: f64,
) {
    if to_x <= from_x {
        return;
    }
    cr.save().ok();
    cr.rectangle(from_x, 0.0, to_x - from_x, height);
    cr.clip();
    cr.set_operator(gtk4::cairo::Operator::Source);
    cr.set_source_rgba(colour.0, colour.1, colour.2, alpha);
    let _ = cr.mask_surface(mask, 0.0, 0.0);
    cr.restore().ok();
}

/// Paints a horizontal band of the pre-coloured bar surface at one alpha.
/// `Source` rather than `Over` so the three bands never composite on top of
/// each other: each one owns its slice of the width outright.
fn paint_surface(
    cr: &Context,
    colour: &ImageSurface,
    alpha: f64,
    from_x: f64,
    to_x: f64,
    height: f64,
) {
    if to_x <= from_x {
        return;
    }
    cr.save().ok();
    cr.rectangle(from_x, 0.0, to_x - from_x, height);
    cr.clip();
    cr.set_operator(gtk4::cairo::Operator::Source);
    if cr.set_source_surface(colour, 0.0, 0.0).is_ok() {
        let _ = cr.paint_with_alpha(alpha);
    }
    cr.restore().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPAQUE: (f64, f64, f64) = (0.5, 0.25, 0.75);

    fn key(
        width: i32,
        height: i32,
        scale_factor: i32,
        bar_count: usize,
        colour: (f64, f64, f64),
        desaturation: f64,
    ) -> SurfaceKey {
        SurfaceKey::new(
            width,
            height,
            scale_factor,
            bar_count,
            colour,
            desaturation,
            SeekColouring::DEFAULT,
        )
    }

    #[test]
    fn surface_key_is_stable_for_unchanged_parameters() {
        assert_eq!(
            key(400, 28, 1, 80, OPAQUE, 0.0),
            key(400, 28, 1, 80, OPAQUE, 0.0)
        );
    }

    #[test]
    fn surface_key_changes_with_every_dimension_that_repaints_the_bars() {
        // One assertion per dimension: dropping any of them from the key would
        // keep a stale cache on screen, and that is invisible until someone
        // resizes, changes theme, or pauses.
        let base = key(400, 28, 1, 80, OPAQUE, 0.0);
        assert_ne!(base, key(401, 28, 1, 80, OPAQUE, 0.0), "width");
        assert_ne!(base, key(400, 29, 1, 80, OPAQUE, 0.0), "height");
        assert_ne!(base, key(400, 28, 2, 80, OPAQUE, 0.0), "scale factor");
        assert_ne!(base, key(400, 28, 1, 81, OPAQUE, 0.0), "bar count");
        assert_ne!(
            base,
            key(400, 28, 1, 80, (0.5, 0.25, 0.76), 0.0),
            "widget colour"
        );
        assert_ne!(base, key(400, 28, 1, 80, OPAQUE, 1.0), "desaturation");
        assert_ne!(
            base,
            SurfaceKey::new(400, 28, 1, 80, OPAQUE, 0.0, SeekColouring::Solid),
            "colouring"
        );
    }

    #[test]
    fn cache_serves_only_the_settled_state() {
        assert!(cache_is_live_for(1.0, 1.0, 0.0, 0.0));
        assert!(cache_is_live_for(1.0, 1.0, 1.0, 1.0));
        // Build-up, crossfade and desaturation each repaint the bars
        // themselves, so each one alone must fall back to the direct path.
        assert!(!cache_is_live_for(0.5, 1.0, 0.0, 0.0), "build-up running");
        assert!(!cache_is_live_for(1.0, 0.5, 0.0, 0.0), "crossfade running");
        assert!(
            !cache_is_live_for(1.0, 1.0, 0.4, 1.0),
            "desaturation in flight"
        );
    }
}
