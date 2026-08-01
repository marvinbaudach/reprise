//! Shared queue snapshot owned by the window composition root.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::player_controller::PlayerController;
use crate::ui::track_list::queue_sections::QueueViewModel;

pub(in crate::ui) type SharedQueueModel = Rc<RefCell<QueueViewModel>>;
pub(super) type RefreshCallback = Rc<dyn Fn()>;

pub(super) fn install_refresh_callbacks(
    add_queue_changed: impl FnOnce(RefreshCallback),
    add_external_changed: impl FnOnce(RefreshCallback),
    refresh: RefreshCallback,
) {
    add_queue_changed(refresh.clone());
    add_external_changed(refresh);
}

/// Creates the one queue model consumed by both the management ColumnView
/// and the compact Up Next panel. The first queue/external-change callback
/// always updates this snapshot; later surface callbacks only decide whether
/// they need to render it.
pub(in crate::ui) fn build(player: &Option<Rc<PlayerController>>) -> SharedQueueModel {
    let initial = match player {
        Some(player) => player.queue_view_model(),
        None => QueueViewModel::default(),
    };
    let model = Rc::new(RefCell::new(initial));
    if let Some(player) = player {
        let player_weak = Rc::downgrade(player);
        let model_weak = Rc::downgrade(&model);
        let refresh = Rc::new(move || {
            let (Some(player), Some(model)) = (player_weak.upgrade(), model_weak.upgrade()) else {
                return;
            };
            *model.borrow_mut() = player.queue_view_model();
        }) as RefreshCallback;
        install_refresh_callbacks(
            |refresh| player.add_on_queue_changed(move || refresh()),
            |refresh| player.add_on_external_changed(move |_| refresh()),
            refresh,
        );
    }
    model
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use reprise_core::up_next::QueueItem;

    use super::*;

    #[test]
    fn shared_queue_refresh_subscribes_to_queue_and_external_changes() {
        let queue_callback: Rc<RefCell<Option<RefreshCallback>>> = Rc::new(RefCell::new(None));
        let external_callback: Rc<RefCell<Option<RefreshCallback>>> = Rc::new(RefCell::new(None));
        let generation = Rc::new(Cell::new(0_i64));
        let model = Rc::new(RefCell::new(QueueViewModel::default()));
        let refresh = {
            let generation = generation.clone();
            let model = model.clone();
            Rc::new(move || {
                let next = generation.get() + 1;
                generation.set(next);
                model.borrow_mut().items = vec![QueueItem::Episode(next)];
            }) as RefreshCallback
        };

        install_refresh_callbacks(
            {
                let callback = queue_callback.clone();
                move |refresh| *callback.borrow_mut() = Some(refresh)
            },
            {
                let callback = external_callback.clone();
                move |refresh| *callback.borrow_mut() = Some(refresh)
            },
            refresh,
        );

        queue_callback.borrow().as_ref().unwrap()();
        assert_eq!(model.borrow().items, vec![QueueItem::Episode(1)]);
        external_callback.borrow().as_ref().unwrap()();
        assert_eq!(model.borrow().items, vec![QueueItem::Episode(2)]);
    }
}
