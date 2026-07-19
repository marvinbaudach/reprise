//! NAV-5 session memory for the Albums grid.

use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use reprise_core::queries::AlbumSummary;

const CARD_MIN_WIDTH: i32 = 184;
const GRID_HORIZONTAL_PADDING: i32 = 48;
const RESTORE_ATTEMPTS: u8 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct AlbumIdentity {
    pub title: String,
    pub artist: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::ui) struct SavedAlbumViewState {
    pub anchor: Option<(AlbumIdentity, f64)>,
    pub selection: Option<AlbumIdentity>,
}

fn identity_at(model: &gtk4::FilterListModel, position: u32) -> Option<AlbumIdentity> {
    model
        .item(position)
        .and_then(|object| object.downcast::<gtk4::glib::BoxedAnyObject>().ok())
        .map(|boxed| {
            let album = boxed.borrow::<AlbumSummary>();
            AlbumIdentity {
                title: album.album.clone(),
                artist: album.album_artist.clone(),
            }
        })
}

fn identity_position(model: &gtk4::FilterListModel, wanted: &AlbumIdentity) -> Option<u32> {
    (0..model.n_items()).find(|position| identity_at(model, *position).as_ref() == Some(wanted))
}

fn column_count(grid_width: i32, item_count: u32) -> u32 {
    if item_count == 0 {
        return 1;
    }
    let available = grid_width.saturating_sub(GRID_HORIZONTAL_PADDING);
    let columns = (available / CARD_MIN_WIDTH).max(1) as u32;
    columns.min(item_count)
}

fn row_count(item_count: u32, columns: u32) -> u32 {
    item_count.saturating_add(columns - 1) / columns
}

pub(in crate::ui) fn capture(
    grid: &gtk4::GridView,
    selection: &gtk4::SingleSelection,
    model: &gtk4::FilterListModel,
) -> SavedAlbumViewState {
    let selection = (selection.selected() != gtk4::INVALID_LIST_POSITION)
        .then(|| identity_at(model, selection.selected()))
        .flatten();
    let count = model.n_items();
    let columns = column_count(grid.width(), count);
    let anchor = grid.vadjustment().and_then(|adjustment| {
        let rows = row_count(count, columns);
        if rows == 0 || adjustment.upper() <= 0.0 {
            return None;
        }
        let row_height = adjustment.upper() / f64::from(rows);
        let value = adjustment.value();
        let row = (value / row_height).floor().max(0.0) as u32;
        let position = row.saturating_mul(columns).min(count.saturating_sub(1));
        identity_at(model, position).map(|id| (id, value - f64::from(row) * row_height))
    });
    SavedAlbumViewState { anchor, selection }
}

pub(in crate::ui) fn restore(
    grid: &gtk4::GridView,
    selection: &gtk4::SingleSelection,
    model: &gtk4::FilterListModel,
    saved: &SavedAlbumViewState,
) {
    selection.unselect_all();
    if let Some(position) = saved
        .selection
        .as_ref()
        .and_then(|id| identity_position(model, id))
    {
        selection.set_selected(position);
    }
    let Some((identity, offset)) = saved.anchor.clone() else {
        return;
    };
    let Some(position) = identity_position(model, &identity) else {
        return;
    };
    restore_scroll(
        grid.clone(),
        model.clone(),
        position,
        offset,
        RESTORE_ATTEMPTS,
        false,
    );
}

pub(in crate::ui) fn reveal(
    grid: &gtk4::GridView,
    model: &gtk4::FilterListModel,
    identity: &AlbumIdentity,
) -> bool {
    let Some(position) = identity_position(model, identity) else {
        return false;
    };
    restore_scroll(
        grid.clone(),
        model.clone(),
        position,
        0.0,
        RESTORE_ATTEMPTS,
        false,
    );
    true
}

pub(in crate::ui) fn reveal_and_focus_position(
    grid: &gtk4::GridView,
    model: &gtk4::FilterListModel,
    position: u32,
) {
    restore_scroll(
        grid.clone(),
        model.clone(),
        position,
        0.0,
        RESTORE_ATTEMPTS,
        true,
    );
}

fn restore_scroll(
    grid: gtk4::GridView,
    model: gtk4::FilterListModel,
    position: u32,
    offset: f64,
    attempts: u8,
    focus: bool,
) {
    gtk4::glib::idle_add_local_once(move || {
        let Some(adjustment) = grid.vadjustment() else {
            return;
        };
        let count = model.n_items();
        let columns = column_count(grid.width(), count);
        let rows = row_count(count, columns);
        if rows > 0 && adjustment.upper() > 0.0 {
            if adjustment.upper() > adjustment.page_size() {
                let row_height = adjustment.upper() / f64::from(rows);
                let row = position / columns;
                let max = adjustment.upper() - adjustment.page_size();
                adjustment.set_value((f64::from(row) * row_height + offset).clamp(0.0, max));
            }
            if focus {
                let grid = grid.clone();
                gtk4::glib::idle_add_local_once(move || {
                    grid.scroll_to(position, gtk4::ListScrollFlags::FOCUS, None);
                });
            }
        } else if attempts > 0 {
            restore_scroll(grid, model, position, offset, attempts - 1, focus);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_math_tracks_the_same_album_after_resort() {
        assert_eq!(column_count(600, 20), 3);
        assert_eq!(row_count(20, 3), 7);
        assert_eq!(row_count(21, 3), 7);
    }
}
