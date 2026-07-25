//! Five-track songs card and its expandable sortable full ranking.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen::TopTrack;
use reprise_core::library::stats_snapshot::{SortBy, StatsSnapshot};

use super::stats_metadata_links::{self, MetadataCallback, StatsMetadataTarget};
use super::stats_view_widgets::{card, clear, label};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

const SONG_ROW_LIMIT: usize = 5;
const FULL_TRACK_LIMIT: usize = 10;

type IdCallback = Rc<RefCell<Option<Rc<dyn Fn(i64)>>>>;

#[derive(Clone, Default)]
struct CoverGenerations {
    summary: Rc<Cell<u64>>,
    full: Rc<Cell<u64>>,
}

impl CoverGenerations {
    fn next_summary(&self) -> u64 {
        next_generation(&self.summary)
    }

    fn next_full(&self) -> u64 {
        next_generation(&self.full)
    }
}

#[derive(Clone)]
pub(in crate::ui) struct StatsSongsCard {
    root: gtk4::Box,
    rows: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    revealer: gtk4::Revealer,
    #[cfg_attr(not(test), allow(dead_code))]
    reveal_button: gtk4::Button,
    full_rows: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    plays_sort: gtk4::ToggleButton,
    #[cfg_attr(not(test), allow(dead_code))]
    time_sort: gtk4::ToggleButton,
    play_buttons: Rc<RefCell<Vec<gtk4::Button>>>,
    summary_bars: Rc<RefCell<Vec<gtk4::LevelBar>>>,
    row_clicks: Rc<RefCell<Vec<gtk4::GestureClick>>>,
    snapshot: Rc<RefCell<Option<StatsSnapshot>>>,
    sort_by: Rc<Cell<SortBy>>,
    cover_loader: Rc<CoverLoader>,
    cover_generations: CoverGenerations,
    metadata: MetadataCallback,
    on_play_track: IdCallback,
    on_play_next: IdCallback,
    on_add_to_queue: IdCallback,
    #[cfg_attr(not(test), allow(dead_code))]
    context_actions: Rc<RefCell<Vec<gio::SimpleActionGroup>>>,
}

impl StatsSongsCard {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>, metadata: MetadataCallback) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("stats-songs-card");
        let kicker = label("MOST PLAYED SONGS", "stats-eyebrow");
        root.append(&kicker);

        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.append(&rows);

        let reveal_button = gtk4::Button::with_label("Show all top tracks");
        reveal_button.add_css_class("flat");
        reveal_button.add_css_class("stats-songs-reveal");
        reveal_button.set_halign(gtk4::Align::Start);
        root.append(&reveal_button);

        let full_rows = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let plays_sort = gtk4::ToggleButton::with_label("by plays");
        let time_sort = gtk4::ToggleButton::with_label("by time");
        time_sort.set_group(Some(&plays_sort));
        plays_sort.set_active(true);
        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        controls.set_halign(gtk4::Align::End);
        controls.append(&plays_sort);
        controls.append(&time_sort);
        let expanded_content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        expanded_content.append(&controls);
        expanded_content.append(&full_rows);
        let expanded = card(&expanded_content);
        expanded.set_hexpand(true);
        let revealer = gtk4::Revealer::new();
        revealer.set_reveal_child(false);
        revealer.set_child(Some(&expanded));

        let snapshot = Rc::new(RefCell::new(None::<StatsSnapshot>));
        let sort_by = Rc::new(Cell::new(SortBy::Plays));
        let cover_generations = CoverGenerations::default();
        let on_play_track: IdCallback = Rc::new(RefCell::new(None));
        let on_play_next: IdCallback = Rc::new(RefCell::new(None));
        let on_add_to_queue: IdCallback = Rc::new(RefCell::new(None));
        let context_actions = Rc::new(RefCell::new(Vec::new()));
        let play_buttons = Rc::new(RefCell::new(Vec::new()));
        let summary_bars = Rc::new(RefCell::new(Vec::new()));
        let row_clicks = Rc::new(RefCell::new(Vec::new()));

        reveal_button.connect_clicked(glib::clone!(
            #[weak]
            revealer,
            move |button| {
                let reveal = !revealer.reveals_child();
                revealer.set_reveal_child(reveal);
                button.set_label(if reveal {
                    "Hide top tracks"
                } else {
                    "Show all top tracks"
                });
            }
        ));

        for (button, value) in [(&plays_sort, SortBy::Plays), (&time_sort, SortBy::Time)] {
            button.connect_toggled({
                let full_rows = full_rows.clone();
                let snapshot = snapshot.clone();
                let sort_by = sort_by.clone();
                let cover_loader = cover_loader.clone();
                let cover_generations = cover_generations.clone();
                let metadata = metadata.clone();
                move |button| {
                    if !button.is_active() {
                        return;
                    }
                    sort_by.set(value);
                    if let Some(snapshot) = snapshot.borrow().as_ref() {
                        render_full_rows(
                            &full_rows,
                            snapshot,
                            value,
                            &cover_loader,
                            &cover_generations,
                            &metadata,
                        );
                    }
                }
            });
        }

        Self {
            root,
            rows,
            revealer,
            reveal_button,
            full_rows,
            plays_sort,
            time_sort,
            play_buttons,
            summary_bars,
            row_clicks,
            snapshot,
            sort_by,
            cover_loader,
            cover_generations,
            metadata,
            on_play_track,
            on_play_next,
            on_add_to_queue,
            context_actions,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn expanded_widget(&self) -> &gtk4::Revealer {
        &self.revealer
    }

    pub(in crate::ui) fn set_data(&self, snapshot: &StatsSnapshot) {
        if snapshot.top_tracks.is_empty() {
            self.revealer.set_reveal_child(false);
            self.reveal_button.set_label("Show all top tracks");
        }
        *self.snapshot.borrow_mut() = Some(snapshot.clone());
        self.render_summary(snapshot);
        render_full_rows(
            &self.full_rows,
            snapshot,
            self.sort_by.get(),
            &self.cover_loader,
            &self.cover_generations,
            &self.metadata,
        );
    }

    fn render_summary(&self, snapshot: &StatsSnapshot) {
        clear(&self.rows);
        self.context_actions.borrow_mut().clear();
        self.play_buttons.borrow_mut().clear();
        self.summary_bars.borrow_mut().clear();
        self.row_clicks.borrow_mut().clear();
        let tracks = snapshot.top_tracks_sorted(SortBy::Plays);
        let leader = tracks.first().map_or(0, |track| track.play_count);
        let token = self.cover_generations.next_summary();
        for track in tracks.iter().take(SONG_ROW_LIMIT) {
            let row = self.song_row(track, leader, token);
            self.rows.append(&row);
        }
    }

    fn song_row(&self, track: &TopTrack, leader: i64, token: u64) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.add_css_class("stats-song-row");
        row.set_hexpand(true);
        row.set_focusable(true);

        let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        let cover_overlay = gtk4::Overlay::new();
        let cover = gtk4::Image::builder()
            .pixel_size(40)
            .width_request(40)
            .height_request(40)
            .build();
        CoverLoader::set_placeholder(&cover);
        self.cover_loader.load_into(
            &cover,
            &track.track_path,
            ThumbnailSize::List,
            token,
            &self.cover_generations.summary,
        );
        cover_overlay.set_child(Some(&cover));
        let play = gtk4::Button::from_icon_name("media-playback-start-symbolic");
        play.add_css_class("circular");
        play.add_css_class("stats-song-play");
        play.set_tooltip_text(Some("Play this track"));
        play.set_visible(false);
        play.connect_clicked({
            let callback = self.on_play_track.clone();
            let track_id = track.track_id;
            move |_| invoke_id(&callback, track_id)
        });
        cover_overlay.add_overlay(&play);
        install_play_visibility(&row, &play);
        self.play_buttons.borrow_mut().push(play);
        body.append(&cover_overlay);

        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.append(&label(&track.title, "stats-item-title"));
        text.append(&label(&track.artist, "stats-item-subtitle"));
        body.append(&text);

        let bar = gtk4::LevelBar::new();
        bar.add_css_class("stats-song-bar");
        bar.set_min_value(0.0);
        bar.set_max_value(1.0);
        bar.set_value(relative_value(track.play_count, leader));
        bar.set_width_request(110);
        bar.set_height_request(8);
        bar.set_valign(gtk4::Align::Center);
        self.summary_bars.borrow_mut().push(bar.clone());
        body.append(&bar);
        let plays = label(
            &format!("{} plays", format_thousands(track.play_count)),
            "stats-play-count",
        );
        plays.set_xalign(1.0);
        body.append(&plays);
        row.append(&body);
        let activate = gtk4::GestureClick::new();
        activate.set_button(1);
        activate.connect_released({
            let callback = self.metadata.clone();
            let target = StatsMetadataTarget::Artist {
                track_id: track.track_id,
                artist: track.effective_artist.clone(),
            };
            move |_, _, _, _| invoke_metadata(&callback, target.clone())
        });
        row.add_controller(activate.clone());
        self.row_clicks.borrow_mut().push(activate);
        self.install_context_menu(&row, track);
        row
    }

    fn install_context_menu(&self, row: &gtk4::Box, track: &TopTrack) {
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
            let metadata = self.metadata.clone();
            let artist_target = StatsMetadataTarget::Artist {
                track_id: track.track_id,
                artist: track.effective_artist.clone(),
            };
            move |controller, key, _, modifiers| {
                if key == gtk4::gdk::Key::Return || key == gtk4::gdk::Key::KP_Enter {
                    invoke_metadata(&metadata, artist_target.clone());
                    return glib::Propagation::Stop;
                }
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
        self.context_actions.borrow_mut().push(actions);
    }

    pub(in crate::ui) fn set_on_play_track(&self, callback: impl Fn(i64) + 'static) {
        *self.on_play_track.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_play_next(&self, callback: impl Fn(i64) + 'static) {
        *self.on_play_next.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_add_to_queue(&self, callback: impl Fn(i64) + 'static) {
        *self.on_add_to_queue.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn summary_bars(&self) -> Vec<gtk4::LevelBar> {
        self.summary_bars.borrow().clone()
    }
}

fn install_play_visibility(row: &gtk4::Box, play: &gtk4::Button) {
    let hovered = Rc::new(Cell::new(false));
    let focused = Rc::new(Cell::new(false));
    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter({
        let play = play.clone();
        let hovered = hovered.clone();
        move |_, _, _| {
            hovered.set(true);
            play.set_visible(true);
        }
    });
    motion.connect_leave({
        let play = play.clone();
        let hovered = hovered.clone();
        let focused = focused.clone();
        move |_| {
            hovered.set(false);
            play.set_visible(focused.get());
        }
    });
    row.add_controller(motion);
    play.connect_has_focus_notify({
        let hovered = hovered.clone();
        move |button| button.set_visible(button.has_focus() || hovered.get())
    });
    let focus = gtk4::EventControllerFocus::new();
    focus.connect_contains_focus_notify({
        let play = play.clone();
        move |focus| {
            focused.set(focus.contains_focus());
            play.set_visible(focus.contains_focus() || hovered.get());
        }
    });
    row.add_controller(focus);
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

fn render_full_rows(
    container: &gtk4::Box,
    snapshot: &StatsSnapshot,
    sort_by: SortBy,
    cover_loader: &Rc<CoverLoader>,
    generations: &CoverGenerations,
    metadata: &MetadataCallback,
) {
    clear(container);
    let token = generations.next_full();
    let tracks = snapshot.top_tracks_sorted(sort_by);
    let leader = tracks.first().map_or(0, |track| metric(track, sort_by));
    for (index, track) in tracks.iter().take(FULL_TRACK_LIMIT).enumerate() {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.add_css_class("stats-top-track-row");
        row.set_height_request(56);
        let rank = label(&(index + 1).to_string(), "stats-rank");
        rank.set_width_request(24);
        rank.set_xalign(1.0);
        row.append(&rank);
        let cover = gtk4::Image::builder()
            .pixel_size(42)
            .width_request(42)
            .height_request(42)
            .build();
        CoverLoader::set_placeholder(&cover);
        cover_loader.load_into(
            &cover,
            &track.track_path,
            ThumbnailSize::List,
            token,
            &generations.full,
        );
        row.append(&cover);
        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.append(&stats_metadata_links::button(
            &track.title,
            "stats-item-title",
            StatsMetadataTarget::Track(track.track_id),
            metadata,
        ));
        text.append(&stats_metadata_links::button(
            &track.artist,
            "stats-item-subtitle",
            StatsMetadataTarget::Artist {
                track_id: track.track_id,
                artist: track.effective_artist.clone(),
            },
            metadata,
        ));
        row.append(&text);
        let bar = gtk4::LevelBar::new();
        bar.add_css_class("stats-song-bar");
        bar.set_min_value(0.0);
        bar.set_max_value(1.0);
        bar.set_value(relative_value(metric(track, sort_by), leader));
        bar.set_width_request(120);
        bar.set_height_request(8);
        bar.set_valign(gtk4::Align::Center);
        row.append(&bar);
        row.append(&label(
            &format!(
                "{} plays · {}",
                format_thousands(track.play_count),
                strings::stats_duration(track.total_ms)
            ),
            "stats-play-count",
        ));
        container.append(&row);
    }
}

fn next_generation(generation: &Cell<u64>) -> u64 {
    let token = generation.get().wrapping_add(1);
    generation.set(token);
    token
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

fn metric(track: &TopTrack, sort_by: SortBy) -> i64 {
    match sort_by {
        SortBy::Plays => track.play_count,
        SortBy::Time => track.total_ms,
    }
}

fn relative_value(value: i64, leader: i64) -> f64 {
    if leader <= 0 {
        0.0
    } else {
        value.max(0) as f64 / leader as f64
    }
}

#[cfg(test)]
mod tests {
    use chrono::Datelike;
    use reprise_core::library::stats_period::StatsPeriod;
    use reprise_core::library::stats_snapshot;

    use super::*;

    #[test]
    fn summary_cover_generation_survives_rendering_the_full_ranking() {
        let generations = CoverGenerations::default();
        let summary_token = generations.next_summary();

        generations.next_full();

        assert_eq!(generations.summary.get(), summary_token);
    }

    fn card_and_snapshot(metadata: MetadataCallback) -> (StatsSongsCard, StatsSnapshot) {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        for id in 1..=6_i64 {
            conn.execute(
                "INSERT INTO tracks \
                 (id, path, title, artist, album, album_artist, genre, duration_ms, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, '', 'Rock', 300000, 0)",
                rusqlite::params![
                    id,
                    format!("/music/{id}.flac"),
                    format!("Track {id}"),
                    format!("Artist {id}"),
                    format!("Album {id}")
                ],
            )
            .unwrap();
            for play in 0..=(6 - id) {
                conn.execute(
                    "INSERT INTO listen_events (track_id, played_at, ms_played) \
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, now - play, id * 60_000],
                )
                .unwrap();
            }
        }
        let snapshot = stats_snapshot::compute(
            &conn,
            StatsPeriod::YearToDate(chrono::Local::now().year()),
            now,
            &chrono::Local,
        )
        .unwrap();
        let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
        (StatsSongsCard::new(loader, metadata), snapshot)
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_14_song_row_focuses_track_in_artist_scope() {
        gtk4::init().unwrap();
        let target = Rc::new(RefCell::new(None));
        let metadata: MetadataCallback = Rc::new(RefCell::new(Some({
            let target = target.clone();
            Rc::new(move |value| *target.borrow_mut() = Some(value))
        })));
        let (card, snapshot) = card_and_snapshot(metadata);
        card.set_data(&snapshot);

        card.row_clicks.borrow()[0].emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);

        assert!(matches!(
            target.borrow().as_ref(),
            Some(StatsMetadataTarget::Artist {
                track_id: 1,
                artist
            }) if artist == "Artist 1"
        ));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_14_hover_play_targets_exactly_one_track() {
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
        let metadata: MetadataCallback = Rc::new(RefCell::new(None));
        let (card, snapshot) = card_and_snapshot(metadata);
        let played = Rc::new(RefCell::new(Vec::new()));
        card.set_on_play_track({
            let played = played.clone();
            move |id| played.borrow_mut().push(id)
        });
        card.set_data(&snapshot);

        card.play_buttons.borrow()[2].emit_clicked();

        assert_eq!(*played.borrow(), vec![3]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_14_show_all_reveals_the_sortable_list() {
        gtk4::init().unwrap();
        let metadata: MetadataCallback = Rc::new(RefCell::new(None));
        let (card, snapshot) = card_and_snapshot(metadata);
        card.set_data(&snapshot);

        assert!(!card.revealer.reveals_child());
        assert!(
            card.revealer.parent().is_none(),
            "the expanded ranking must not live inside the songs card"
        );
        let stage = gtk4::Box::new(gtk4::Orientation::Vertical, 20);
        stage.append(card.widget());
        stage.append(card.expanded_widget());
        let window = gtk4::Window::builder()
            .default_width(960)
            .child(&stage)
            .build();
        window.present();
        card.revealer
            .set_transition_type(gtk4::RevealerTransitionType::None);
        assert_eq!(
            card.reveal_button.label().as_deref(),
            Some("Show all top tracks")
        );
        assert_eq!(card.rows.observe_children().n_items(), 5);
        card.reveal_button.emit_clicked();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(card.revealer.reveals_child());
        card.time_sort.set_active(true);
        assert_eq!(card.sort_by.get(), SortBy::Time);
        assert_eq!(card.full_rows.observe_children().n_items(), 6);
        card.plays_sort.set_active(true);
        assert_eq!(card.sort_by.get(), SortBy::Plays);
        // The sort toggle rebuilds the rows after the initial layout pass.
        // A non-blocking pump does not tick a frame, so the fresh widgets
        // would still report an unallocated 0 — run a real main loop briefly.
        {
            let main_loop = gtk4::glib::MainLoop::new(None, false);
            let quit = main_loop.clone();
            gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                quit.quit();
            });
            main_loop.run();
        }
        let row = card.full_rows.first_child().unwrap();
        let rank = row.first_child().unwrap();
        let cover = rank.next_sibling().unwrap();
        let text = cover.next_sibling().unwrap();
        let bar = text.next_sibling().unwrap();
        assert_eq!(row.height_request(), 56);
        assert!(
            row.height() <= 64,
            "expanded row was {} px tall",
            row.height()
        );
        assert_eq!(rank.width(), 24);
        assert_eq!((cover.width(), cover.height()), (42, 42));
        assert_eq!(bar.height(), 8);
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_14_context_actions_target_the_same_track() {
        gtk4::init().unwrap();
        let opened = Rc::new(RefCell::new(None));
        let metadata: MetadataCallback = Rc::new(RefCell::new(Some({
            let opened = opened.clone();
            Rc::new(move |value| *opened.borrow_mut() = Some(value))
        })));
        let (card, snapshot) = card_and_snapshot(metadata);
        let next = Rc::new(RefCell::new(Vec::new()));
        let queued = Rc::new(RefCell::new(Vec::new()));
        card.set_on_play_next({
            let next = next.clone();
            move |id| next.borrow_mut().push(id)
        });
        card.set_on_add_to_queue({
            let queued = queued.clone();
            move |id| queued.borrow_mut().push(id)
        });
        card.set_data(&snapshot);

        let actions = &card.context_actions.borrow()[1];
        actions.lookup_action("play-next").unwrap().activate(None);
        actions
            .lookup_action("add-to-queue")
            .unwrap()
            .activate(None);
        actions.lookup_action("open-album").unwrap().activate(None);

        assert_eq!(*next.borrow(), vec![2]);
        assert_eq!(*queued.borrow(), vec![2]);
        assert!(matches!(
            opened.borrow().as_ref(),
            Some(StatsMetadataTarget::Album { track_id: 2, .. })
        ));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn discarded_song_rows_release_their_context_widgets() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let metadata: MetadataCallback = Rc::new(RefCell::new(None));
        let (card, snapshot) = card_and_snapshot(metadata);
        card.set_data(&snapshot);
        let old_row = card.rows.first_child().unwrap();
        let old_row = old_row.downcast::<gtk4::Box>().unwrap();
        let weak_row = old_row.downgrade();
        drop(old_row);

        card.set_data(&snapshot);
        let context = glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }

        assert!(
            weak_row.upgrade().is_none(),
            "a discarded row must not be retained by its input controllers"
        );
    }
}
