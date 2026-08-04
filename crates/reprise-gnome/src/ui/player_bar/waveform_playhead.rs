pub(super) const PLAYHEAD_WIDTH: f64 = 3.0;
pub(super) const PLAYHEAD_OVERHANG: f64 = 3.0;
pub(super) const AFTERGLOW_WIDTH: f64 = 14.0;
const AFTERGLOW_ALPHA: f64 = 0.33;

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
    pub(super) afterglow: bool,
}

pub(super) fn decoration_visibility(
    fill_bars: bool,
    dragging: bool,
    animations_enabled: bool,
    build_progress: f64,
    crossfade_progress: f64,
) -> DecorationVisibility {
    let settled = build_progress >= 1.0 && crossfade_progress >= 1.0;
    let decorate = !fill_bars && animations_enabled && settled;
    DecorationVisibility {
        glow: decorate,
        afterglow: decorate && !dragging,
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

pub(super) fn afterglow_rect(
    head_x: f64,
    top: f64,
    max_bar_height: f64,
    dragging: bool,
    animations_enabled: bool,
) -> Option<PlayheadRect> {
    if dragging || !animations_enabled {
        return None;
    }
    let x = (head_x - AFTERGLOW_WIDTH).max(0.0);
    let width = head_x - x;
    (width > 0.0).then_some(PlayheadRect {
        x,
        y: top,
        width,
        height: max_bar_height,
    })
}

pub(super) fn draw_afterglow(
    cr: &gtk4::cairo::Context,
    head_x: f64,
    top: f64,
    max_bar_height: f64,
    colour: (f64, f64, f64),
    dragging: bool,
    animations_enabled: bool,
) {
    let Some(rect) = afterglow_rect(head_x, top, max_bar_height, dragging, animations_enabled)
    else {
        return;
    };
    let (r, g, b) = colour;
    let gradient = gtk4::cairo::LinearGradient::new(rect.x, 0.0, head_x, 0.0);
    gradient.add_color_stop_rgba(0.0, r, g, b, 0.0);
    gradient.add_color_stop_rgba(1.0, r, g, b, AFTERGLOW_ALPHA);
    cr.save().ok();
    cr.set_operator(gtk4::cairo::Operator::Over);
    if cr.set_source(&gradient).is_ok() {
        cr.rectangle(rect.x, rect.y, rect.width, rect.height);
        let _ = cr.fill();
    }
    cr.restore().ok();
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

    #[test]
    fn afterglow_trails_left_of_the_playhead_and_clips_at_zero() {
        let ordinary = afterglow_rect(40.0, 7.0, 26.0, false, true).unwrap();
        assert_eq!((ordinary.x, ordinary.width), (26.0, 14.0));
        assert_eq!((ordinary.y, ordinary.height), (7.0, 26.0));
        assert!(ordinary.x + ordinary.width <= 40.0);

        let clipped = afterglow_rect(6.0, 7.0, 26.0, false, true).unwrap();
        assert_eq!((clipped.x, clipped.width), (0.0, 6.0));
        assert!(afterglow_rect(0.0, 7.0, 26.0, false, true).is_none());
        assert!(afterglow_rect(40.0, 7.0, 26.0, true, true).is_none());
        assert!(afterglow_rect(40.0, 7.0, 26.0, false, false).is_none());
    }

    #[test]
    fn decorations_respect_animation_drag_and_mini_player_state() {
        assert_eq!(
            decoration_visibility(false, false, true, 1.0, 1.0),
            DecorationVisibility {
                glow: true,
                afterglow: true,
            }
        );
        assert_eq!(
            decoration_visibility(false, true, true, 1.0, 1.0),
            DecorationVisibility {
                glow: true,
                afterglow: false,
            }
        );
        for visibility in [
            decoration_visibility(false, false, false, 1.0, 1.0),
            decoration_visibility(true, false, true, 1.0, 1.0),
            decoration_visibility(false, false, true, 0.5, 1.0),
        ] {
            assert_eq!(
                visibility,
                DecorationVisibility {
                    glow: false,
                    afterglow: false,
                }
            );
        }
    }
}
