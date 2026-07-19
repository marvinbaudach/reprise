#[expect(
    dead_code,
    reason = "the renderer material is consumed by the glass compositor in the next task"
)]
pub(crate) mod material;

#[cfg(test)]
mod tests {
    use super::material::{GlassEnvironment, GlassMode, GlassTheme, RendererClass};

    #[test]
    fn style_4_glass_material_is_neutral_and_falls_back_safely() {
        let hardware = GlassEnvironment::new(RendererClass::Hardware, true, false);
        let cairo = GlassEnvironment::new(RendererClass::Cairo, true, false);
        let reduced_motion = GlassEnvironment::new(RendererClass::Hardware, false, false);
        let high_contrast = GlassEnvironment::new(RendererClass::Hardware, true, true);

        let dark = hardware.material(GlassTheme::Dark);
        assert_eq!(dark.mode, GlassMode::BackdropBlur);
        assert_eq!(dark.blur_radius, 24.0);
        assert!(dark.tint.is_neutral());
        assert_eq!(dark.tint.alpha, 0.80);

        for environment in [cairo, reduced_motion, high_contrast] {
            let fallback = environment.material(GlassTheme::Dark);
            assert_eq!(fallback.mode, GlassMode::FallbackTint);
            assert!(fallback.tint.is_neutral());
            assert!(fallback.tint.alpha >= 0.94);
        }
    }

    #[test]
    fn contrast_4_active_glass_content_meets_worst_case_ratio() {
        for theme in [GlassTheme::Light, GlassTheme::Dark] {
            for renderer in [RendererClass::Hardware, RendererClass::Cairo] {
                let material = GlassEnvironment::new(renderer, true, false).material(theme);
                assert!(material.worst_case_primary_contrast() >= 4.5);
                assert!(material.worst_case_secondary_contrast() >= 4.5);
            }
        }
    }
}
