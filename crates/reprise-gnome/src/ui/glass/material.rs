//! Renderer-aware, cover-independent chrome material.

use gtk4::prelude::*;

const BLUR_RADIUS: f32 = 24.0;
const GLASS_TINT_ALPHA: f32 = 0.80;
const FALLBACK_TINT_ALPHA: f32 = 0.94;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RendererClass {
    Hardware,
    Cairo,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlassTheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlassMode {
    BackdropBlur,
    FallbackTint,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NeutralTint {
    pub(crate) red: f32,
    pub(crate) green: f32,
    pub(crate) blue: f32,
    pub(crate) alpha: f32,
}

impl NeutralTint {
    #[cfg(test)]
    pub(crate) fn is_neutral(self) -> bool {
        (self.red - self.green).abs() < f32::EPSILON
            && (self.green - self.blue).abs() < f32::EPSILON
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlassMaterial {
    pub(crate) mode: GlassMode,
    pub(crate) blur_radius: f32,
    pub(crate) tint: NeutralTint,
    foreground: f32,
    primary_alpha: f32,
    secondary_alpha: f32,
}

impl GlassMaterial {
    #[cfg(test)]
    pub(crate) fn worst_case_primary_contrast(self) -> f32 {
        self.worst_case_contrast(self.primary_alpha)
    }

    #[cfg(test)]
    pub(crate) fn worst_case_secondary_contrast(self) -> f32 {
        self.worst_case_contrast(self.secondary_alpha)
    }

    #[cfg(test)]
    fn worst_case_contrast(self, foreground_alpha: f32) -> f32 {
        [0.0_f32, 1.0]
            .into_iter()
            .map(|content| {
                let background = composite(self.tint.red, self.tint.alpha, content);
                let foreground = composite(self.foreground, foreground_alpha, background);
                contrast_ratio(foreground, background)
            })
            .fold(f32::INFINITY, f32::min)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GlassEnvironment {
    renderer: RendererClass,
    animations_enabled: bool,
    high_contrast: bool,
}

impl GlassEnvironment {
    pub(crate) fn new(
        renderer: RendererClass,
        animations_enabled: bool,
        high_contrast: bool,
    ) -> Self {
        Self {
            renderer,
            animations_enabled,
            high_contrast,
        }
    }

    pub(crate) fn for_widget(widget: &impl IsA<gtk4::Widget>) -> Self {
        let renderer = widget
            .native()
            .and_then(|native| native.renderer())
            .map_or(RendererClass::Unknown, |renderer| {
                classify_renderer(renderer.type_().name())
            });
        Self::new(
            renderer,
            widget.settings().is_gtk_enable_animations(),
            libadwaita::StyleManager::default().is_high_contrast(),
        )
    }

    pub(crate) fn material(self, theme: GlassTheme) -> GlassMaterial {
        let blur_available = self.renderer == RendererClass::Hardware
            && self.animations_enabled
            && !self.high_contrast;
        let alpha = if blur_available {
            GLASS_TINT_ALPHA
        } else {
            FALLBACK_TINT_ALPHA
        };
        let (neutral, foreground, primary_alpha, secondary_alpha) = match theme {
            GlassTheme::Light => (1.0, 0.0, 0.92, 0.78),
            GlassTheme::Dark => (0.0, 1.0, 0.95, 0.82),
        };
        GlassMaterial {
            mode: if blur_available {
                GlassMode::BackdropBlur
            } else {
                GlassMode::FallbackTint
            },
            blur_radius: BLUR_RADIUS,
            tint: NeutralTint {
                red: neutral,
                green: neutral,
                blue: neutral,
                alpha,
            },
            foreground,
            primary_alpha,
            secondary_alpha,
        }
    }
}

pub(crate) fn current_theme() -> GlassTheme {
    if libadwaita::StyleManager::default().is_dark() {
        GlassTheme::Dark
    } else {
        GlassTheme::Light
    }
}

fn classify_renderer(type_name: &str) -> RendererClass {
    let normalized = type_name.to_ascii_lowercase();
    if normalized.contains("cairo") || normalized.contains("broadway") {
        RendererClass::Cairo
    } else if normalized.contains("gl") || normalized.contains("vulkan") {
        RendererClass::Hardware
    } else {
        RendererClass::Unknown
    }
}

#[cfg(test)]
fn composite(foreground: f32, alpha: f32, background: f32) -> f32 {
    foreground * alpha + background * (1.0 - alpha)
}

#[cfg(test)]
fn contrast_ratio(first: f32, second: f32) -> f32 {
    let first = linear_luminance(first);
    let second = linear_luminance(second);
    let lighter = first.max(second);
    let darker = first.min(second);
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
fn linear_luminance(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_names_are_classified_fail_closed() {
        assert_eq!(classify_renderer("GskGLRenderer"), RendererClass::Hardware);
        assert_eq!(classify_renderer("GskNglRenderer"), RendererClass::Hardware);
        assert_eq!(
            classify_renderer("GskVulkanRenderer"),
            RendererClass::Hardware
        );
        assert_eq!(classify_renderer("GskCairoRenderer"), RendererClass::Cairo);
        assert_eq!(classify_renderer("FutureRenderer"), RendererClass::Unknown);
    }

    #[test]
    fn unknown_renderer_uses_the_opaque_fallback() {
        let material =
            GlassEnvironment::new(RendererClass::Unknown, true, false).material(GlassTheme::Light);
        assert_eq!(material.mode, GlassMode::FallbackTint);
        assert_eq!(material.tint.alpha, FALLBACK_TINT_ALPHA);
    }
}
