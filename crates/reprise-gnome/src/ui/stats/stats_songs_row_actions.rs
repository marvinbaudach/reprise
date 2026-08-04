//! What a My Stats ranking row does when it is activated.
//!
//! Split out of `stats_songs_card.rs` because the card no longer has one kind
//! of row: since STATS-22 the continuation lives inside the same card as the
//! visible ten and has to answer identically — click, Enter, Space, right
//! click and Shift+F10. Keeping that contract in one place is what stops the
//! two row builders from drifting into two different rows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::stats_screen::TopTrack;

use super::stats_metadata_links::{MetadataCallback, StatsMetadataTarget};
use super::stats_songs_card::SONG_ROW_LIMIT;
use crate::ui::strings;

pub(super) type IdCallback = Rc<RefCell<Option<Rc<dyn Fn(i64)>>>>;
/// Starting playback hands over the *ranking* the row sits in, not just the
/// row: `(ids, index)` is the same shape the track table activates with, so
/// the queue, Previous/Next and Shuffle get a context to work on instead of a
/// single orphaned track (STATS-21).
pub(super) type PlayCallback = Rc<RefCell<Option<Rc<dyn Fn(&[i64], usize)>>>>;

/// The ranking a row hands over when it starts playing (STATS-21): the one on
/// screen at that moment. Collapsed that is the visible ten; with the
/// continuation open it is the ten plus the ranks the user just revealed
/// (STATS-22). Seeding the queue from rows that are still hidden would play
/// tracks nobody was shown, so the list follows the card, not the render.
#[derive(Clone, Default)]
pub(super) struct PlayContext {
    /// Every rendered row's track id — the ten first, then the continuation,
    /// both in the sort currently selected.
    pub(super) ranking: Rc<RefCell<Vec<i64>>>,
    pub(super) expanded: Rc<Cell<bool>>,
}

impl PlayContext {
    /// Owned on purpose: the borrow ends before the callback runs, so a
    /// playback handler is free to re-render the card it was started from.
    fn visible(&self) -> Vec<i64> {
        let ranking = self.ranking.borrow();
        let len = if self.expanded.get() {
            ranking.len()
        } else {
            ranking.len().min(SONG_ROW_LIMIT)
        };
        ranking[..len].to_vec()
    }
}

/// The callbacks every ranking row shares, and the ranking they act inside.
#[derive(Clone, Default)]
pub(super) struct RowActions {
    pub(super) metadata: MetadataCallback,
    pub(super) on_play_track: PlayCallback,
    pub(super) on_play_next: IdCallback,
    pub(super) on_add_to_queue: IdCallback,
    pub(super) play_context: PlayContext,
}

impl RowActions {
    pub(super) fn new(metadata: MetadataCallback) -> Self {
        Self {
            metadata,
            ..Self::default()
        }
    }

    /// Everything that turns a ranking row into an activatable one: the
    /// pointer and keyboard affordances that start `index` inside the visible
    /// ranking, and the context menu carrying the three track actions. The
    /// continuation gets the identical treatment since STATS-22 — rank 11 sits
    /// in the same card as rank 10 and must not be a dead line under it.
    ///
    /// Returns the gesture and the action group so the caller can keep them
    /// alive for exactly as long as it keeps the row.
    pub(super) fn attach(
        &self,
        row: &gtk4::Box,
        track: &TopTrack,
        index: usize,
    ) -> (gtk4::GestureClick, gio::SimpleActionGroup) {
        // a11y-semantics: role=button name=track-row state=focusable action=enter/shift-f10
        row.set_focusable(true);
        row.set_accessible_role(gtk4::AccessibleRole::Button);
        // input-parity: ACC-8 keyboard=enter-space-row
        row.set_cursor_from_name(Some("pointer"));
        row.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::STATS_PLAY_TRACK,
        ))]);

        // input-parity: ACC-8 keyboard=enter-row
        let activate = gtk4::GestureClick::new();
        activate.set_button(1);
        activate.connect_released({
            let callback = self.on_play_track.clone();
            let context = self.play_context.clone();
            move |_, _, _, _| invoke_play(&callback, &context, index)
        });
        row.add_controller(activate.clone());
        self.install_row_keys(row, index);

        (activate, self.install_context_menu(row, track))
    }

    /// Enter and Space start the row's track, matching the pointer. Return
    /// `Proceed` for everything else so the context-menu shortcut still reaches
    /// its own controller.
    fn install_row_keys(&self, row: &gtk4::Box, index: usize) {
        let keys = gtk4::EventControllerKey::new();
        let callback = self.on_play_track.clone();
        let context = self.play_context.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if matches!(
                key,
                gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter | gtk4::gdk::Key::space
            ) {
                invoke_play(&callback, &context, index);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        row.add_controller(keys);
    }

    fn install_context_menu(&self, row: &gtk4::Box, track: &TopTrack) -> gio::SimpleActionGroup {
        let menu = gio::Menu::new();
        menu.append(Some("Play next"), Some("song.play-next"));
        menu.append(Some("Add to queue"), Some("song.add-to-queue"));
        menu.append(Some("Go to album"), Some("song.open-album"));
        let actions = gio::SimpleActionGroup::new();
        add_id_action(&actions, "play-next", track.track_id, &self.on_play_next);
        add_id_action(
            &actions,
            "add-to-queue",
            track.track_id,
            &self.on_add_to_queue,
        );
        let open_album = gio::SimpleAction::new("open-album", None);
        open_album.connect_activate({
            let callback = self.metadata.clone();
            let target = StatsMetadataTarget::Album {
                track_id: track.track_id,
                album: track.album.clone(),
                album_artist: track.effective_artist.clone(),
            };
            move |_, _| invoke_metadata(&callback, target.clone())
        });
        actions.add_action(&open_album);
        row.insert_action_group("song", Some(&actions));

        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(row);
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        // input-parity: ACC-8 keyboard=menu-shift-f10
        let click = gtk4::GestureClick::new();
        click.set_button(3);
        click.connect_pressed({
            let popover = popover.downgrade();
            move |_, _, x, y| {
                let Some(popover) = popover.upgrade() else {
                    return;
                };
                popup(&popover, x, y);
            }
        });
        row.add_controller(click);
        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed({
            let popover = popover.downgrade();
            move |controller, key, _, modifiers| {
                if !crate::ui::track_list::track_list_context_keys::is_context_menu_shortcut(
                    key, modifiers,
                ) {
                    return glib::Propagation::Proceed;
                }
                let Some(popover) = popover.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let Some(row) = controller.widget() else {
                    return glib::Propagation::Proceed;
                };
                popup(
                    &popover,
                    f64::from(row.width()) / 2.0,
                    f64::from(row.height()) / 2.0,
                );
                glib::Propagation::Stop
            }
        });
        row.add_controller(keys);
        actions
    }
}

fn add_id_action(
    actions: &gio::SimpleActionGroup,
    name: &str,
    track_id: i64,
    callback: &IdCallback,
) {
    let action = gio::SimpleAction::new(name, None);
    let callback = callback.clone();
    action.connect_activate(move |_, _| invoke_id(&callback, track_id));
    actions.add_action(&action);
}

fn popup(popover: &gtk4::PopoverMenu, x: f64, y: f64) {
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
        x.round() as i32,
        y.round() as i32,
        1,
        1,
    )));
    popover.popup();
}

/// Starts the row at `index` of the visible ranking. An index that ranking
/// cannot answer for would seed the queue at the wrong track, so it plays
/// nothing instead.
fn invoke_play(callback: &PlayCallback, context: &PlayContext, index: usize) {
    let callback = callback.borrow().clone();
    let ranking = context.visible();
    if index >= ranking.len() {
        tracing::warn!(index, len = ranking.len(), "stale stats row; not playing");
        return;
    }
    if let Some(callback) = callback {
        callback(&ranking, index);
    }
}

fn invoke_id(callback: &IdCallback, id: i64) {
    let callback = callback.borrow().clone();
    if let Some(callback) = callback {
        callback(id);
    }
}

fn invoke_metadata(callback: &MetadataCallback, target: StatsMetadataTarget) {
    let callback = callback.borrow().clone();
    if let Some(callback) = callback {
        callback(target);
    }
}
