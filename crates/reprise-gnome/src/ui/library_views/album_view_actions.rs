//! Album-grid context menu and playback action wiring.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::queries::AlbumSummary;
use rusqlite::Connection;

use crate::ui::album_card::{AlbumAction, AlbumCardShared};
use crate::ui::album_card_actions;
use crate::ui::album_card_css;
use crate::ui::album_context_menu::{self, AlbumMenuShared};
use crate::ui::album_view_state;

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

    pub(in crate::ui) fn set_on_play(&self, callback: impl Fn(Vec<i64>, usize) + 'static) {
        let conn = self.conn.clone();
        let callback = Rc::new(callback);
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                callback(ids, 0);
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

    pub(in crate::ui) fn set_on_shuffle(&self, callback: impl Fn(Vec<i64>, usize) + 'static) {
        let conn = self.conn.clone();
        let callback = Rc::new(callback);
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let mut ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                album_card_actions::shuffle_ids(&mut ids);
                callback(ids, 0);
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
    filter_model: &gtk4::FilterListModel,
    menu_shared: &Rc<AlbumMenuShared>,
) {
    let action_group = album_context_menu::wire_actions(menu_shared);
    grid_view.insert_action_group(album_context_menu::ACTION_GROUP_NAME, Some(&action_group));

    let right_click = gtk4::GestureClick::builder().button(3).build();
    let menu_shared = menu_shared.clone();
    let grid_weak = grid_view.downgrade();
    let filter_model = filter_model.clone();
    right_click.connect_released(move |gesture, _press_count, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let Some(grid) = grid_weak.upgrade() else {
            return;
        };
        let Some(card) = picked_album_card(&grid, x, y) else {
            return;
        };
        let tooltip = card.tooltip_text().unwrap_or_default();
        let mut parts = tooltip.split(" \u{00b7} ");
        let album_title = parts.next().unwrap_or_default();
        let album_artist = parts.next().unwrap_or_default();
        for index in 0..filter_model.n_items() {
            let Some(object) = filter_model.item(index) else {
                continue;
            };
            let Some(boxed) = object.downcast_ref::<glib::BoxedAnyObject>() else {
                continue;
            };
            let album: std::cell::Ref<AlbumSummary> = boxed.borrow();
            if album_view_state::identity_matches(&album, album_title, album_artist) {
                let selected = album.clone();
                drop(album);
                album_context_menu::show(&card, &menu_shared, selected, x, y);
                break;
            }
        }
    });
    grid_view.add_controller(right_click);
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
