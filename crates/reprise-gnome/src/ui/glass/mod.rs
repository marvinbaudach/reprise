#[allow(dead_code)] // Wired into the library shell in the activation task.
mod backdrop;
#[allow(dead_code)] // Wired into the library shell in the activation task.
mod insets;
pub(crate) mod material;
#[allow(dead_code)] // Consumed by the paired render-cost runner in the analysis task.
mod performance;
#[allow(dead_code)] // Wired into the library shell in the activation task.
mod surface;

#[allow(unused_imports)] // Wired into the library shell in the activation task.
pub(crate) use insets::{InsetMeasurements, PlayerBarEdge, SafeInsetApplier, SafeInsets};
#[allow(unused_imports)] // Wired into the library shell in the activation task.
pub(crate) use surface::{GlassEdge, GlassSurface};

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::material::{GlassEnvironment, GlassMode, GlassTheme, RendererClass};
    use super::performance::{evaluate_pair, FrameSeries, PerfFailure};
    use super::{GlassEdge, GlassSurface};
    use super::{InsetMeasurements, PlayerBarEdge, SafeInsetApplier, SafeInsets};

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

    #[test]
    fn play_7a_glass_shell_overlays_content_with_exact_safe_insets() {
        let top = InsetMeasurements {
            header: 48,
            search: 42,
            player: 96,
            player_edge: PlayerBarEdge::Top,
        };
        assert_eq!(
            top.safe_insets(),
            SafeInsets {
                top: 186,
                bottom: 0
            }
        );

        let bottom = InsetMeasurements {
            player_edge: PlayerBarEdge::Bottom,
            ..top
        };
        assert_eq!(
            bottom.safe_insets(),
            SafeInsets {
                top: 90,
                bottom: 96
            }
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn inset_applier_adds_exact_padding_to_every_scrolled_child() {
        gtk4::init().unwrap();
        let first_child = gtk4::Label::new(Some("first"));
        first_child.set_margin_top(3);
        first_child.set_margin_bottom(4);
        let second_child = gtk4::Label::new(Some("second"));
        let first = gtk4::ScrolledWindow::builder().child(&first_child).build();
        let second = gtk4::ScrolledWindow::builder().child(&second_child).build();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&first);
        root.append(&second);

        let mut applier = SafeInsetApplier::discover(&root);
        assert_eq!(applier.target_count(), 2);
        applier.apply(SafeInsets {
            top: 90,
            bottom: 96,
        });
        assert_eq!(first_child.margin_top(), 93);
        assert_eq!(first_child.margin_bottom(), 100);
        assert_eq!(second_child.margin_top(), 90);
        assert_eq!(second_child.margin_bottom(), 96);
    }

    #[test]
    fn glass_performance_gate_is_fail_closed() {
        let baseline = FrameSeries::new(vec![12_000; 120]);
        let passing = FrameSeries::new(vec![14_500; 120]);
        assert!(evaluate_pair(&baseline, &passing).is_ok());

        assert_eq!(
            evaluate_pair(&FrameSeries::new(vec![12_000; 119]), &passing),
            Err(PerfFailure::TooFewFrames)
        );
        assert_eq!(
            evaluate_pair(&baseline, &FrameSeries::new(vec![20_500; 120])),
            Err(PerfFailure::P95Budget)
        );
        let mut stalled = vec![14_000; 120];
        stalled[42] = 50_001;
        assert_eq!(
            evaluate_pair(&baseline, &FrameSeries::new(stalled)),
            Err(PerfFailure::SingleFrameBudget)
        );
        assert_eq!(
            evaluate_pair(&baseline, &FrameSeries::new(vec![15_001; 120])),
            Err(PerfFailure::OverheadBudget)
        );
    }
}
