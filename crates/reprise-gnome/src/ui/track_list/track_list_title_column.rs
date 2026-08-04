//! Title-column factory: playing marker and title badges.
//!
//! Extracted as one cohesive cell factory to keep the shared column module
//! below the project's 800-line code-file cap.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::models::Track;
use reprise_core::queries::QueueItemMetadata;

use super::now_playing_marker;
use super::track_list_columns::{
    ai_badge_visible, apply_missing_title, apply_now_playing_item, build_playing_marker,
    clear_missing_title, toggle_class, NOW_PLAYING_CLASS, NOW_PLAYING_TITLE_CLASS,
};
use super::{
    list_density, match_highlight, queue_item_presentation, strings, track_list_context_menu,
    track_list_dnd, track_list_row_interaction, Shared,
};

pub(in crate::ui) fn append_title_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    let shared_for_bind = shared.clone();
    let shared_for_unbind = shared.clone();
    let shared = shared.clone();
    let column_view_for_setup = column_view.clone();
    factory.connect_setup(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("title column setup: object is not a ListItem");
            return;
        };
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        track_list_row_interaction::expand_to_cell(&row);
        let eq = build_playing_marker();
        eq.set_visible(false);
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_hexpand(true);
        row.append(&eq);
        row.append(&label);
        let ai_badge = gtk4::Label::new(Some(&strings::text(strings::AI_BADGE_LABEL)));
        ai_badge.add_css_class("stats-badge");
        ai_badge.set_tooltip_text(Some(&strings::text(strings::AI_BADGE_TOOLTIP)));
        ai_badge.set_visible(false);
        row.append(&ai_badge);
        track_list_context_menu::wire_context_menu_gesture(
            &row,
            item,
            &shared,
            &column_view_for_setup,
        );
        track_list_dnd::wire_row_dnd(&row, item, &shared);
        item.set_child(Some(&row));
        list_density::inherit(&column_view_for_setup, &row);
    });

    factory.connect_bind(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("title column bind: object is not a ListItem");
            return;
        };
        let Some(row) = item
            .child()
            .and_then(|widget| widget.downcast::<gtk4::Box>().ok())
        else {
            tracing::warn!("title column bind: title cell is not a Box");
            return;
        };
        let Some(eq) = row.first_child() else {
            tracing::warn!("title column bind: title cell has no equaliser child");
            return;
        };
        let Some(label) = eq
            .next_sibling()
            .and_then(|widget| widget.downcast::<gtk4::Label>().ok())
        else {
            tracing::warn!("title column bind: title cell has no label child");
            return;
        };
        let Some(boxed) = item
            .item()
            .and_then(|object| object.downcast::<glib::BoxedAnyObject>().ok())
        else {
            tracing::warn!("title column bind: item is not typed queue metadata");
            return;
        };
        let metadata = boxed.borrow::<QueueItemMetadata>();
        let track = queue_item_presentation::track(&metadata);
        if track.is_some_and(Track::is_missing) {
            let track = track.expect("checked above");
            label.set_text(&track.title);
            apply_missing_title(&label, track);
        } else if let Some(track) = track {
            apply_missing_title(&label, track);
            match match_highlight::highlight_from_filter(&track.title, &shared_for_bind.filter, {
                let label = label.clone();
                move || match_highlight::accent_foreground(&label)
            }) {
                Some(markup) => label.set_markup(&markup),
                None => label.set_text(&track.title),
            }
        } else {
            clear_missing_title(&label);
            label.set_text(queue_item_presentation::title(&metadata));
        }
        let playing = apply_now_playing_item(&row, &metadata, &shared_for_bind, false);
        eq.set_visible(playing);
        toggle_class(&label, NOW_PLAYING_TITLE_CLASS, playing);
        if let Some(ai_badge) = label.next_sibling() {
            ai_badge.set_visible(track.is_some_and(|track| ai_badge_visible(track.is_ai)));
        }
        let track_id = queue_item_presentation::rating_track_id(&metadata);
        now_playing_marker::register_cell(&shared_for_bind, item, {
            let row = row.clone();
            let eq = eq.clone();
            let label = label.clone();
            move |shared| {
                let playing = track_id
                    .is_some_and(|track_id| shared.playing_track_id.get() == Some(track_id));
                toggle_class(&row, NOW_PLAYING_CLASS, playing);
                eq.set_visible(playing);
                toggle_class(&label, NOW_PLAYING_TITLE_CLASS, playing);
            }
        });
    });

    factory.connect_unbind(move |_, obj| {
        if let Some(item) = obj.downcast_ref::<gtk4::ListItem>() {
            now_playing_marker::unregister_cell(&shared_for_unbind, item);
        }
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::COLUMN_TITLE))
        .factory(&factory)
        .resizable(true)
        .build();
    column.set_id(Some("title"));
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));
    column_view.append_column(&column);
    column
}
