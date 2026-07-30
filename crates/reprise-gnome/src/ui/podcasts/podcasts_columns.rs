use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use reprise_core::podcasts::EpisodeRow;

use super::podcasts_context_menu;
use super::podcasts_model::PodcastEpisodeObject;
use super::podcasts_presentation::{duration, relative_date, source_pill, status_pill};
use crate::ui::strings;

pub(super) type OnUnsubscribe = Rc<dyn Fn(i64)>;
pub(super) type IsPlaying = Rc<dyn Fn(i64) -> bool>;

pub(super) struct PodcastColumns {
    pub date: gtk4::ColumnViewColumn,
}

fn text_column(
    view: &gtk4::ColumnView,
    title: &str,
    expand: bool,
    render: impl Fn(&EpisodeRow) -> String + 'static,
    is_playing: &IsPlaying,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        let surface = crate::ui::source_context_surface::wrap(&label);
        podcasts_context_menu::wire_gesture(&surface, item);
        item.set_child(Some(&surface));
    });
    let is_playing = is_playing.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(label) = surface.first_child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<PodcastEpisodeObject>() else {
            return;
        };
        let row = object.row();
        label.set_text(&render(&row));
        if is_playing(row.id) {
            label.add_css_class("reprise-podcast-playing");
        } else {
            label.remove_css_class("reprise-podcast-playing");
        }
    });
    factory.connect_unbind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        if let Some(label) = surface.first_child().and_downcast::<gtk4::Label>() {
            label.set_text("");
            label.remove_css_class("reprise-podcast-playing");
        }
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(true)
        .expand(expand)
        .build();
    view.append_column(&column);
    column
}

fn pill_column(view: &gtk4::ColumnView, title: &str, source: bool, is_playing: &IsPlaying) {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        let icon = gtk4::Image::new();
        let label = gtk4::Label::new(None);
        cell.append(&icon);
        cell.append(&label);
        let surface = crate::ui::source_context_surface::wrap(&cell);
        podcasts_context_menu::wire_gesture(&surface, item);
        item.set_child(Some(&surface));
    });
    let is_playing = is_playing.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(cell) = surface.first_child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(icon) = cell.first_child().and_downcast::<gtk4::Image>() else {
            return;
        };
        let Some(label) = icon.next_sibling().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<PodcastEpisodeObject>() else {
            return;
        };
        let row = object.row();
        let pill = if source {
            source_pill(row.kind)
        } else {
            status_pill(&row)
        };
        icon.set_icon_name(pill.icon);
        icon.set_visible(pill.icon.is_some());
        label.set_text(&strings::text(pill.label));
        cell.set_css_classes(&[pill.css_class]);
        if is_playing(row.id) {
            cell.add_css_class("reprise-podcast-playing");
        }
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(false)
        .build();
    view.append_column(&column);
}

fn unsubscribe_column(view: &gtk4::ColumnView, on_unsubscribe: &OnUnsubscribe) {
    let factory = gtk4::SignalListItemFactory::new();
    let callback = on_unsubscribe.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let button = gtk4::Button::from_icon_name("starred-symbolic");
        button.add_css_class("flat");
        button.add_css_class("accent");
        button.set_opacity(0.0);
        let item_weak = item.downgrade();
        let callback = callback.clone();
        button.connect_clicked(move |_| {
            let Some(item) = item_weak.upgrade() else {
                return;
            };
            let Some(object) = item.item().and_downcast::<PodcastEpisodeObject>() else {
                return;
            };
            callback(object.row().subscription_id);
        });
        let motion = gtk4::EventControllerMotion::new();
        let weak = button.downgrade();
        motion.connect_enter(move |_, _, _| {
            if let Some(button) = weak.upgrade() {
                button.set_opacity(1.0);
            }
        });
        let weak = button.downgrade();
        motion.connect_leave(move |_| {
            if let Some(button) = weak.upgrade() {
                button.set_opacity(0.0);
            }
        });
        button.add_controller(motion);
        let surface = crate::ui::source_context_surface::wrap(&button);
        podcasts_context_menu::wire_gesture(&surface, item);
        item.set_child(Some(&surface));
    });
    factory.connect_bind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(button) = surface.first_child().and_downcast::<gtk4::Button>() else {
            return;
        };
        let Some(row) = item.item().and_downcast::<PodcastEpisodeObject>() else {
            return;
        };
        button.set_tooltip_text(Some(&strings::podcast_unsubscribe_from(&row.row().show)));
    });
    let column = gtk4::ColumnViewColumn::builder()
        .factory(&factory)
        .resizable(false)
        .build();
    view.append_column(&column);
}

pub(super) fn append_columns(
    view: &gtk4::ColumnView,
    on_unsubscribe: &OnUnsubscribe,
    is_playing: &IsPlaying,
) -> PodcastColumns {
    let date = text_column(
        view,
        &strings::text(strings::PODCAST_DATE),
        false,
        |row| relative_date(row.published_at, Local::now().date_naive()),
        is_playing,
    );
    let sorter = gtk4::CustomSorter::new(|left, right| {
        let left = left
            .downcast_ref::<PodcastEpisodeObject>()
            .and_then(|object| object.row().published_at);
        let right = right
            .downcast_ref::<PodcastEpisodeObject>()
            .and_then(|object| object.row().published_at);
        left.cmp(&right).into()
    });
    date.set_sorter(Some(&sorter));
    view.sort_by_column(Some(&date), gtk4::SortType::Descending);
    text_column(
        view,
        &strings::text(strings::PODCAST_EPISODE),
        true,
        |row| row.title.clone(),
        is_playing,
    );
    text_column(
        view,
        &strings::text(strings::PODCAST_SHOW),
        true,
        |row| row.show.clone(),
        is_playing,
    );
    text_column(
        view,
        &strings::text(strings::PODCAST_LENGTH),
        false,
        |row| duration(row.duration_secs),
        is_playing,
    );
    pill_column(
        view,
        &strings::text(strings::PODCAST_SOURCE),
        true,
        is_playing,
    );
    pill_column(
        view,
        &strings::text(strings::PODCAST_STATUS),
        false,
        is_playing,
    );
    unsubscribe_column(view, on_unsubscribe);
    PodcastColumns { date }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::podcasts::PodcastKind;

    use crate::ui::source_context_surface;

    fn episode() -> EpisodeRow {
        EpisodeRow {
            id: 1,
            subscription_id: 1,
            guid: "g".into(),
            title: "Episode".into(),
            show: "Show".into(),
            show_image_url: None,
            image_url: None,
            kind: PodcastKind::Rss,
            audio_url: "https://example.test/e.mp3".into(),
            page_url: None,
            published_at: None,
            duration_secs: None,
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: 1,
        }
    }

    /// The whole row is one context-menu target — not just the text inside
    /// each cell's padding. Built from the production factories, so a column
    /// that forgets [`source_context_surface::wrap`] fails here.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_1_every_point_of_a_podcast_row_reaches_the_context_menu() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&source_context_surface::css());

        let store = gtk4::gio::ListStore::new::<PodcastEpisodeObject>();
        store.append(&PodcastEpisodeObject::new(episode()));
        let view = gtk4::ColumnView::new(Some(gtk4::SingleSelection::new(Some(store))));
        view.add_css_class(source_context_surface::TABLE_CSS_CLASS);
        let on_unsubscribe: OnUnsubscribe = Rc::new(|_| {});
        let is_playing: IsPlaying = Rc::new(|_| false);
        append_columns(&view, &on_unsubscribe, &is_playing);

        let window = gtk4::Window::new();
        window.set_default_size(1200, 400);
        window.set_child(Some(&view));
        window.present();
        source_context_surface::settle_layout();

        let uncovered = source_context_surface::row_points_without_a_surface(&view);
        assert!(
            uncovered.is_empty(),
            "podcast row points without a context surface: {uncovered:?}"
        );
    }
}
