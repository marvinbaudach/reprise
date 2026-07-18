//! Album-grid context menu and playback action wiring.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::queries::AlbumSummary;
use reprise_core::view_source::ViewSource;

/// The `ViewSource` a container-play from this album card/menu belongs to —
/// the playback origin (QUE-1 section label, NAV-9a jump target).
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum AlbumKeyAction {
    ExplicitPlay,
    Propagate,
}

pub(in crate::ui) fn album_key_action(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
) -> AlbumKeyAction {
    let enter = matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter);
    let control_only = modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
        && !modifiers.intersects(
            gtk4::gdk::ModifierType::SHIFT_MASK
                | gtk4::gdk::ModifierType::ALT_MASK
                | gtk4::gdk::ModifierType::SUPER_MASK,
        );
    if enter && control_only {
        AlbumKeyAction::ExplicitPlay
    } else {
        AlbumKeyAction::Propagate
    }
}

pub(in crate::ui) fn route_album_key(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
    explicit_play: impl FnOnce() -> bool,
) -> gtk4::glib::Propagation {
    if album_key_action(key, modifiers) == AlbumKeyAction::ExplicitPlay && explicit_play() {
        gtk4::glib::Propagation::Stop
    } else {
        gtk4::glib::Propagation::Proceed
    }
}

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

    pub(in crate::ui) fn set_on_primary(
        &self,
        callback: impl Fn(Vec<i64>, usize, ViewSource, AlbumSummary) + 'static,
    ) {
        let conn = self.conn.clone();
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                callback(ids, 0, album_source(album), album.clone());
            }
        });
        *self.card_shared.on_primary.borrow_mut() = Some(action);
    }

    pub(in crate::ui) fn set_on_play_next(&self, callback: impl Fn(Vec<i64>) + 'static) {
        let conn = self.conn.clone();
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                callback(ids);
            }
        });
        *self.menu_shared.on_play_next.borrow_mut() = Some(action);
    }

    pub(in crate::ui) fn set_on_queue(&self, callback: impl Fn(Vec<i64>) + 'static) {
        let conn = self.conn.clone();
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                callback(ids);
            }
        });
        *self.menu_shared.on_queue.borrow_mut() = Some(action);
    }

    pub(in crate::ui) fn set_on_edit_tags(&self, callback: impl Fn(Vec<i64>) + 'static) {
        let conn = self.conn.clone();
        let action: AlbumAction = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let conn = conn.borrow();
                album_card_actions::album_track_ids(&conn, album)
            };
            if !ids.is_empty() {
                callback(ids);
            }
        });
        *self.menu_shared.on_edit_tags.borrow_mut() = Some(action);
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

pub(in crate::ui) fn focused_album(
    grid: &gtk4::GridView,
    identities: &Rc<RefCell<AlbumCardIdentityRegistry>>,
) -> Option<AlbumSummary> {
    let card = focused_album_card(grid)?;
    resolve_card_album(&card, identities)
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn grid_2_enter_opens_detail_ctrl_enter_plays() {
        let calls = Cell::new(0);
        assert_eq!(
            route_album_key(
                gtk4::gdk::Key::Return,
                gtk4::gdk::ModifierType::empty(),
                || {
                    calls.set(calls.get() + 1);
                    true
                }
            ),
            gtk4::glib::Propagation::Proceed
        );
        assert_eq!(calls.get(), 0, "plain Enter stays native activation");
        assert_eq!(
            route_album_key(
                gtk4::gdk::Key::Return,
                gtk4::gdk::ModifierType::CONTROL_MASK,
                || {
                    calls.set(calls.get() + 1);
                    true
                }
            ),
            gtk4::glib::Propagation::Stop
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn grid_2_space_is_global_playpause_not_album() {
        let album_calls = Cell::new(0);
        assert_eq!(
            route_album_key(
                gtk4::gdk::Key::space,
                gtk4::gdk::ModifierType::empty(),
                || {
                    album_calls.set(album_calls.get() + 1);
                    true
                }
            ),
            gtk4::glib::Propagation::Proceed
        );
        assert_eq!(album_calls.get(), 0);
    }
}
