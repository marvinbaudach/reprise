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

/// Gates a *surface* refresh on the model actually having changed.
///
/// External-media changes are far more numerous than queue changes: a radio
/// stream re-tags itself on every song, and pausing a podcast is a phase
/// change too. Only some of them alter the projected queue model — starting,
/// skipping or ending a direct episode does, a radio title or a pause does
/// not. Re-rendering the Queue ColumnView for the latter would emit
/// `items_changed` for an identical list, and this project has already paid
/// for that once: the resulting focus reset drops the focused row to 0 and
/// the view visibly jumps.
///
/// The model itself is rebuilt unconditionally (see [`build`], registered
/// first); this only decides whether the surfaces need to re-render, which is
/// exactly the split the module doc describes.
pub(super) fn refresh_on_model_change(
    model: &SharedQueueModel,
    refresh: RefreshCallback,
) -> RefreshCallback {
    let model_weak = Rc::downgrade(model);
    let last = RefCell::new(model.borrow().clone());
    Rc::new(move || {
        let Some(model) = model_weak.upgrade() else {
            return;
        };
        // The borrows end here, before `refresh` runs: a surface refresh
        // reads the model again and would otherwise hit an active borrow.
        {
            let current = model.borrow();
            if *current == *last.borrow() {
                return;
            }
            *last.borrow_mut() = current.clone();
        }
        refresh();
    })
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

    /// QUE-10: a radio re-tag and a podcast pause both reach
    /// `add_on_external_changed` without touching the projected model. The
    /// surfaces must not re-render for those — an `items_changed` over an
    /// identical list resets the Queue view's focused row to 0.
    #[test]
    fn que_10_a_surface_refresh_skips_external_changes_that_leave_the_model_alone() {
        let model: SharedQueueModel = Rc::new(RefCell::new(QueueViewModel::default()));
        let renders = Rc::new(Cell::new(0_u32));
        let gated = {
            let renders = renders.clone();
            refresh_on_model_change(&model, Rc::new(move || renders.set(renders.get() + 1)))
        };

        gated();
        assert_eq!(renders.get(), 0, "an unchanged model renders nothing");

        model.borrow_mut().items = vec![QueueItem::Episode(7)];
        gated();
        assert_eq!(renders.get(), 1, "a changed model renders once");

        gated();
        assert_eq!(
            renders.get(),
            1,
            "the same change must not render a second time"
        );

        model.borrow_mut().items = vec![QueueItem::Episode(8)];
        gated();
        assert_eq!(renders.get(), 2, "the next real change renders again");
    }

    /// The gate reads the model inside the refresh it guards, so it must not
    /// still hold a borrow when the surface callback runs.
    #[test]
    fn que_10_the_gate_leaves_no_borrow_open_for_the_surface_refresh() {
        let model: SharedQueueModel = Rc::new(RefCell::new(QueueViewModel::default()));
        let gated = {
            let model = model.clone();
            refresh_on_model_change(
                &model.clone(),
                Rc::new(move || {
                    let _read_again = model.borrow().items.len();
                }),
            )
        };

        model.borrow_mut().items = vec![QueueItem::Episode(3)];
        gated();
    }
}
