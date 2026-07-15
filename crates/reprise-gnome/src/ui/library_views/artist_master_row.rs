//! One recycled row of the Artists master list, and its row
//! `SignalListItemFactory`.
//!
//! Split from `artist_master.rs` (which owns the model, sort, selection, and
//! public API) so each file stays cohesive. `GtkListView` recycles row
//! widgets as they scroll, so the per-row `EqBars` and the avatar's inline
//! gradient `CssProvider` are held in a side table ([`Registry`]) keyed by the
//! cell's `ListItem` pointer identity — inserted on `connect_setup`, removed
//! on `connect_teardown`, looked up on `connect_bind` (the same pattern
//! `track_list_columns.rs` uses for its per-cell cover generation counters).
//! The master walks that table to light the now-playing row's EQ rather than
//! forcing a full model rebind.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::artist_avatar;
use crate::ui::eq_bars::EqBars;
use crate::ui::strings;
use reprise_core::queries::ArtistSummary;

/// Row height target from the design; the avatar is 38px within it.
const ROW_HEIGHT: i32 = 56;
const AVATAR_SIZE: i32 = 38;

/// Side table of live (recycled) rows, keyed by `ListItem` pointer identity.
pub(super) type Registry = Rc<RefCell<HashMap<usize, Rc<RowHandles>>>>;

/// Widgets and per-row state kept alive for a single (recycled) row, so
/// `connect_bind` can update them and the master's `set_now_playing_artist`
/// can reach the row's `EqBars` without walking the widget tree.
pub(super) struct RowHandles {
    root: gtk4::Box,
    avatar_css: gtk4::CssProvider,
    initials: gtk4::Label,
    name: gtk4::Label,
    meta: gtk4::Label,
    eq: EqBars,
    /// The artist currently bound to this row — read by [`Self::set_now_playing`]
    /// to decide whether to light the EQ.
    artist: RefCell<String>,
}

impl RowHandles {
    /// Lights this row's mini-EQ iff `now_playing` matches the bound artist
    /// (case-insensitively).
    pub(super) fn set_now_playing(&self, now_playing: Option<&str>) {
        self.eq
            .set_active(is_now_playing(now_playing, &self.artist.borrow()));
    }
}

/// Whether `row_artist` is the currently-playing artist.
fn is_now_playing(now_playing: Option<&str>, row_artist: &str) -> bool {
    now_playing.is_some_and(|now| now.eq_ignore_ascii_case(row_artist))
}

/// The row factory: builds each 56px row once (`connect_setup`), registers its
/// handles keyed by the cell pointer, updates them on `connect_bind`, and
/// unregisters on `connect_teardown`.
pub(super) fn build_row_factory(
    registry: &Registry,
    now_playing: &Rc<RefCell<Option<String>>>,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();

    {
        let registry = registry.clone();
        factory.connect_setup(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let handles = build_row();
            item.set_child(Some(&handles.root));
            registry
                .borrow_mut()
                .insert(item.as_ptr() as usize, handles);
        });
    }

    {
        let registry = registry.clone();
        let now_playing = now_playing.clone();
        factory.connect_bind(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let handles = registry.borrow().get(&(item.as_ptr() as usize)).cloned();
            let Some(handles) = handles else {
                tracing::warn!("artist master bind: no row handles for cell");
                return;
            };
            let Some(boxed) = item
                .item()
                .and_then(|obj| obj.downcast::<glib::BoxedAnyObject>().ok())
            else {
                return;
            };
            let summary = boxed.borrow::<ArtistSummary>();
            bind_row(&handles, &summary, now_playing.borrow().as_deref());
        });
    }

    {
        let registry = registry.clone();
        factory.connect_teardown(move |_, obj| {
            if let Some(item) = obj.downcast_ref::<gtk4::ListItem>() {
                registry.borrow_mut().remove(&(item.as_ptr() as usize));
            }
        });
    }

    factory
}

/// Builds one row's widgets: gradient avatar (initials) + name/meta stack +
/// trailing mini-EQ. Styling is class-only (Task 10) except the avatar's
/// per-artist gradient, which is intrinsically per-row data carried on its own
/// `CssProvider`.
fn build_row() -> Rc<RowHandles> {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    root.add_css_class("artist-list-row");
    root.set_height_request(ROW_HEIGHT);

    let avatar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    avatar.add_css_class("artist-list-avatar");
    avatar.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
    avatar.set_halign(gtk4::Align::Center);
    avatar.set_valign(gtk4::Align::Center);
    let avatar_css = gtk4::CssProvider::new();
    attach_avatar_provider(&avatar, &avatar_css);

    let initials = gtk4::Label::new(None);
    initials.set_halign(gtk4::Align::Center);
    initials.set_valign(gtk4::Align::Center);
    avatar.append(&initials);
    root.append(&avatar);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    text_box.set_valign(gtk4::Align::Center);
    text_box.set_hexpand(true);

    let name = gtk4::Label::new(None);
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name.add_css_class("artist-list-name");

    let meta = gtk4::Label::new(None);
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    meta.add_css_class("artist-list-meta");
    meta.add_css_class("dim-label");

    text_box.append(&name);
    text_box.append(&meta);
    root.append(&text_box);

    let eq = EqBars::new();
    eq.widget().set_valign(gtk4::Align::Center);
    root.append(eq.widget());

    Rc::new(RowHandles {
        root,
        avatar_css,
        initials,
        name,
        meta,
        eq,
        artist: RefCell::new(String::new()),
    })
}

/// Updates a recycled row's widgets for `summary`.
fn bind_row(handles: &RowHandles, summary: &ArtistSummary, now_playing: Option<&str>) {
    handles
        .initials
        .set_text(&artist_avatar::initials(&summary.artist));
    handles
        .avatar_css
        .load_from_string(&avatar_gradient_css(&summary.artist));
    handles.name.set_text(&summary.artist);
    handles.meta.set_text(&strings::artist_counts(
        summary.album_count,
        summary.track_count,
    ));
    *handles.artist.borrow_mut() = summary.artist.clone();
    handles
        .eq
        .set_active(is_now_playing(now_playing, &summary.artist));
}

/// The avatar's inline background rule. The provider is scoped to the single
/// avatar box, so the class selector only ever matches that one widget.
fn avatar_gradient_css(name: &str) -> String {
    format!(
        ".artist-list-avatar {{ background-image: {}; }}",
        artist_avatar::gradient_css(name)
    )
}

/// Attaches a per-widget `CssProvider` to the avatar box. `style_context` is
/// deprecated in GTK 4.10+, but a per-row gradient is intrinsically per-widget
/// data (see the module doc and this task's constraints), and there is no
/// non-deprecated per-widget provider API.
#[allow(deprecated)]
fn attach_avatar_provider(avatar: &gtk4::Box, provider: &gtk4::CssProvider) {
    avatar
        .style_context()
        .add_provider(provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
}
