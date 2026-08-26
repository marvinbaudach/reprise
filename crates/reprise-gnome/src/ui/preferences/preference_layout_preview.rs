//! The interactive layout preview at the top of the Layout preferences page.
//!
//! One coherent mini window instead of two static choice cards: every region
//! of the library window is drawn and directly clickable, and the switches
//! below mirror the same state. The widget owns no state of its own — it
//! renders a [`LayoutPreviewState`] and reports a requested state back through
//! its callback, so the page stays the single place that saves and rolls back.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::settings::PlayerBarPosition;

use super::preference_visual_strings as visual_strings;

/// The framed mini window.
pub(in crate::ui) const PREVIEW_CLASS: &str = "reprise-choice-preview";
/// The window's own title strip — decoration, never clickable.
pub(in crate::ui) const TITLEBAR_CLASS: &str = "reprise-preview-titlebar";
/// A clickable region.
pub(in crate::ui) const ZONE_CLASS: &str = "reprise-preview-zone";
/// The dashed stand-in a hidden region leaves behind.
pub(in crate::ui) const GHOST_CLASS: &str = "reprise-preview-ghost";
const SIDEBAR_CLASS: &str = "reprise-preview-sidebar";
const CONTENT_CLASS: &str = "reprise-preview-content";
const PLAYER_CLASS: &str = "reprise-preview-player";
const LABEL_CLASS: &str = "reprise-preview-label";
const WINDOW_TITLE: &str = "Reprise";

const PREVIEW_HEIGHT: i32 = 290;
const TITLEBAR_HEIGHT: i32 = 26;
const PLAYER_BAR_HEIGHT: i32 = 38;
const FILTER_BAR_HEIGHT: i32 = 30;
const STATUS_BAR_HEIGHT: i32 = 20;
const SIDEBAR_WIDTH: i32 = 96;
const DETAILS_WIDTH: i32 = 104;
/// Hidden side regions keep a narrow strip, hidden bars a low one, so the
/// window never loses the place the region will come back to.
const GHOST_WIDTH: i32 = 34;
const GHOST_BAR_HEIGHT: i32 = 22;

const SIDEBAR_ROW_WIDTHS: [i32; 4] = [80, 60, 70, 50];
const TRACK_ROW_WIDTHS: [i32; 6] = [180, 145, 200, 115, 165, 135];
const FILTER_CHIP_WIDTHS: [i32; 3] = [40, 30, 34];

/// Everything the preview draws, and everything the switches below mirror.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct LayoutPreviewState {
    pub(in crate::ui) bar: PlayerBarPosition,
    pub(in crate::ui) sidebar: bool,
    pub(in crate::ui) browse: bool,
    pub(in crate::ui) info: bool,
    pub(in crate::ui) status: bool,
}

impl LayoutPreviewState {
    /// The state "Restore defaults" writes back.
    pub(in crate::ui) fn defaults() -> Self {
        Self {
            bar: PlayerBarPosition::Bottom,
            sidebar: true,
            browse: true,
            info: true,
            status: true,
        }
    }
}

/// The clickable regions. Navigation is always left and Details always right —
/// the sides are not configurable, so there is no position enum here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LayoutRegion {
    PlayerBar,
    NavigationSidebar,
    FilterBar,
    DetailsSidebar,
    StatusBar,
}

/// What a click on `region` asks for. Pure, so the page's wiring is testable
/// without a display.
pub(in crate::ui) fn state_after_click(
    state: LayoutPreviewState,
    region: LayoutRegion,
) -> LayoutPreviewState {
    match region {
        LayoutRegion::PlayerBar => LayoutPreviewState {
            bar: match state.bar {
                PlayerBarPosition::Top => PlayerBarPosition::Bottom,
                PlayerBarPosition::Bottom => PlayerBarPosition::Top,
            },
            ..state
        },
        LayoutRegion::NavigationSidebar => LayoutPreviewState {
            sidebar: !state.sidebar,
            ..state
        },
        LayoutRegion::FilterBar => LayoutPreviewState {
            browse: !state.browse,
            ..state
        },
        LayoutRegion::DetailsSidebar => LayoutPreviewState {
            info: !state.info,
            ..state
        },
        LayoutRegion::StatusBar => LayoutPreviewState {
            status: !state.status,
            ..state
        },
    }
}

fn region_tooltip(region: LayoutRegion, visible: bool) -> String {
    let message = match (region, visible) {
        (LayoutRegion::PlayerBar, _) => visual_strings::MOVE_PLAYER_BAR,
        (LayoutRegion::NavigationSidebar, true) => visual_strings::HIDE_NAVIGATION_SIDEBAR,
        (LayoutRegion::NavigationSidebar, false) => visual_strings::SHOW_NAVIGATION_SIDEBAR,
        (LayoutRegion::FilterBar, true) => visual_strings::HIDE_FILTER_BAR,
        (LayoutRegion::FilterBar, false) => visual_strings::SHOW_FILTER_BAR,
        (LayoutRegion::DetailsSidebar, true) => visual_strings::HIDE_DETAILS_SIDEBAR,
        (LayoutRegion::DetailsSidebar, false) => visual_strings::SHOW_DETAILS_SIDEBAR,
        (LayoutRegion::StatusBar, true) => visual_strings::HIDE_STATUS_BAR,
        (LayoutRegion::StatusBar, false) => visual_strings::SHOW_STATUS_BAR,
    };
    visual_strings::text(message)
}

fn zone_button(region: LayoutRegion, visible: bool) -> gtk4::Button {
    let label = region_tooltip(region, visible);
    let button = gtk4::Button::builder()
        .tooltip_text(&label)
        .has_frame(false)
        .build();
    button.add_css_class(ZONE_CLASS);
    if !visible {
        button.add_css_class(GHOST_CLASS);
    }
    button.update_property(&[gtk4::accessible::Property::Label(&label)]);
    button
}

fn bar_label(message: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(&visual_strings::text(message)));
    label.add_css_class(LABEL_CLASS);
    label
}

fn block(width: i32, height: i32) -> gtk4::Box {
    let block = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    block.set_size_request(width, height);
    block.add_css_class("reprise-preview-block");
    block
}

fn ghost_child(message: &str, with_label: bool) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    content.set_halign(gtk4::Align::Center);
    content.set_valign(gtk4::Align::Center);
    content.append(&gtk4::Image::from_icon_name("list-add-symbolic"));
    if with_label {
        content.append(&bar_label(message));
    }
    content
}

/// The window's own strip: decoration, never a target. The app name is a
/// proper noun and stays untranslated.
fn titlebar() -> gtk4::Box {
    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    bar.add_css_class(TITLEBAR_CLASS);
    bar.set_height_request(TITLEBAR_HEIGHT);
    let name = gtk4::Label::new(Some(WINDOW_TITLE));
    name.add_css_class(LABEL_CLASS);
    name.set_hexpand(true);
    name.set_halign(gtk4::Align::Start);
    bar.append(&name);
    let dots = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    dots.set_valign(gtk4::Align::Center);
    for _ in 0..3 {
        let dot = block(5, 5);
        dot.add_css_class("reprise-preview-dot");
        dots.append(&dot);
    }
    bar.append(&dots);
    bar
}

fn player_bar_zone(visible_label: &str) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.append(&block(22, 22));
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    for icon in [
        "media-skip-backward-symbolic",
        "media-playback-start-symbolic",
        "media-skip-forward-symbolic",
    ] {
        let image = gtk4::Image::from_icon_name(icon);
        image.add_css_class("accent");
        controls.append(&image);
    }
    content.append(&controls);
    let seek = block(-1, 3);
    seek.set_hexpand(true);
    seek.set_valign(gtk4::Align::Center);
    seek.add_css_class("reprise-preview-seek");
    content.append(&seek);
    content.append(&bar_label(visible_label));
    content
}

fn sidebar_zone() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
    content.append(&bar_label(visual_strings::REGION_NAVIGATION));
    for width in SIDEBAR_ROW_WIDTHS {
        content.append(&block(width, 9));
    }
    content
}

fn details_zone() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    content.append(&bar_label(visual_strings::REGION_DETAILS));
    content.append(&block(-1, 46));
    content.append(&block(70, 6));
    content.append(&block(50, 6));
    content
}

fn filter_bar_zone() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    for width in FILTER_CHIP_WIDTHS {
        content.append(&block(width, 12));
    }
    let label = bar_label(visual_strings::FILTER_BAR);
    label.set_hexpand(true);
    label.set_halign(gtk4::Align::End);
    content.append(&label);
    content
}

fn track_list() -> gtk4::Box {
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    list.set_vexpand(true);
    list.add_css_class(CONTENT_CLASS);
    for width in TRACK_ROW_WIDTHS {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        row.append(&block(16, 16));
        row.append(&block(width, 6));
        list.append(&row);
    }
    list
}

/// The preview widget. `render` is the only way its picture changes.
pub(in crate::ui) struct LayoutPreview {
    pub(in crate::ui) root: gtk4::Box,
    on_request: Rc<dyn Fn(LayoutPreviewState)>,
}

impl LayoutPreview {
    pub(in crate::ui) fn new(on_request: Rc<dyn Fn(LayoutPreviewState)>) -> Rc<Self> {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class(PREVIEW_CLASS);
        root.set_height_request(PREVIEW_HEIGHT);
        root.set_overflow(gtk4::Overflow::Hidden);
        Rc::new(Self { root, on_request })
    }

    fn zone(
        self: &Rc<Self>,
        state: LayoutPreviewState,
        region: LayoutRegion,
        visible: bool,
    ) -> gtk4::Button {
        let button = zone_button(region, visible);
        let preview = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            let Some(preview) = preview.upgrade() else {
                return;
            };
            (preview.on_request)(state_after_click(state, region));
        });
        button
    }

    pub(in crate::ui) fn render(self: &Rc<Self>, state: LayoutPreviewState) {
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }

        self.root.append(&titlebar());

        let window = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        window.set_vexpand(true);

        let player = self.zone(state, LayoutRegion::PlayerBar, true);
        player.add_css_class(PLAYER_CLASS);
        player.set_height_request(PLAYER_BAR_HEIGHT);
        player.set_child(Some(&player_bar_zone(visual_strings::PLAYER_BAR)));

        let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        body.set_vexpand(true);

        // Fixed order: Navigation, content, Details.
        let sidebar = self.zone(state, LayoutRegion::NavigationSidebar, state.sidebar);
        if state.sidebar {
            sidebar.add_css_class(SIDEBAR_CLASS);
            sidebar.set_size_request(SIDEBAR_WIDTH, -1);
            sidebar.set_child(Some(&sidebar_zone()));
        } else {
            sidebar.set_size_request(GHOST_WIDTH, -1);
            sidebar.set_child(Some(&ghost_child(visual_strings::REGION_NAVIGATION, false)));
        }
        body.append(&sidebar);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_hexpand(true);
        let filters = self.zone(state, LayoutRegion::FilterBar, state.browse);
        if state.browse {
            filters.set_height_request(FILTER_BAR_HEIGHT);
            filters.set_child(Some(&filter_bar_zone()));
        } else {
            filters.set_height_request(GHOST_BAR_HEIGHT);
            filters.set_child(Some(&ghost_child(visual_strings::FILTER_BAR, true)));
        }
        content.append(&filters);
        content.append(&track_list());
        let status = self.zone(state, LayoutRegion::StatusBar, state.status);
        status.set_height_request(if state.status {
            STATUS_BAR_HEIGHT
        } else {
            GHOST_BAR_HEIGHT
        });
        if state.status {
            let label = bar_label(visual_strings::STATUS_BAR);
            label.set_halign(gtk4::Align::Start);
            status.set_child(Some(&label));
        } else {
            status.set_child(Some(&ghost_child(visual_strings::STATUS_BAR, true)));
        }
        content.append(&status);
        body.append(&content);

        let details = self.zone(state, LayoutRegion::DetailsSidebar, state.info);
        if state.info {
            details.add_css_class(SIDEBAR_CLASS);
            details.set_size_request(DETAILS_WIDTH, -1);
            details.set_child(Some(&details_zone()));
        } else {
            details.set_size_request(GHOST_WIDTH, -1);
            details.set_child(Some(&ghost_child(visual_strings::REGION_DETAILS, false)));
        }
        body.append(&details);

        if state.bar == PlayerBarPosition::Top {
            window.append(&player);
            window.append(&body);
        } else {
            window.append(&body);
            window.append(&player);
        }
        self.root.append(&window);
    }
}

pub(in crate::ui) fn css() -> String {
    use super::style::tokens::{
        ACCENT_TINT_CEILING, PREVIEW_BORDER_ALPHA, PREVIEW_CONTENT_ALPHA, PREVIEW_SIDEBAR_ALPHA,
    };
    format!(
        ".{PREVIEW_CLASS} {{ \
           border: 1px solid alpha(@window_fg_color, {PREVIEW_BORDER_ALPHA}); \
           border-radius: 8px; }} \
         .{TITLEBAR_CLASS} {{ \
           background: alpha(@window_fg_color, {PREVIEW_SIDEBAR_ALPHA}); \
           border-bottom: 1px solid alpha(@window_fg_color, {PREVIEW_BORDER_ALPHA}); \
           padding: 0 10px; }} \
         .{SIDEBAR_CLASS} {{ background: alpha(@window_fg_color, {PREVIEW_SIDEBAR_ALPHA}); }} \
         .{CONTENT_CLASS} {{ \
           background: alpha(@window_fg_color, {PREVIEW_CONTENT_ALPHA}); \
           padding: 8px; }} \
         .{PLAYER_CLASS} {{ background: alpha(@accent_bg_color, {ACCENT_TINT_CEILING}); }} \
         .{ZONE_CLASS} {{ \
           border: 1px solid transparent; \
           border-radius: 0; \
           padding: 8px; \
           min-height: 0; \
           min-width: 0; }} \
         .{ZONE_CLASS}:hover {{ border-color: @accent_color; }} \
         .{ZONE_CLASS}:focus-visible {{ outline: 2px solid @accent_color; outline-offset: -2px; }} \
         .{GHOST_CLASS} {{ \
           border: 1px dashed alpha(@window_fg_color, {PREVIEW_BORDER_ALPHA}); \
           color: alpha(@window_fg_color, 0.55); }} \
         .{GHOST_CLASS}:hover {{ \
           border-color: @accent_color; \
           color: @reprise_accent_text_color; }} \
         .{LABEL_CLASS} {{ \
           font-size: 0.72em; \
           letter-spacing: 0.09em; \
           text-transform: uppercase; \
           opacity: 0.65; }} \
         .reprise-preview-block {{ \
           background: alpha(@window_fg_color, {PREVIEW_SIDEBAR_ALPHA}); \
           border-radius: 2px; }} \
         .reprise-preview-seek {{ background: @accent_bg_color; border-radius: 999px; }} \
         .reprise-preview-dot {{ border-radius: 999px; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_on() -> LayoutPreviewState {
        LayoutPreviewState {
            bar: PlayerBarPosition::Bottom,
            sidebar: true,
            browse: true,
            info: true,
            status: true,
        }
    }

    #[test]
    fn clicking_the_player_bar_swaps_its_edge_and_touches_nothing_else() {
        let moved = state_after_click(all_on(), LayoutRegion::PlayerBar);

        assert_eq!(moved.bar, PlayerBarPosition::Top);
        assert_eq!(
            state_after_click(moved, LayoutRegion::PlayerBar).bar,
            PlayerBarPosition::Bottom
        );
        assert!(moved.sidebar && moved.browse && moved.info && moved.status);
    }

    #[test]
    fn set_16_clicking_a_region_toggles_exactly_that_region() {
        for (region, read) in [
            (
                LayoutRegion::NavigationSidebar,
                (|state: LayoutPreviewState| state.sidebar) as fn(LayoutPreviewState) -> bool,
            ),
            (LayoutRegion::FilterBar, |state| state.browse),
            (LayoutRegion::DetailsSidebar, |state| state.info),
            (LayoutRegion::StatusBar, |state| state.status),
        ] {
            let hidden = state_after_click(all_on(), region);

            assert!(!read(hidden), "{region:?} must hide on the first click");
            assert!(
                read(state_after_click(hidden, region)),
                "{region:?} must come back on the second click"
            );
            assert_eq!(hidden.bar, all_on().bar);
        }
    }

    #[test]
    fn the_defaults_show_every_region_with_the_bar_at_the_bottom() {
        let defaults = LayoutPreviewState::defaults();

        assert_eq!(defaults.bar, PlayerBarPosition::Bottom);
        assert!(defaults.sidebar && defaults.browse && defaults.info && defaults.status);
    }

    #[test]
    fn css_covers_the_zones_and_their_ghosts() {
        let css = css();

        assert!(css.contains(&format!(".{ZONE_CLASS}:hover")));
        assert!(css.contains(&format!(".{ZONE_CLASS}:focus-visible")));
        assert!(css.contains(&format!(".{GHOST_CLASS} {{")));
        assert!(css.contains("border: 1px dashed"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_16_the_bar_sits_at_the_clicked_edge_and_the_body_keeps_its_order() {
        if gtk4::init().is_err() {
            return;
        }
        let preview = LayoutPreview::new(Rc::new(|_| {}));
        preview.render(LayoutPreviewState {
            bar: PlayerBarPosition::Top,
            ..all_on()
        });
        let window = preview.root.last_child().expect("preview renders a window");
        assert!(window
            .first_child()
            .is_some_and(|child| child.has_css_class(PLAYER_CLASS)));

        preview.render(all_on());
        let window = preview.root.last_child().expect("preview renders a window");
        assert!(window
            .last_child()
            .is_some_and(|child| child.has_css_class(PLAYER_CLASS)));

        let body = window.first_child().expect("the body sits next to the bar");
        let sidebar = body.first_child().expect("navigation comes first");
        let details = body.last_child().expect("details come last");
        assert!(sidebar.has_css_class(SIDEBAR_CLASS));
        assert!(details.has_css_class(SIDEBAR_CLASS));
        assert!(!sidebar.has_css_class(GHOST_CLASS));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_16_a_hidden_region_renders_its_ghost_and_clicking_it_asks_for_the_region_back() {
        if gtk4::init().is_err() {
            return;
        }
        let requested = Rc::new(std::cell::RefCell::new(None));
        let sink = requested.clone();
        let preview = LayoutPreview::new(Rc::new(move |state| {
            *sink.borrow_mut() = Some(state);
        }));
        preview.render(LayoutPreviewState {
            sidebar: false,
            ..all_on()
        });
        let window = preview.root.last_child().expect("preview renders a window");
        let body = window.first_child().expect("the body sits next to the bar");
        let ghost = body
            .first_child()
            .and_downcast::<gtk4::Button>()
            .expect("the hidden sidebar leaves a button behind");

        assert!(ghost.has_css_class(GHOST_CLASS));
        ghost.emit_clicked();
        assert_eq!(
            requested.borrow().map(|state| state.sidebar),
            Some(true),
            "clicking the ghost must ask for the sidebar back"
        );
    }
}
