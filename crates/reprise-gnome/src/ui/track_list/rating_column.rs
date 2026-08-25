//! Interactive rating-column factory and rating write-back.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::stats;
use reprise_core::queries::QueueItemMetadata;
use reprise_core::up_next::QueueItem;

use super::now_playing_marker;
use super::rating_cell_refresh;
use super::track_list_columns::{
    apply_now_playing, apply_now_playing_item, rating_refresh_for_sort, RatingRefresh,
};
use super::{reload, show_toast, Shared};
use crate::ui::rating::RatingWidget;
use crate::ui::strings;
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd;
use crate::ui::track_list_row_interaction;

pub(in crate::ui) fn append_rating_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    {
        let shared = shared.clone();
        let column_view = column_view.clone();
        factory.connect_setup(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("rating column setup: object is not a ListItem");
                return;
            };
            let rating_widget = RatingWidget::new();
            track_list_row_interaction::expand_to_cell(&rating_widget);
            track_list_context_menu::wire_context_menu_gesture(
                &rating_widget,
                item,
                &shared,
                &column_view,
            );
            super::track_list_selection_input::wire_cell_selection(&rating_widget, item, &shared);
            track_list_dnd::wire_row_dnd(&rating_widget, item, &shared);
            item.set_child(Some(&rating_widget));
        });
    }

    {
        let shared = shared.clone();
        factory.connect_bind(move |_, obj| bind(obj, &shared));
    }

    let shared_for_unbind = shared.clone();
    factory.connect_unbind(move |_, obj| unbind(obj, &shared_for_unbind));

    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::RATING))
        .factory(&factory)
        .resizable(true)
        .build();
    column.set_id(Some("rating"));
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));
    column_view.append_column(&column);
    column
}

fn bind(obj: &glib::Object, shared: &Rc<Shared>) {
    let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
        tracing::warn!("rating column bind: object is not a ListItem");
        return;
    };
    let Some(rating_widget) = item.child().and_then(|w| w.downcast::<RatingWidget>().ok()) else {
        tracing::warn!("rating column bind: list item child is not a RatingWidget");
        return;
    };
    let Some(boxed) = item
        .item()
        .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
    else {
        tracing::warn!("rating column bind: item is not typed queue metadata");
        return;
    };
    let metadata = boxed.borrow::<QueueItemMetadata>();
    let track = super::queue_item_presentation::track(&metadata);
    let binding_changed = rating_widget.set_bound_track(track.map(|track| track.id));
    rating_widget.set_on_changed(|_| {});
    if binding_changed {
        rating_cell_refresh::unregister_cell(shared, item);
        now_playing_marker::unregister_cell(shared, item);
    }
    let Some(track) = track else {
        rating_widget.set_visible(false);
        rating_widget.set_rating(0);
        apply_now_playing_item(&rating_widget, &metadata, shared, false);
        return;
    };
    rating_widget.set_visible(true);
    rating_widget.set_rating(track.rating);
    if binding_changed {
        rating_cell_refresh::register_cell(shared, item, track.id, {
            let rating_widget = rating_widget.clone();
            move |rating| rating_widget.set_rating(rating)
        });
    }
    apply_now_playing(&rating_widget, track.id, shared, false);
    if binding_changed {
        now_playing_marker::register_cell(shared, item, {
            let rating_widget = rating_widget.clone();
            let track_id = track.id;
            move |shared| {
                apply_now_playing(&rating_widget, track_id, shared, false);
            }
        });
    }

    let queue_item = metadata.item();
    let title = track.title.clone();
    let position = item.position();
    let shared = shared.clone();
    rating_widget.set_on_changed(move |new_rating| {
        on_rating_changed(&shared, queue_item, &title, position, new_rating);
    });
}

fn unbind(obj: &glib::Object, shared: &Shared) {
    let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
        return;
    };
    now_playing_marker::unregister_cell(shared, item);
    rating_cell_refresh::unregister_cell(shared, item);
    let Some(rating_widget) = item.child().and_then(|w| w.downcast::<RatingWidget>().ok()) else {
        return;
    };
    rating_widget.set_bound_track(None);
    rating_widget.set_on_changed(|_| {});
}

fn on_rating_changed(
    shared: &Rc<Shared>,
    item: QueueItem,
    title: &str,
    position: u32,
    new_rating: i32,
) {
    let Some(track_id) = super::queue_item_presentation::rating_write_target(item) else {
        tracing::warn!(
            ?item,
            position,
            "rating change rejected for non-track queue row"
        );
        return;
    };
    tracing::debug!(track_id, position, new_rating, "rating changed");
    let result = stats::set_rating(&shared.conn, track_id, new_rating);
    match result {
        Ok(()) => match rating_refresh_for_sort(&shared.sort.borrow().field) {
            RatingRefresh::Row => shared.model.set_cached_rating(position, new_rating),
            RatingRefresh::Query => reload(shared),
        },
        Err(error) => {
            tracing::error!(%error, track_id, new_rating, "failed to persist rating change");
            show_toast(shared, &strings::rating_save_failed_toast(title));
        }
    }
}
