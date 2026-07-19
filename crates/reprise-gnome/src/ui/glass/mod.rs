mod backdrop;
mod insets;
pub(crate) mod material;
mod performance;
mod scroll_inset;
mod surface;

pub(crate) use insets::{mark_top_inset_anchor, SafeInsetApplier, SafeInsets};
#[cfg(test)]
pub(crate) use insets::{InsetMeasurements, PlayerBarEdge};
pub(in crate::ui) use performance::arm as arm_performance_measurement;
#[cfg(test)]
pub(crate) use scroll_inset::ScrollInset;
pub(crate) use surface::{GlassEdge, GlassSurface};

pub(in crate::ui) fn css() -> String {
    ".reprise-glass-surface { background-color: @headerbar_bg_color; }\n\
     .reprise-glass-controls { background-color: transparent; }\n\
     .reprise-glass-controls .subtitle { \
       color: alpha(@window_fg_color, 0.82); }\n\
     .reprise-glass-hairline { min-height: 1px; min-width: 1px; \
       background-color: alpha(@window_fg_color, 0.10); }"
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gtk4::prelude::*;

    use super::material::{GlassEnvironment, GlassMode, GlassTheme, RendererClass};
    use super::performance::{evaluate_pair, FrameSeries, PerfFailure};
    use super::{GlassEdge, GlassSurface};
    use super::{InsetMeasurements, PlayerBarEdge, SafeInsetApplier, SafeInsets};

    fn wait_for_layout() {
        wait_for_layout_for(Duration::from_millis(50));
    }

    fn wait_for_layout_for(duration: Duration) {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(duration, move || quit.quit());
        main_loop.run();
    }

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
    fn glass_css_keeps_a_neutral_fallback_floor_and_one_shared_hairline() {
        let css = super::css();

        assert!(css.contains("background-color: @headerbar_bg_color"));
        assert!(css.contains(".reprise-glass-controls { background-color: transparent;"));
        assert!(css.contains(".reprise-glass-hairline"));
        assert!(css.contains("background-color: alpha(@window_fg_color, 0.10)"));
        assert!(!css.contains("@reprise_player_accent"));
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn glass_backdrop_repaints_when_the_live_source_changes() {
        use std::cell::Cell;
        use std::rc::Rc;

        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().expect("GTK settings after gtk4::init");
        let animations_before = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(true);

        let blue = Rc::new(Cell::new(false));
        let source = gtk4::DrawingArea::new();
        source.set_hexpand(true);
        source.set_vexpand(true);
        let blue_for_draw = blue.clone();
        source.set_draw_func(move |_, context, width, height| {
            if blue_for_draw.get() {
                context.set_source_rgb(0.0, 0.2, 1.0);
            } else {
                context.set_source_rgb(1.0, 0.1, 0.0);
            }
            context.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
            context.fill().unwrap();
        });

        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        controls.set_height_request(80);
        let surface = GlassSurface::new(&source, &controls, GlassEdge::Top);
        surface.root().set_halign(gtk4::Align::Fill);
        surface.root().set_valign(gtk4::Align::Start);

        let root = gtk4::Overlay::new();
        root.set_child(Some(&source));
        root.add_overlay(surface.root());
        let window = gtk4::Window::builder()
            .default_width(400)
            .default_height(240)
            .child(&root)
            .build();
        window.present();
        wait_for_layout_for(Duration::from_millis(100));

        let material = GlassEnvironment::for_widget(surface.backdrop()).material(GlassTheme::Dark);
        assert_eq!(
            material.mode,
            GlassMode::BackdropBlur,
            "this regression needs the hardware Glass path"
        );
        let before = surface.backdrop().snapshot_count();
        blue.set(true);
        source.queue_draw();
        wait_for_layout_for(Duration::from_millis(100));
        let after = surface.backdrop().snapshot_count();

        settings.set_gtk_enable_animations(animations_before);
        window.close();
        assert!(
            after > before,
            "live source invalidation did not schedule another Glass snapshot"
        );
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

        let applier = SafeInsetApplier::discover(&root);
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn top_anchor_keeps_a_fixed_sibling_visible_without_double_padding_the_scroller() {
        gtk4::init().unwrap();
        let fixed_controls = gtk4::Label::new(Some("Filters"));
        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        rows.append(&gtk4::Label::new(Some("First")));
        rows.append(&gtk4::Label::new(Some("Last")));
        let scrolled = gtk4::ScrolledWindow::builder().child(&rows).build();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-glass-top-inset-anchor");
        root.append(&fixed_controls);
        root.append(&scrolled);

        let applier = SafeInsetApplier::discover(&root);
        applier.apply(SafeInsets {
            top: 90,
            bottom: 96,
        });

        assert_eq!(root.margin_top(), 90);
        assert_eq!(rows.margin_top(), 0);
        assert_eq!(rows.margin_bottom(), 96);
        assert_eq!(scrolled.margin_bottom(), 0);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn inset_applier_follows_a_swapped_scrolled_child() {
        gtk4::init().unwrap();
        let original = gtk4::Label::new(Some("original"));
        let scrolled = gtk4::ScrolledWindow::builder().child(&original).build();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&scrolled);

        let applier = SafeInsetApplier::discover(&root);
        let insets = SafeInsets {
            top: 90,
            bottom: 96,
        };
        applier.apply(insets);
        assert_eq!(original.margin_top(), 90);

        let replacement = gtk4::Label::new(Some("replacement"));
        replacement.set_margin_top(3);
        replacement.set_margin_bottom(4);
        scrolled.set_child(Some(&replacement));

        // Re-applying equal insets must discover the replacement and must not
        // compound its own margins.
        applier.apply(insets);
        assert_eq!(replacement.margin_top(), 93);
        assert_eq!(replacement.margin_bottom(), 100);
        applier.apply(insets);
        assert_eq!(replacement.margin_top(), 93);
        assert_eq!(replacement.margin_bottom(), 100);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn native_grid_content_traverses_both_glass_zones_without_losing_virtualization() {
        gtk4::init().unwrap();
        let strings = gtk4::StringList::new(
            &(0..100)
                .map(|index| format!("Album {index}"))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        let selection = gtk4::NoSelection::new(Some(strings));
        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
            item.set_child(Some(&gtk4::Label::new(None)));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
            let label = item.child().and_downcast::<gtk4::Label>().unwrap();
            let value = item.item().and_downcast::<gtk4::StringObject>().unwrap();
            label.set_label(&value.string());
        });
        let grid = gtk4::GridView::new(Some(selection), Some(factory));
        grid.set_min_columns(2);
        grid.set_max_columns(2);
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&grid)
            .min_content_height(240)
            .build();
        let applier = SafeInsetApplier::discover(&scrolled);
        let inset = scrolled
            .child()
            .and_downcast::<super::scroll_inset::ScrollInset>()
            .expect("native scrollables must keep a direct GtkScrollable adapter");
        assert_eq!(inset.child().as_ref(), Some(grid.upcast_ref()));

        let window = gtk4::Window::builder()
            .default_width(400)
            .default_height(240)
            .child(&scrolled)
            .build();
        window.present();
        wait_for_layout();
        applier.apply(SafeInsets {
            top: 90,
            bottom: 96,
        });
        wait_for_layout();

        assert_eq!(grid.margin_top(), 0);
        assert_eq!(grid.margin_bottom(), 0);
        let outer = scrolled.vadjustment();
        let inner = grid.vadjustment().unwrap();
        let inner_max = (inner.upper() - inner.page_size()).max(inner.lower());
        assert_eq!(outer.lower(), inner.lower() - 90.0);
        assert_eq!(outer.upper() - outer.page_size(), inner_max + 96.0);

        outer.set_value(outer.lower());
        wait_for_layout();
        let start = grid
            .compute_point(&inset, &gtk4::graphene::Point::new(0.0, 0.0))
            .unwrap();
        assert!((start.y() - 90.0).abs() <= 1.0);

        outer.set_value(inner.lower());
        wait_for_layout();
        let normal = grid
            .compute_point(&inset, &gtk4::graphene::Point::new(0.0, 0.0))
            .unwrap();
        assert!(normal.y().abs() <= 1.0);

        outer.set_value(outer.upper() - outer.page_size());
        wait_for_layout();
        let end = grid
            .compute_point(&inset, &gtk4::graphene::Point::new(0.0, 0.0))
            .unwrap();
        assert!((end.y() + 96.0).abs() <= 1.0);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn width_dependent_grid_keeps_full_allocation_after_view_stack_reveal() {
        gtk4::init().unwrap();
        let strings = gtk4::StringList::new(
            &(0..120)
                .map(|index| format!("Album {index:05}"))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        let selection = gtk4::NoSelection::new(Some(strings));
        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
            let label = gtk4::Label::new(None);
            label.set_size_request(180, 180);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
            let label = item.child().and_downcast::<gtk4::Label>().unwrap();
            let value = item.item().and_downcast::<gtk4::StringObject>().unwrap();
            label.set_label(&value.string());
        });
        let grid = gtk4::GridView::new(Some(selection), Some(factory));
        grid.set_min_columns(1);
        grid.set_max_columns(200);
        let albums = gtk4::ScrolledWindow::builder()
            .child(&grid)
            .vexpand(true)
            .hexpand(true)
            .build();
        let tracks = gtk4::Label::new(Some("Tracks"));
        tracks.set_vexpand(true);
        let stack = libadwaita::ViewStack::builder()
            .hhomogeneous(false)
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .build();
        stack.add_named(&tracks, Some("tracks"));
        stack.add_named(&albums, Some("albums"));
        stack.set_visible_child_name("tracks");

        let applier = SafeInsetApplier::discover(&stack);
        applier.apply(SafeInsets {
            top: 90,
            bottom: 96,
        });
        let inset = albums
            .child()
            .and_downcast::<super::scroll_inset::ScrollInset>()
            .unwrap();
        assert_eq!(inset.request_mode(), grid.request_mode());

        let window = gtk4::Window::builder()
            .default_width(800)
            .default_height(600)
            .child(&stack)
            .build();
        window.present();
        wait_for_layout();
        stack.set_visible_child_name("albums");
        wait_for_layout_for(Duration::from_millis(u64::from(
            crate::ui::motion::STANDARD_MS + 50,
        )));

        let geometry = format!(
            "window={} stack={} scroller={} inset={} grid={} transition_running={}",
            window.height(),
            stack.height(),
            albums.height(),
            inset.height(),
            grid.height(),
            stack.is_transition_running(),
        );

        assert!(
            stack.height() > 0,
            "album page received no viewport height: {geometry}"
        );
        assert_eq!(
            albums.height(),
            stack.height(),
            "album scroller lost viewport height: {geometry}"
        );
        assert_eq!(
            inset.height(),
            albums.height(),
            "scroll adapter collapsed after hidden-page reveal: {geometry}"
        );
        assert_eq!(
            grid.height(),
            inset.height(),
            "width-dependent grid collapsed after hidden-page reveal: {geometry}"
        );
        window.close();
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
