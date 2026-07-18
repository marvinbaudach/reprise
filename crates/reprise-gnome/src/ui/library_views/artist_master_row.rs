//! One recycled row of the Artists master list, and its row
//! `SignalListItemFactory`.
//!
//! Split from `artist_master.rs` (which owns the model, sort, selection, and
//! public API) so each file stays cohesive. `GtkListView` recycles row
//! widgets as they scroll, so the per-row `EqBars` and avatar are held in a
//! side table ([`Registry`]) keyed by the cell's `ListItem` pointer identity —
//! inserted on `connect_setup`, removed on `connect_teardown`, looked up on
//! `connect_bind` (the same pattern `track_list_columns.rs` uses for its
//! per-cell cover generation counters).
//! The master walks that table to light the now-playing row's EQ rather than
//! forcing a full model rebind.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::artist_avatar;
use crate::ui::artist_portrait_worker::{ArtistPortraitRequest, ArtistPortraitRuntime};
use crate::ui::eq_bars::{self, EqVariant};
use crate::ui::strings;
use reprise_core::artist_portrait::PortraitOutcome;
use reprise_core::queries::ArtistSummary;

/// Row height target from the design; the avatar is 38px within it.
const ROW_HEIGHT: i32 = 56;
const AVATAR_SIZE: i32 = 38;

/// Side table of live (recycled) rows, keyed by `ListItem` pointer identity.
pub(in crate::ui) type Registry = Rc<RefCell<HashMap<usize, Rc<RowHandles>>>>;

/// Widgets and per-row state kept alive for a single (recycled) row, so
/// `connect_bind` can update them and the master's `set_now_playing_artist`
/// can reach the row's mini-EQ without walking the widget tree.
pub(in crate::ui) struct RowHandles {
    root: gtk4::Box,
    avatar: gtk4::Box,
    portrait: gtk4::Picture,
    initials: gtk4::Label,
    name: gtk4::Label,
    meta: gtk4::Label,
    /// The shared `eq_bars` motif (CSS-animated); visibility is the only
    /// per-row control — shown on the now-playing artist's row.
    eq: gtk4::Box,
    /// The artist currently bound to this row — read by [`Self::set_now_playing`]
    /// to decide whether to light the EQ.
    artist: Rc<RefCell<String>>,
}

impl RowHandles {
    /// Shows this row's mini-EQ iff `now_playing` matches the bound artist
    /// (case-insensitively).
    pub(in crate::ui) fn set_now_playing(&self, now_playing: Option<&str>) {
        self.eq
            .set_visible(is_now_playing(now_playing, &self.artist.borrow()));
    }

    fn clear_portrait(&self) {
        self.portrait.set_visible(false);
        self.portrait.set_paintable(gtk4::gdk::Paintable::NONE);
    }
}

/// Whether `row_artist` is the currently-playing artist (Unicode case-folded,
/// matching the case-insensitive sort keys elsewhere in the master list).
fn is_now_playing(now_playing: Option<&str>, row_artist: &str) -> bool {
    now_playing.is_some_and(|now| now.to_lowercase() == row_artist.to_lowercase())
}

/// The row factory: builds each 56px row once (`connect_setup`), registers its
/// handles keyed by the cell pointer, updates them on `connect_bind`, and
/// unregisters on `connect_teardown`.
pub(in crate::ui) fn build_row_factory(
    registry: &Registry,
    now_playing: &Rc<RefCell<Option<String>>>,
    portraits: &Rc<ArtistPortraitRuntime>,
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
        let portraits = portraits.clone();
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
            bind_row(
                &handles,
                &summary,
                now_playing.borrow().as_deref(),
                &portraits,
            );
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
/// trailing mini-EQ. The avatar identity is represented by one class from the
/// centrally registered palette.
fn build_row() -> Rc<RowHandles> {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    root.add_css_class("artist-list-row");
    root.set_height_request(ROW_HEIGHT);

    let avatar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    avatar.add_css_class("artist-list-avatar");
    avatar.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
    avatar.set_halign(gtk4::Align::Center);
    avatar.set_valign(gtk4::Align::Center);

    let initials = gtk4::Label::new(None);
    initials.set_halign(gtk4::Align::Center);
    initials.set_valign(gtk4::Align::Center);
    avatar.append(&initials);

    let portrait = gtk4::Picture::new();
    portrait.set_content_fit(gtk4::ContentFit::Cover);
    portrait.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
    portrait.set_overflow(gtk4::Overflow::Hidden);
    portrait.add_css_class("artist-portrait-image");
    portrait.set_visible(false);

    let avatar_overlay = gtk4::Overlay::new();
    avatar_overlay.set_child(Some(&avatar));
    avatar_overlay.add_overlay(&portrait);
    root.append(&avatar_overlay);

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

    let eq = eq_bars::build(EqVariant::Animated);
    eq.set_visible(false);
    root.append(&eq);

    Rc::new(RowHandles {
        root,
        avatar,
        portrait,
        initials,
        name,
        meta,
        eq,
        artist: Rc::new(RefCell::new(String::new())),
    })
}

/// Updates a recycled row's widgets for `summary`.
fn bind_row(
    handles: &RowHandles,
    summary: &ArtistSummary,
    now_playing: Option<&str>,
    portraits: &Rc<ArtistPortraitRuntime>,
) {
    handles
        .initials
        .set_text(&artist_avatar::initials(&summary.artist));
    set_avatar_gradient(&handles.avatar, &summary.artist);
    handles.name.set_text(&summary.artist);
    handles.meta.set_text(&strings::artist_counts(
        summary.album_count,
        summary.track_count,
    ));
    *handles.artist.borrow_mut() = summary.artist.clone();
    handles
        .eq
        .set_visible(is_now_playing(now_playing, &summary.artist));

    handles.clear_portrait();
    request_portrait(handles, portraits);
}

fn request_portrait(handles: &RowHandles, portraits: &Rc<ArtistPortraitRuntime>) {
    let artist = handles.artist.borrow().clone();
    if artist.is_empty() {
        return;
    }
    let (sender, receiver) = async_channel::bounded(1);
    portraits.request(ArtistPortraitRequest {
        generation: 0,
        artist,
        force: false,
        response: sender,
    });
    let handles_artist = handles.artist.clone();
    let portrait = handles.portrait.clone();
    glib::spawn_future_local(async move {
        let Ok(response) = receiver.recv().await else {
            return;
        };
        if handles_artist.borrow().as_str() != response.artist.as_str() {
            return;
        }
        if let Ok(PortraitOutcome::Found(path)) = response.result {
            if let Ok(texture) = gtk4::gdk::Texture::from_filename(&path) {
                portrait.set_paintable(Some(&texture));
                portrait.set_visible(true);
            }
        }
    });
}

fn set_avatar_gradient(avatar: &gtk4::Box, artist: &str) {
    for index in 0..artist_avatar::GRADIENT_COUNT {
        avatar.remove_css_class(&format!("artist-avatar-gradient-{index}"));
    }
    avatar.add_css_class(&artist_avatar::gradient_class(artist));
}
