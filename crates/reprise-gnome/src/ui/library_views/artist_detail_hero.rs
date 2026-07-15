//! The Artists detail-pane hero: eyebrow, 34px name, meta line, 132px gradient
//! avatar, an accent glow surface, and the Play all / Shuffle / ⋮ action row.
//!
//! Split from `artist_detail_pane.rs` so the pane file stays focused on the
//! rebuild orchestration (albums + top tracks + async accent). The hero
//! widgets persist across artist switches — `show_artist` only updates their
//! text, the avatar gradient, and the glow color.

use gtk4::gio;
use gtk4::prelude::*;

use crate::ui::artist_avatar;
use crate::ui::artist_detail_pane::{ArtistCallback, HeroCallbacks};
use crate::ui::strings;
use crate::ui::style::cover_accent::Rgb;
use reprise_core::library::artist_detail::ArtistHeader;

/// Avatar edge length, per the design mockup.
const AVATAR_SIZE: i32 = 132;

/// The persistent hero widgets, updated in place on each artist switch.
pub(super) struct Hero {
    root: gtk4::Widget,
    name: gtk4::Label,
    meta: gtk4::Label,
    initials: gtk4::Label,
    avatar_css: gtk4::CssProvider,
    glow_css: gtk4::CssProvider,
}

impl Hero {
    /// The hero's top-level widget, appended into the pane column.
    pub(super) fn widget(&self) -> &gtk4::Widget {
        &self.root
    }

    /// The hero name label text — the pane's `#[cfg(test)] hero_name` reads this.
    pub(super) fn name_text(&self) -> String {
        self.name.text().to_string()
    }

    /// Updates the hero for `artist` with its aggregate `header`.
    pub(super) fn update(&self, artist: &str, header: &ArtistHeader) {
        self.name.set_text(artist);
        self.meta.set_text(&strings::artist_detail_meta(
            header.album_count,
            header.track_count,
            header.catalog_ms,
            header.plays_this_year,
        ));
        self.initials.set_text(&artist_avatar::initials(artist));
        self.avatar_css
            .load_from_string(&avatar_gradient_css(artist));
    }

    /// Sets the glow surface to a static color fill derived from `accent`
    /// (v1: no live blur, no cross-fade).
    pub(super) fn set_glow_accent(&self, accent: Rgb) {
        self.glow_css.load_from_string(&glow_css(accent));
    }

    /// Clears the glow (used for artists without any album cover to sample).
    pub(super) fn clear_glow(&self) {
        self.glow_css.load_from_string("");
    }
}

/// Builds the hero. The Play all / Shuffle buttons and the ⋮ menu actions read
/// the pane's current-artist cell at click time and invoke `callbacks`.
pub(super) fn build_hero(callbacks: &HeroCallbacks) -> Hero {
    let glow = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    glow.add_css_class("artist-hero-glow");
    glow.set_hexpand(true);
    glow.set_vexpand(true);
    let glow_css = gtk4::CssProvider::new();
    attach_provider(&glow, &glow_css);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
    content.add_css_class("artist-hero");

    let (avatar, initials, avatar_css) = build_avatar();
    content.append(&avatar);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    column.set_valign(gtk4::Align::Center);
    column.set_hexpand(true);

    let eyebrow = gtk4::Label::new(Some(&strings::text(strings::ARTIST_DETAIL_EYEBROW)));
    eyebrow.set_xalign(0.0);
    eyebrow.add_css_class("artist-eyebrow");

    let name = gtk4::Label::new(None);
    name.set_xalign(0.0);
    name.set_wrap(true);
    name.add_css_class("artist-hero-name");

    let meta = gtk4::Label::new(None);
    meta.set_xalign(0.0);
    meta.set_wrap(true);
    meta.add_css_class("artist-hero-meta");
    meta.add_css_class("dim-label");

    column.append(&eyebrow);
    column.append(&name);
    column.append(&meta);
    column.append(&build_actions(callbacks));
    content.append(&column);

    // Glow as the sized base; the content overlaid on top drives the natural
    // hero size (`set_measure_overlay`), so the glow just fills behind it.
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&glow));
    overlay.add_overlay(&content);
    overlay.set_measure_overlay(&content, true);
    overlay.add_css_class("artist-hero-container");

    Hero {
        root: overlay.upcast(),
        name,
        meta,
        initials,
        avatar_css,
        glow_css,
    }
}

/// Builds the round gradient avatar box with its centered initials label.
fn build_avatar() -> (gtk4::Box, gtk4::Label, gtk4::CssProvider) {
    let avatar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    avatar.add_css_class("artist-hero-avatar");
    avatar.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
    avatar.set_halign(gtk4::Align::Center);
    avatar.set_valign(gtk4::Align::Center);
    let avatar_css = gtk4::CssProvider::new();
    attach_provider(&avatar, &avatar_css);

    let initials = gtk4::Label::new(None);
    initials.set_halign(gtk4::Align::Center);
    initials.set_valign(gtk4::Align::Center);
    initials.add_css_class("artist-hero-initials");
    avatar.append(&initials);
    (avatar, initials, avatar_css)
}

/// Builds the Play all (accent pill) / Shuffle / ⋮ menu action row.
fn build_actions(callbacks: &HeroCallbacks) -> gtk4::Box {
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    actions.add_css_class("artist-hero-actions");
    actions.set_margin_top(6);

    let play_all = gtk4::Button::with_label(&strings::text(strings::ARTIST_DETAIL_PLAY_ALL));
    play_all.add_css_class("player-bar-play");
    play_all.add_css_class("artist-hero-play");
    play_all.add_css_class("pill");
    connect_artist_action(&play_all, &callbacks.on_play_all, callbacks);
    actions.append(&play_all);

    let shuffle = gtk4::Button::with_label(&strings::text(strings::SHUFFLE));
    shuffle.add_css_class("artist-hero-shuffle");
    shuffle.add_css_class("pill");
    connect_artist_action(&shuffle, &callbacks.on_shuffle, callbacks);
    actions.append(&shuffle);

    actions.append(&build_menu_button(callbacks));
    actions
}

/// Wires `button` to invoke `callback` with the pane's current artist name.
fn connect_artist_action(
    button: &gtk4::Button,
    callback: &ArtistCallback,
    callbacks: &HeroCallbacks,
) {
    let callback = callback.clone();
    let current_artist = callbacks.current_artist.clone();
    button.connect_clicked(move |_| {
        let cb = callback.borrow().clone();
        if let Some(cb) = cb {
            let artist = current_artist.borrow().clone();
            cb(artist);
        }
    });
}

/// The ⋮ menu button. Its Add to queue / Go to folder entries route to the
/// pane's callbacks with the current artist name (Task 9a), mirroring how the
/// Play all / Shuffle buttons thread through `connect_artist_action`.
// v2: Edit tags for all — deferred; the multi-track tag editor's entry point is
// coupled to track-list selection, so wiring it to arbitrary artist ids waits.
fn build_menu_button(callbacks: &HeroCallbacks) -> gtk4::MenuButton {
    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::text(strings::ARTIST_DETAIL_ADD_TO_QUEUE)),
        Some("artist-detail.add-to-queue"),
    );
    menu.append(
        Some(&strings::text(strings::ARTIST_DETAIL_GO_TO_FOLDER)),
        Some("artist-detail.go-to-folder"),
    );

    let group = gio::SimpleActionGroup::new();
    group.add_action(&menu_action(
        "add-to-queue",
        &callbacks.on_add_to_queue,
        &callbacks.current_artist,
    ));
    group.add_action(&menu_action(
        "go-to-folder",
        &callbacks.on_go_to_folder,
        &callbacks.current_artist,
    ));

    let button = gtk4::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text(strings::text(strings::ARTIST_DETAIL_MENU))
        .menu_model(&menu)
        .build();
    button.add_css_class("artist-hero-menu");
    button.add_css_class("flat");
    button.insert_action_group("artist-detail", Some(&group));
    button
}

/// Builds a menu `SimpleAction` that invokes `callback` with the pane's current
/// artist name at activation time. Clones the callback out of its `RefCell`
/// before calling (never holds the borrow across the invocation) — the same
/// reentrancy-safe pattern as `connect_artist_action`.
fn menu_action(
    name: &str,
    callback: &ArtistCallback,
    current_artist: &std::rc::Rc<std::cell::RefCell<String>>,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new(name, None);
    let callback = callback.clone();
    let current_artist = current_artist.clone();
    action.connect_activate(move |_, _| {
        let cb = callback.borrow().clone();
        if let Some(cb) = cb {
            let artist = current_artist.borrow().clone();
            cb(artist);
        }
    });
    action
}

/// The avatar's inline gradient rule, scoped to the single avatar box.
fn avatar_gradient_css(name: &str) -> String {
    format!(
        ".artist-hero-avatar {{ background-image: {}; }}",
        artist_avatar::gradient_css(name)
    )
}

/// The glow surface's inline fill: a soft radial tint in the accent color.
fn glow_css(accent: Rgb) -> String {
    format!(
        ".artist-hero-glow {{ background-image: radial-gradient(circle at 26% 0%, \
         rgba({r},{g},{b},0.42), rgba({r},{g},{b},0.0) 62%); }}",
        r = accent.r,
        g = accent.g,
        b = accent.b,
    )
}

/// Attaches a per-widget `CssProvider`. `style_context` is deprecated in GTK
/// 4.10+, but the avatar gradient and glow color are intrinsically per-widget
/// data, and there is no non-deprecated per-widget provider API — the same
/// exception `artist_master_row.rs` documents.
#[allow(deprecated)]
fn attach_provider(widget: &impl IsA<gtk4::Widget>, provider: &gtk4::CssProvider) {
    widget
        .style_context()
        .add_provider(provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
}
