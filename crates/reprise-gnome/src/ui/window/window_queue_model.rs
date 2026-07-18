//! Shared queue snapshot owned by the window composition root.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::player_controller::PlayerController;
use crate::ui::track_list::queue_sections::QueueViewModel;

pub(in crate::ui) type SharedQueueModel = Rc<RefCell<QueueViewModel>>;

/// Creates the one queue model consumed by both the management ColumnView
/// and the compact Up Next panel. The first queue-change callback always
/// updates this snapshot; later surface callbacks only decide whether they
/// need to render it.
pub(in crate::ui) fn build(player: &Option<Rc<PlayerController>>) -> SharedQueueModel {
    let model = Rc::new(RefCell::new(
        player
            .as_ref()
            .map_or_else(QueueViewModel::default, |player| player.queue_view_model()),
    ));
    if let Some(player) = player {
        let player_weak = Rc::downgrade(player);
        let model_weak = Rc::downgrade(&model);
        player.add_on_queue_changed(move || {
            let (Some(player), Some(model)) = (player_weak.upgrade(), model_weak.upgrade()) else {
                return;
            };
            *model.borrow_mut() = player.queue_view_model();
        });
    }
    model
}
