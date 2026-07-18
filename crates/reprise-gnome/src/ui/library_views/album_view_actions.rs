//! Album-grid context menu and playback action wiring.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::queries::AlbumSummary;
use reprise_core::view_source::ViewSource;

/// The `ViewSource` a container-play from this album card/menu belongs to —
/// the playback origin (QUE-1 section label, NAV-9 jump target).
fn album_source(album: &AlbumSummary) -> ViewSource {
    ViewSource::Album {
        album: album.album.clone(),
        album_artist: album.album_artist.clone(),
    }
}
use rusqlite::Connection;

use crate::ui::album_card::{AlbumAction, AlbumCardShared};
use crate::ui::album_card_actions;
use crate::ui::album_card_css;
use crate::ui::album_card_state::AlbumCardIdentityRegistry;
use crate::ui::album_context_menu::{self, AlbumMenuShared};

pub(in crate::ui) struct AlbumViewActions {
    conn: Rc<RefCell<Connection>>,
    card_shared: Rc<AlbumCardShared>,
    menu_shared: Rc<AlbumMenuShared>,
}

impl AlbumViewActions {
    pub(in crate::ui) fn new(
        conn: &Rc<RefCell<Connection>>,
        card_shared: &Rc<AlbumCardShared>,
        menu_shared: &Rc<AlbumMenuShared>,
    ) -> Self {
        Self {
            conn: conn.clone(),
            card_shared: card_shared.clone(),
            menu_shared: menu_shared.clone(),
        }
    }

    pub(in crate::ui) fn set_on_play(
        &self,
        callback: impl Fn(Vec<i64>, usize, ViewSource) + 'static,
    ) {
        let conn = self.conn.clone();
        let callback = Rc::new(callback);
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                callback(ids, 0, album_source(album));
            }
        });
        *self.card_shared.on_play.borrow_mut() = Some(action.clone());
        *self.menu_shared.on_play.borrow_mut() = Some(action);
    }

    pub(in crate::ui) fn set_on_queue(&self, callback: impl Fn(Vec<i64>) + 'static) {
        let conn = self.conn.clone();
        let callback = Rc::new(callback);
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                callback(ids);
            }
        });
        *self.card_shared.on_queue.borrow_mut() = Some(action.clone());
        *self.menu_shared.on_queue.borrow_mut() = Some(action);
    }

    pub(in crate::ui) fn set_on_shuffle(
        &self,
        callback: impl Fn(Vec<i64>, usize, ViewSource) + 'static,
    ) {
        let conn = self.conn.clone();
        let callback = Rc::new(callback);
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let mut ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                album_card_actions::shuffle_ids(&mut ids);
                callback(ids, 0, album_source(album));
            }
        });
        *self.menu_shared.on_shuffle.borrow_mut() = Some(action);
    }

    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        let overlay = overlay.clone();
        *self.menu_shared.on_toast.borrow_mut() = Some(Rc::new(move |text: String| {
            crate::ui::toasts::show(&overlay, &text);
        }));
    }
}

pub(in crate::ui) fn install_context_menu(
    grid_view: &gtk4::GridView,
    identities: &Rc<RefCell<AlbumCardIdentityRegistry>>,
    menu_shared: &Rc<AlbumMenuShared>,
) {
    let action_group = album_context_menu::wire_actions(menu_shared);
    grid_view.insert_action_group(album_context_menu::ACTION_GROUP_NAME, Some(&action_group));

    let right_click = gtk4::GestureClick::builder().button(3).build();
    {
        let menu_shared = menu_shared.clone();
        let grid_weak = grid_view.downgrade();
        let identities = identities.clone();
        right_click.connect_released(move |gesture, _press_count, x, y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            let Some(grid) = grid_weak.upgrade() else {
                return;
            };
            let Some(card) = picked_album_card(&grid, x, y) else {
                return;
            };
            let Some(selected) = resolve_card_album(&card, &identities) else {
                return;
            };
            album_context_menu::show(&card, &menu_shared, selected, x, y);
        });
    }
    grid_view.add_controller(right_click);

    // Keyboard path (Menu key / Shift+F10): the same menu for the FOCUSED
    // card — mirrors `track_list_context_keys` on the track table.
    let key_controller = gtk4::EventControllerKey::new();
    {
        let menu_shared = menu_shared.clone();
        let grid_weak = grid_view.downgrade();
        let identities = identities.clone();
        key_controller.connect_key_pressed(move |_, key, _, modifiers| {
            let is_shortcut =
                crate::ui::track_list::track_list_context_keys::is_context_menu_shortcut(
                    key, modifiers,
                );
            if !is_shortcut {
                return gtk4::glib::Propagation::Proceed;
            }
            let Some(grid) = grid_weak.upgrade() else {
                return gtk4::glib::Propagation::Proceed;
            };
            let Some(card) = focused_album_card(&grid) else {
                return gtk4::glib::Propagation::Proceed;
            };
            let Some(selected) = resolve_card_album(&card, &identities) else {
                return gtk4::glib::Propagation::Proceed;
            };
            album_context_menu::show(
                &card,
                &menu_shared,
                selected,
                f64::from(card.width()) / 2.0,
                f64::from(card.height()) / 2.0,
            );
            tracing::debug!("album context menu opened from keyboard");
            gtk4::glib::Propagation::Stop
        });
    }
    grid_view.add_controller(key_controller);
}

/// Resolves the current generation-safe identity registered for a recycled
/// card. Pointer and keyboard context menus share this exact lookup.
fn resolve_card_album(
    card: &gtk4::Widget,
    identities: &Rc<RefCell<AlbumCardIdentityRegistry>>,
) -> Option<AlbumSummary> {
    identities.borrow().resolve(card.as_ptr() as usize)
}

fn picked_album_card(grid: &gtk4::GridView, x: f64, y: f64) -> Option<gtk4::Widget> {
    let mut current = grid.pick(x, y, gtk4::PickFlags::DEFAULT);
    while let Some(widget) = current {
        if widget.has_css_class(album_card_css::CARD_CLASS) {
            return Some(widget);
        }
        current = widget.parent();
    }
    None
}

/// The card `Box` inside the grid cell that currently has keyboard focus —
/// `focus_child` is the cell wrapper, the card is its subtree.
fn focused_album_card(grid: &gtk4::GridView) -> Option<gtk4::Widget> {
    let mut current = grid.focus_child();
    while let Some(widget) = current {
        if widget.has_css_class(album_card_css::CARD_CLASS) {
            return Some(widget);
        }
        current = widget.first_child();
    }
    None
}
