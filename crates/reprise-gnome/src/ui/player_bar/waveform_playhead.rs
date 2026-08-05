//! The playhead and its glow.
//!
//! There used to be an afterglow here — a gradient trailing the played side —
//! whose job was to emphasise the progress boundary while that boundary was a
//! change of *colour*. With both sides carrying the same colour and progress
//! reading as a step in opacity, it emphasised nothing and only made the fill
//! restless, so it is gone. The playhead now carries the one hard edge in the
//! picture, which makes it more important than it was, not less.

pub(super) const PLAYHEAD_WIDTH: f64 = 3.0;
pub(super) const PLAYHEAD_OVERHANG: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PlayheadRect {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GlowLayer {
    pub(super) rect: PlayheadRect,
    pub(super) alpha: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DecorationVisibility {
    pub(super) glow: bool,
}

pub(super) fn decoration_visibility(
    fill_bars: bool,
    _dragging: bool,
    animations_enabled: bool,
    build_progress: f64,
    crossfade_progress: f64,
) -> DecorationVisibility {
    let settled = build_progress >= 1.0 && crossfade_progress >= 1.0;
    DecorationVisibility {
        glow: !fill_bars && animations_enabled && settled,
    }
}

pub(super) fn playhead_rect(head_x: f64, top: f64, max_bar_height: f64) -> PlayheadRect {
    PlayheadRect {
        x: head_x - PLAYHEAD_WIDTH / 2.0,
        y: top - PLAYHEAD_OVERHANG,
        width: PLAYHEAD_WIDTH,
        height: max_bar_height + 2.0 * PLAYHEAD_OVERHANG,
    }
}

pub(super) fn glow_layers(playhead: PlayheadRect) -> [GlowLayer; 3] {
    [(2.0, 0.35), (4.0, 0.18), (6.0, 0.08)].map(|(growth, alpha)| GlowLayer {
        rect: PlayheadRect {
            x: playhead.x - growth / 2.0,
            y: playhead.y - growth / 2.0,
            width: playhead.width + growth,
            height: playhead.height + growth,
        },
        alpha,
    })
}

pub(super) fn draw_playhead(
    cr: &gtk4::cairo::Context,
    head_x: f64,
    top: f64,
    max_bar_height: f64,
    colour: (f64, f64, f64),
    draw_glow: bool,
    alpha: f64,
) {
    let playhead = playhead_rect(head_x, top, max_bar_height);
    cr.save().ok();
    cr.set_operator(gtk4::cairo::Operator::Over);
    if draw_glow {
        for layer in glow_layers(playhead).into_iter().rev() {
            cr.set_source_rgba(colour.0, colour.1, colour.2, layer.alpha);
            super::waveform_primitives::rounded_bar(
                cr,
                layer.rect.x,
                layer.rect.y,
                layer.rect.width,
                layer.rect.height,
                layer.rect.width / 2.0,
            );
            let _ = cr.fill();
        }
    }
    cr.set_source_rgba(colour.0, colour.1, colour.2, alpha);
    super::waveform_primitives::rounded_bar(
        cr,
        playhead.x,
        playhead.y,
        playhead.width,
        playhead.height,
        PLAYHEAD_WIDTH / 2.0,
    );
    let _ = cr.fill();
    cr.restore().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playhead_is_three_pixels_wide_with_three_pixel_overhangs() {
        let rect = playhead_rect(40.0, 7.0, 26.0);
        assert_eq!(rect.x, 38.5);
        assert_eq!(rect.y, 4.0);
        assert_eq!(rect.width, 3.0);
        assert_eq!(rect.height, 32.0);
    }

    #[test]
    fn glow_has_three_fixed_layers_from_inner_to_outer() {
        let layers = glow_layers(playhead_rect(40.0, 7.0, 26.0));
        assert_eq!(layers.map(|layer| layer.rect.width), [5.0, 7.0, 9.0]);
        assert_eq!(layers.map(|layer| layer.alpha), [0.35, 0.18, 0.08]);
    }

    /// The afterglow is gone, and this is the regression that keeps it gone:
    /// it existed to mark a boundary that was a colour change. With both sides
    /// carrying the same colour it marked nothing and only made the played
    /// fill restless, so no decoration may trail the playhead again.
    #[test]
    fn nothing_trails_the_played_side_of_the_playhead() {
        let source = include_str!("waveform_playhead.rs");
        assert!(!source.contains(&["AFTER", "GLOW_WIDTH"].concat()));
        assert!(!source.contains(&["fn after", "glow_rect"].concat()));
        let render = include_str!("waveform_seek_render.rs");
        assert!(!render.contains(&["draw_after", "glow"].concat()));
    }

    #[test]
    fn decorations_respect_animation_drag_and_mini_player_state() {
        // The glow survives a drag: it is the playhead's own, and the playhead
        // is exactly what the user is holding on to.
        assert_eq!(
            decoration_visibility(false, false, true, 1.0, 1.0),
            DecorationVisibility { glow: true }
        );
        assert_eq!(
            decoration_visibility(false, true, true, 1.0, 1.0),
            DecorationVisibility { glow: true }
        );
        for visibility in [
            decoration_visibility(false, false, false, 1.0, 1.0),
            decoration_visibility(true, false, true, 1.0, 1.0),
            decoration_visibility(false, false, true, 0.5, 1.0),
        ] {
            assert_eq!(visibility, DecorationVisibility { glow: false });
        }
    }
}
