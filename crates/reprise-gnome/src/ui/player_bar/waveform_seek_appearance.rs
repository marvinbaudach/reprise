//! Appearance-dependent waveform paint roles.
//!
//! The dark values are the established waveform model. Light raises the same
//! roles and replaces the fallback renderer's white coming bars with the
//! current appearance foreground.

use crate::ui::style::color_math::parse_hex_rgb;
use crate::ui::style::theme::Theme;

/// Dark coming-side alpha. Measured so deep bass remains visible without
/// erasing the played/unplayed boundary.
const UNPLAYED_ALPHA: f64 = 0.34;
/// Dark seek-preview alpha, between coming and played.
const HOVER_PREVIEW_ALPHA: f64 = 0.62;
/// Dark buffered-media alpha, between coming and played.
const BUFFERED_ALPHA: f64 = 0.48;
/// Dark section-marker alpha for the single-colour waveform.
const SECTION_MARK_ALPHA: f64 = 0.30;
/// Dark drag-ghost alpha.
const GHOST_ALPHA: f64 = 0.40;
/// Dark rounded-playhead alpha.
const PLAYHEAD_ALPHA: f64 = 0.70;

const LIGHT_UNPLAYED_ALPHA: f64 = 0.55;
const LIGHT_HOVER_PREVIEW_ALPHA: f64 = 0.78;
const LIGHT_BUFFERED_ALPHA: f64 = 0.66;
const LIGHT_SECTION_MARK_ALPHA: f64 = 0.42;
const LIGHT_GHOST_ALPHA: f64 = 0.55;
const LIGHT_PLAYHEAD_ALPHA: f64 = 0.85;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WaveformAppearance {
    pub(super) unplayed_alpha: f64,
    pub(super) hover_preview_alpha: f64,
    pub(super) buffered_alpha: f64,
    pub(super) section_mark_alpha: f64,
    pub(super) ghost_alpha: f64,
    pub(super) playhead_alpha: f64,
    pub(super) fallback_unplayed: (f64, f64, f64),
}

impl WaveformAppearance {
    pub(super) fn current() -> Self {
        let is_dark = libadwaita::StyleManager::default().is_dark();
        Self::for_appearance(is_dark, crate::ui::style::current_theme())
    }

    pub(super) fn for_appearance(is_dark: bool, theme: Theme) -> Self {
        if is_dark {
            Self {
                unplayed_alpha: UNPLAYED_ALPHA,
                hover_preview_alpha: HOVER_PREVIEW_ALPHA,
                buffered_alpha: BUFFERED_ALPHA,
                section_mark_alpha: SECTION_MARK_ALPHA,
                ghost_alpha: GHOST_ALPHA,
                playhead_alpha: PLAYHEAD_ALPHA,
                fallback_unplayed: (1.0, 1.0, 1.0),
            }
        } else {
            Self {
                unplayed_alpha: LIGHT_UNPLAYED_ALPHA,
                hover_preview_alpha: LIGHT_HOVER_PREVIEW_ALPHA,
                buffered_alpha: LIGHT_BUFFERED_ALPHA,
                section_mark_alpha: LIGHT_SECTION_MARK_ALPHA,
                ghost_alpha: LIGHT_GHOST_ALPHA,
                playhead_alpha: LIGHT_PLAYHEAD_ALPHA,
                fallback_unplayed: rgb_fraction(theme.light_palette().fg),
            }
        }
    }
}

fn rgb_fraction(hex: &str) -> (f64, f64, f64) {
    let rgb = parse_hex_rgb(hex).expect("theme foreground must be valid #RRGGBB");
    (
        f64::from(rgb[0]) / 255.0,
        f64::from(rgb[1]) / 255.0,
        f64::from(rgb[2]) / 255.0,
    )
}
