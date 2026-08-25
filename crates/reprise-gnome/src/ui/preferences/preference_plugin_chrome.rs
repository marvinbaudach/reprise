//! Row chrome for the Plugins page.
//!
//! `docs/plans/plugins-online-content-master-hierarchy.md`, third draft. The
//! second draft dissolved the card and moved the expander chevron into a
//! reserved gutter left of the title, so that row titles and group headings
//! shared one left edge. The third draft puts the card back — the five online
//! plugins are explicitly asked for as *one* card with hairlines between the
//! rows — and with it the gutter's whole reason to exist: subordination is now
//! carried by the 18px indent and the rail, and the group headings no longer
//! share an edge with an indented card. So the chevron returns to libadwaita's
//! own trailing slot, and the only alignment work left is the one thing that
//! keeps every switch on one right edge: rows without a chevron reserve its
//! width (`SET-14b`).
//!
//! What stays here is the page's own chrome: the expanded settings of a plugin
//! must read as *contents of that plugin*, one step further in and on a
//! slightly lifted surface, not as further plugins floating at the same level.

use gtk4::prelude::*;
use libadwaita as adw;

/// Set on the Plugins page; every rule in [`css`] is scoped to it.
pub(in crate::ui) const PLUGINS_PAGE_CLASS: &str = "reprise-plugin-page";
/// Set on every expandable plugin row, so its nested settings can be addressed
/// without depending on a libadwaita-internal style class.
pub(in crate::ui) const EXPANDER_ROW_CLASS: &str = "reprise-plugin-expander";
/// Set on every settings row nested below a plugin.
const NESTED_ROW_CLASS: &str = "reprise-plugin-nested-row";

/// How far a plugin's own settings sit inside its row.
const NESTED_INDENT_PX: u32 = 24;
/// The lift that marks the nested surface as belonging to the row above it.
const NESTED_SURFACE_ALPHA: f32 = 0.21;

/// A chevron-sized hole on a row that never expands.
///
/// libadwaita puts the expander arrow *after* the enable area, so a plain
/// switch row is one arrow narrower than an expander row and its switch would
/// sit further right. The placeholder gives it the same trailing width.
pub(in crate::ui) fn switch_alignment_placeholder() -> gtk4::Image {
    gtk4::Image::builder()
        .icon_name("pan-down-symbolic")
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .opacity(0.0)
        .can_target(false)
        .can_focus(false)
        .build()
}

/// Marks an expandable plugin row so [`css`] can reach its nested settings.
pub(in crate::ui) fn mark_expander(row: &adw::ExpanderRow) {
    row.add_css_class(EXPANDER_ROW_CLASS);
}

/// Adds one settings row at the visual depth promised by `SET-11a`.
pub(in crate::ui) fn add_nested_row(expander: &adw::ExpanderRow, row: &impl IsA<gtk4::Widget>) {
    row.as_ref().add_css_class(NESTED_ROW_CLASS);
    row.as_ref().set_margin_start(NESTED_INDENT_PX as i32);
    libadwaita::prelude::ExpanderRowExt::add_row(expander, row);
}

pub(in crate::ui) fn css() -> String {
    format!(
        "/* --- Plugins rows: a plugin's settings live inside the plugin --- */ \
         .{PLUGINS_PAGE_CLASS} .{EXPANDER_ROW_CLASS} .{NESTED_ROW_CLASS} {{ \
           background-color: alpha(@window_fg_color, {NESTED_SURFACE_ALPHA}); }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use libadwaita::prelude::*;

    const MIN_NESTED_TITLE_INDENT_PX: f32 = 16.0;
    /// The fixture floor is calibrated against the complete Preferences
    /// dialog in CUA, where it keeps the resting parent and child surfaces at
    /// least 48 total RGB steps apart without turning the child into a card.
    const MIN_NESTED_SURFACE_RGB_DISTANCE: u16 = 84;

    fn find_label(root: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
        if let Ok(label) = root.clone().downcast::<gtk4::Label>() {
            if label.label() == text {
                return Some(label);
            }
        }
        let mut child = root.first_child();
        while let Some(current) = child {
            if let Some(label) = find_label(&current, text) {
                return Some(label);
            }
            child = current.next_sibling();
        }
        None
    }

    struct RenderedWidget {
        width: i32,
        height: i32,
        stride: usize,
        pixels: Vec<u8>,
    }

    impl RenderedWidget {
        fn rgb_at(&self, x: i32, y: i32) -> [u8; 3] {
            assert!((0..self.width).contains(&x), "sample x {x} outside widget");
            assert!((0..self.height).contains(&y), "sample y {y} outside widget");
            let offset = y as usize * self.stride + x as usize * 4;
            self.pixels[offset..offset + 3]
                .try_into()
                .expect("one RGB pixel")
        }
    }

    fn render_widget(window: &gtk4::Window, widget: &impl IsA<gtk4::Widget>) -> RenderedWidget {
        let width = widget.width();
        let height = widget.height();
        let paintable = gtk4::WidgetPaintable::new(Some(widget));
        let snapshot = gtk4::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
        let node = snapshot
            .to_node()
            .expect("the allocated Plugins fixture paints a node");
        let renderer = window
            .native()
            .and_then(|native| native.renderer())
            .expect("the presented window has a renderer");
        let texture = renderer.render_texture(&node, None);
        let stride = texture.width() as usize * 4;
        let mut pixels = vec![0; stride * texture.height() as usize];
        texture.download(&mut pixels, stride);
        RenderedWidget {
            width: texture.width(),
            height: texture.height(),
            stride,
            pixels,
        }
    }

    #[test]
    fn set_11a_expanded_settings_read_as_contents_of_their_plugin() {
        let css = css();

        assert!(css.contains(&format!(".{EXPANDER_ROW_CLASS} .{NESTED_ROW_CLASS}")));
        assert!(css.contains(&format!(
            "background-color: alpha(@window_fg_color, {NESTED_SURFACE_ALPHA})"
        )));
    }

    #[test]
    fn set_11a_the_card_and_its_hairlines_are_left_to_libadwaita() {
        let css = css();

        // The second draft's overrides are gone: a boxed list already is the
        // one card with hairlines the third draft asks for.
        assert!(!css.contains("background-color: transparent"));
        assert!(!css.contains("border-radius: 0"));
        assert!(!css.contains("expander-row-arrow"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_11a_expanded_settings_are_visibly_nested_below_the_plugin() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let style_manager = libadwaita::StyleManager::default();
        let previous_scheme = style_manager.color_scheme();
        style_manager.set_color_scheme(libadwaita::ColorScheme::ForceDark);
        crate::ui::style::install();

        let plugin = adw::ExpanderRow::builder().title("New Releases").build();
        mark_expander(&plugin);
        let nested = adw::ActionRow::builder().title("Artists").build();
        add_nested_row(&plugin, &nested);
        plugin.set_expanded(true);

        let group = adw::PreferencesGroup::new();
        group.add(&plugin);
        let page = adw::PreferencesPage::new();
        page.add_css_class(PLUGINS_PAGE_CLASS);
        page.add(&group);
        let window = gtk4::Window::builder()
            .default_width(640)
            .default_height(360)
            .child(&page)
            .build();
        window.present();
        assert!(crate::ui::test_settle::settle_until_mapped(&window));

        let plugin_title = find_label(plugin.upcast_ref(), "New Releases")
            .expect("the plugin title must be rendered");
        let nested_title =
            find_label(plugin.upcast_ref(), "Artists").expect("the nested title must be rendered");
        let plugin_title_bounds = plugin_title
            .compute_bounds(&page)
            .expect("the plugin title must be allocated");
        let nested_title_bounds = nested_title
            .compute_bounds(&page)
            .expect("the nested title must be allocated");
        let title_indent = nested_title_bounds.x() - plugin_title_bounds.x();
        assert!(
            title_indent >= MIN_NESTED_TITLE_INDENT_PX,
            "the nested title is only {title_indent}px inside the plugin; expected at least \
             {MIN_NESTED_TITLE_INDENT_PX}px"
        );

        let plugin_bounds = plugin
            .compute_bounds(&page)
            .expect("the expanded plugin must be allocated");
        let sample_x = (plugin_bounds.x() + plugin_bounds.width() * 0.55).round() as i32;
        let plugin_y =
            (plugin_title_bounds.y() + plugin_title_bounds.height() / 2.0).round() as i32;
        let nested_y =
            (nested_title_bounds.y() + nested_title_bounds.height() / 2.0).round() as i32;
        let rendered = render_widget(&window, &page);
        let plugin_rgb = rendered.rgb_at(sample_x, plugin_y);
        let nested_rgb = rendered.rgb_at(sample_x, nested_y);
        let surface_distance: u16 = plugin_rgb
            .into_iter()
            .zip(nested_rgb)
            .map(|(plugin, nested)| u16::from(plugin.abs_diff(nested)))
            .sum();
        assert!(
            surface_distance >= MIN_NESTED_SURFACE_RGB_DISTANCE,
            "expanded settings surface {nested_rgb:?} is only {surface_distance} RGB steps away \
             from plugin surface {plugin_rgb:?}; expected at least \
             {MIN_NESTED_SURFACE_RGB_DISTANCE}"
        );

        window.close();
        style_manager.set_color_scheme(previous_scheme);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_14b_the_alignment_placeholder_is_presentation_only() {
        gtk4::init().unwrap();
        let placeholder = switch_alignment_placeholder();

        assert_eq!(
            placeholder.accessible_role(),
            gtk4::AccessibleRole::Presentation
        );
        assert_eq!(placeholder.opacity(), 0.0);
        assert!(!placeholder.can_target());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_plugin_chrome_css_parses_without_gtk_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&css());
        assert!(
            errors.is_empty(),
            "GTK reported CSS parsing errors: {errors:?}"
        );
    }
}
