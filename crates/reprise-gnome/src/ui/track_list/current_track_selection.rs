//! Keeps the track-table selection synchronized with playback changes while
//! leaving a user's selection untouched when the playing track is not part
//! of the currently filtered/source-specific view.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use super::player_controller::PlayerController;
use super::track_list::TrackList;
use super::track_list_activation::current_queue_ids;

pub(super) type OnCurrentTrackChanged = Rc<dyn Fn(i64, Option<usize>)>;

fn visible_position_for_track_in_source(
    ids: &[i64],
    current_id: i64,
    queue_position: Option<usize>,
    is_queue: bool,
) -> Option<u32> {
    if queue_position.is_some_and(|position| ids.get(position) == Some(&current_id)) {
        return queue_position.and_then(|position| u32::try_from(position).ok());
    }
    if is_queue {
        return None;
    }
    ids.iter()
        .position(|candidate| *candidate == current_id)
        .and_then(|position| u32::try_from(position).ok())
}

pub(super) fn wire(player: Option<&Rc<PlayerController>>, track_list: &Rc<TrackList>) {
    let Some(player) = player else {
        return;
    };
    let track_list = Rc::downgrade(track_list);
    player.set_on_current_track_changed(move |track_id, queue_position| {
        match track_list.upgrade() {
            Some(track_list) => track_list.select_current_track(track_id, queue_position),
            None => tracing::warn!(
                track_id,
                "current-track selection skipped: track list is gone"
            ),
        }
    });

    let weak = Rc::downgrade(player);
    player.set_on_title_click(move || {
        if let Some(controller) = weak.upgrade() {
            controller.notify_restored_current_track();
        }
    });
}

impl PlayerController {
    pub(super) fn set_on_current_track_changed(
        &self,
        callback: impl Fn(i64, Option<usize>) + 'static,
    ) {
        *self.current_track_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn notify_current_track_changed(
        &self,
        track_id: i64,
        queue_position: Option<usize>,
    ) {
        let callback = self.current_track_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(track_id, queue_position);
        }
    }

    pub(super) fn notify_restored_current_track(&self) {
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current());
        if let Some(track_id) = current {
            self.notify_current_track_changed(track_id, None);
        }
    }
}

impl TrackList {
    fn select_current_track(&self, track_id: i64, queue_position: Option<usize>) {
        let sort = self.shared.sort.borrow().clone();
        let filter = self.shared.filter.borrow().clone();
        let source = self.shared.source.borrow().clone();
        let browse = self.shared.browse_filter.borrow().clone();
        let queue_ids = if matches!(source, ViewSource::Queue) {
            current_queue_ids(&self.shared)
        } else {
            Vec::new()
        };
        let ids = {
            let conn = self.shared.conn.borrow();
            queries::query_track_ids_browsed(
                &conn,
                &source,
                &sort.field,
                &sort.dir,
                &filter,
                &browse,
                &queue_ids,
            )
        };
        let ids = match ids {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(%error, track_id, "failed to locate current track in table");
                return;
            }
        };
        let Some(position) = visible_position_for_track_in_source(
            &ids,
            track_id,
            queue_position,
            matches!(source, ViewSource::Queue),
        ) else {
            tracing::debug!(
                track_id,
                "current track is not visible in the active table query"
            );
            return;
        };

        self.shared.selection.select_item(position, true);
        // Defer `scroll_to` to an idle tick rather than calling it inline.
        // During session restore this runs while the window is still being
        // constructed — before the `ColumnView` has completed its first size
        // allocation — and a `scroll_to` issued that early corrupts the row
        // layout near the top of the list, leaving a persistent phantom gap
        // (an unrendered row) that never resolves on scroll. Running it on the
        // next idle lets GTK finish the initial allocation first; for the live
        // track-change path (view already realized) the one-tick delay is
        // imperceptible.
        let column_view = self.shared.column_view.clone();
        gtk4::glib::idle_add_local_once(move || {
            column_view.scroll_to(position, None, gtk4::ListScrollFlags::NONE, None);
        });
        tracing::info!(track_id, position, "table selection followed current track");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_position_finds_the_current_track_in_view_order() {
        assert_eq!(
            visible_position_for_track_in_source(&[41, 42, 43], 42, None, false),
            Some(1)
        );
    }

    #[test]
    fn visible_position_uses_queue_occurrence_then_falls_back_to_first_match() {
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, Some(2), false),
            Some(2)
        );
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, Some(1), false),
            Some(0)
        );
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 9, None, false),
            None
        );
    }

    #[test]
    fn queue_does_not_highlight_a_pending_duplicate_of_the_current_track() {
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, None, true),
            None
        );
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, None, false),
            Some(0)
        );
    }
}
