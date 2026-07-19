#[allow(dead_code)] // Wired into the library shell in the activation task.
mod backdrop;
pub(crate) mod material;
#[allow(dead_code)] // Wired into the library shell in the activation task.
mod surface;

#[allow(unused_imports)] // Wired into the library shell in the activation task.
pub(crate) use surface::{GlassEdge, GlassSurface};

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::material::{GlassEnvironment, GlassMode, GlassTheme, RendererClass};
    use super::{GlassEdge, GlassSurface};

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

    #[test]
    fn glass_edges_are_explicit_and_symmetric() {
        assert_eq!(GlassEdge::Top.css_class(), "reprise-glass-edge-top");
        assert_eq!(GlassEdge::Bottom.css_class(), "reprise-glass-edge-bottom");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn glass_surface_layers_controls_above_the_backdrop() {
        gtk4::init().unwrap();
        let source = gtk4::Label::new(Some("scrolling content"));
        let controls = gtk4::Button::with_label("Play");
        let surface = GlassSurface::new(&source, &controls, GlassEdge::Top);

        assert_eq!(
            surface.root().child().as_ref(),
            Some(surface.backdrop().upcast_ref())
        );
        assert!(controls.is_ancestor(surface.root()));
        assert!(surface.root().is_measure_overlay(&controls));
        assert!(!surface.backdrop().can_target());
    }
}
