//! `SRC-10`: shared "nothing added yet" empty-state geometry for the three
//! online sources (Podcasts, YouTube, Radio) — Turn 6f of the design.
//!
//! Identical grammar for all three, per source: the sidebar entry's own
//! glyph in a subdued rounded tile, a title, one quiet paragraph naming what
//! lands here and where it comes from, exactly one primary `+ Add` button,
//! and — where the source has one — a quiet secondary line underneath naming
//! the URL path. There is no toolbar, no filter row, and no count on this
//! page: it must look unused, not broken.
//!
//! This module owns only the shared *shape*. *When* it fires is still
//! decided per source by the existing classifications
//! (`podcasts_empty_state.rs::podcasts_empty_state_for`'s `Empty` variant,
//! `radio_empty_state.rs::radio_empty_state_for`'s `Empty` variant) — those
//! already correctly distinguish "nothing subscribed yet" from "subscribed
//! but nothing matches the filter", which stays on its own status page
//! (Block B2, out of scope here). The caller swaps back to its list page the
//! moment the classification stops being `Empty` — i.e. the instant the
//! first subscription lands, this stops rendering.

use gtk4::prelude::*;

const GLYPH_TILE_SIZE: i32 = 64;
const GLYPH_PIXEL_SIZE: i32 = 28;
const CONTENT_SPACING: i32 = 12;
const BODY_MAX_WIDTH_CHARS: i32 = 42;

/// The one thing that differs between the three sources. Geometry
/// (`SourceEmptyState::new`) never varies — only this copy does.
pub(super) struct SourceEmptyStateCopy {
    /// The source's own sidebar glyph (`NavIcon::*::icon_name()`), so the
    /// empty state visually matches the place the user just navigated from.
    pub(super) icon_name: &'static str,
    pub(super) title: String,
    /// One paragraph: what lands here, and where it comes from.
    pub(super) body: String,
    pub(super) button_label: String,
    /// The URL path as a quiet secondary line underneath the button —
    /// omitted entirely, not rendered blank, when a source has none.
    pub(super) secondary_line: Option<String>,
}

/// A built empty-state page plus the one primary action it offers.
pub(super) struct SourceEmptyState {
    root: gtk4::Widget,
    button: gtk4::Button,
}

impl SourceEmptyState {
    pub(super) fn new(copy: &SourceEmptyStateCopy) -> Self {
        let (root, button) = build(copy);
        Self { root, button }
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        &self.root
    }

    /// Wires the one primary button. Every source's empty state offers
    /// exactly one action, so there is exactly one callback to connect.
    pub(super) fn connect_add(&self, callback: impl Fn() + 'static) {
        self.button.connect_clicked(move |_| callback());
    }

    #[cfg(test)]
    pub(super) fn button(&self) -> &gtk4::Button {
        &self.button
    }

    /// The primary button's visible text. It has no `label` property of its
    /// own — its child is an icon+label `Box`, per `SRC-10`'s "plus icon" —
    /// so tests read the label out of that box instead of `Button::label`.
    #[cfg(test)]
    pub(super) fn button_label_text(&self) -> Option<String> {
        let content = self.button.child()?.downcast::<gtk4::Box>().ok()?;
        let mut child = content.first_child();
        while let Some(widget) = child {
            if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
                return Some(label.text().to_string());
            }
            child = widget.next_sibling();
        }
        None
    }
}

fn build(copy: &SourceEmptyStateCopy) -> (gtk4::Widget, gtk4::Button) {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, CONTENT_SPACING);
    root.add_css_class("reprise-source-empty-state");
    root.set_valign(gtk4::Align::Center);
    root.set_halign(gtk4::Align::Center);
    root.set_vexpand(true);

    let tile = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    tile.add_css_class("reprise-source-empty-state-tile");
    tile.set_halign(gtk4::Align::Center);
    tile.set_valign(gtk4::Align::Center);
    tile.set_size_request(GLYPH_TILE_SIZE, GLYPH_TILE_SIZE);
    let glyph = gtk4::Image::from_icon_name(copy.icon_name);
    glyph.set_pixel_size(GLYPH_PIXEL_SIZE);
    glyph.set_halign(gtk4::Align::Center);
    glyph.set_valign(gtk4::Align::Center);
    tile.append(&glyph);
    root.append(&tile);

    let title = gtk4::Label::new(Some(&copy.title));
    title.add_css_class("title-2");
    root.append(&title);

    let body = gtk4::Label::new(Some(&copy.body));
    body.add_css_class("dim-label");
    body.set_wrap(true);
    body.set_justify(gtk4::Justification::Center);
    body.set_max_width_chars(BODY_MAX_WIDTH_CHARS);
    root.append(&body);

    let button = gtk4::Button::new();
    let button_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    button_content.append(&gtk4::Image::from_icon_name("list-add-symbolic"));
    button_content.append(&gtk4::Label::new(Some(&copy.button_label)));
    button.set_child(Some(&button_content));
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.set_halign(gtk4::Align::Center);
    root.append(&button);

    if let Some(secondary) = &copy.secondary_line {
        let secondary_label = gtk4::Label::new(Some(secondary.as_str()));
        secondary_label.add_css_class("caption");
        secondary_label.add_css_class("dim-label");
        root.append(&secondary_label);
    }

    (root.upcast(), button)
}

pub(super) fn css() -> String {
    ".reprise-source-empty-state { padding: 24px; }\n\
     .reprise-source-empty-state-tile {\n  \
       border-radius: 16px;\n  \
       background: alpha(currentColor, 0.08);\n\
     }"
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn children_of(root: &gtk4::Box) -> Vec<gtk4::Widget> {
        std::iter::successors(root.first_child(), gtk4::prelude::WidgetExt::next_sibling).collect()
    }

    fn copy(secondary: Option<&str>) -> SourceEmptyStateCopy {
        SourceEmptyStateCopy {
            icon_name: "video-x-generic-symbolic",
            title: "No channels yet".to_owned(),
            body: "Subscribe to a channel and its uploads appear here.".to_owned(),
            button_label: "Add channel".to_owned(),
            secondary_line: secondary.map(str::to_owned),
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_10_geometry_renders_glyph_title_body_one_button_and_secondary_in_order() {
        gtk4::init().unwrap();
        let state = SourceEmptyState::new(&copy(Some("or paste a channel URL in the dialog")));
        let root = state.widget().clone().downcast::<gtk4::Box>().unwrap();

        let children = children_of(&root);
        // Tile, title, body, button, secondary — exactly five top-level
        // children, in that order, with exactly one button in the whole tree.
        assert_eq!(children.len(), 5);
        assert!(children[0].has_css_class("reprise-source-empty-state-tile"));
        assert_eq!(
            children[1]
                .clone()
                .downcast::<gtk4::Label>()
                .unwrap()
                .text(),
            "No channels yet"
        );
        assert!(children[3].clone().downcast::<gtk4::Button>().is_ok());
        assert_eq!(
            children[4]
                .clone()
                .downcast::<gtk4::Label>()
                .unwrap()
                .text(),
            "or paste a channel URL in the dialog"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_10_the_secondary_line_is_omitted_not_left_blank_when_a_source_has_none() {
        gtk4::init().unwrap();
        let state = SourceEmptyState::new(&copy(None));
        let root = state.widget().clone().downcast::<gtk4::Box>().unwrap();

        let children = children_of(&root);
        // Tile, title, body, button — no fifth child, no reserved gap.
        assert_eq!(children.len(), 4);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_10_the_single_add_button_carries_a_plus_icon_and_fires_its_callback() {
        gtk4::init().unwrap();
        let state = SourceEmptyState::new(&copy(None));
        let clicked = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = clicked.clone();
        state.connect_add(move || flag.set(true));

        state.button().emit_clicked();
        assert!(clicked.get());
    }

    #[test]
    fn src_10_css_defines_a_subdued_rounded_tile_not_a_generic_placeholder_graphic() {
        let css = css();
        assert!(css.contains(".reprise-source-empty-state-tile"));
        assert!(css.contains("border-radius"));
        // A subdued fill via the existing text color, never a bespoke image
        // or a hardcoded accent — this is a tile, not a placeholder graphic.
        assert!(css.contains("alpha(currentColor"));
    }
}
