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

fn visible_position_for_track(
    ids: &[i64],
    current_id: i64,
    queue_position: Option<usize>,
) -> Option<u32> {
    if queue_position.is_some_and(|position| ids.get(position) == Some(&current_id)) {
        return queue_position.and_then(|position| u32::try_from(position).ok());
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
        let snapshot = self.queue.borrow().snapshot();
        let current = self.queue.borrow().current();
        if let Some(track_id) = current {
            self.notify_current_track_changed(track_id, snapshot.position);
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
        let Some(position) = visible_position_for_track(&ids, track_id, queue_position) else {
            tracing::debug!(
                track_id,
                "current track is not visible in the active table query"
            );
            return;
        };

        self.shared.selection.select_item(position, true);
        self.shared
            .column_view
            .scroll_to(position, None, gtk4::ListScrollFlags::NONE, None);
        tracing::info!(track_id, position, "table selection followed current track");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_position_finds_the_current_track_in_view_order() {
        assert_eq!(visible_position_for_track(&[41, 42, 43], 42, None), Some(1));
    }

    #[test]
    fn visible_position_uses_queue_occurrence_then_falls_back_to_first_match() {
        assert_eq!(visible_position_for_track(&[7, 8, 7], 7, Some(2)), Some(2));
        assert_eq!(visible_position_for_track(&[7, 8, 7], 7, Some(1)), Some(0));
        assert_eq!(visible_position_for_track(&[7, 8, 7], 9, None), None);
    }
}
