//! Six-track songs card and its expandable sortable full ranking.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::cover::ThumbnailSize;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen::TopTrack;
use reprise_core::library::stats_snapshot::{SortBy, StatsSnapshot};

use super::stats_metadata_links::{self, MetadataCallback, StatsMetadataTarget};
use super::stats_view_widgets::{card, clear, label};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

const SONG_ROW_LIMIT: usize = 6;
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
struct SummaryRenderer {
    rows: gtk4::Box,
    play_buttons: Rc<RefCell<Vec<gtk4::Button>>>,
    bars: Rc<RefCell<Vec<gtk4::LevelBar>>>,
    row_clicks: Rc<RefCell<Vec<gtk4::GestureClick>>>,
    cover_loader: Rc<CoverLoader>,
    generations: CoverGenerations,
    metadata: MetadataCallback,
    on_play_track: IdCallback,
    on_play_next: IdCallback,
    on_add_to_queue: IdCallback,
    context_actions: Rc<RefCell<Vec<gio::SimpleActionGroup>>>,
}

#[derive(Clone)]
pub(in crate::ui) struct StatsSongsCard {
    root: gtk4::Box,
    summary: SummaryRenderer,
    #[cfg_attr(not(test), allow(dead_code))]
    revealer: gtk4::Revealer,
    #[cfg_attr(not(test), allow(dead_code))]
    reveal_button: gtk4::Button,
    full_rows: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    sort_toggle: adw::ToggleGroup,
    snapshot: Rc<RefCell<Option<StatsSnapshot>>>,
    sort_by: Rc<Cell<SortBy>>,
}

impl StatsSongsCard {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>, metadata: MetadataCallback) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("stats-songs-card");
        let kicker = label("MOST PLAYED SONGS", "stats-eyebrow");
        let plays_sort = adw::Toggle::builder()
            .name("plays")
            .label("by plays")
            .build();
        let time_sort = adw::Toggle::builder().name("time").label("by time").build();
        let sort_toggle = adw::ToggleGroup::new();
        sort_toggle.add(plays_sort);
        sort_toggle.add(time_sort);
        sort_toggle.set_active_name(Some("plays"));
        sort_toggle.set_halign(gtk4::Align::End);
        // a11y-semantics: role=group name=sort-top-tracks state=one-selected action=arrow-keys
        sort_toggle.update_property(&[gtk4::accessible::Property::Label("Sort top tracks")]);
        // input-parity: ACC-8 keyboard=sort-toggle-arrows
        // AdwToggleGroup owns the focusable pill buttons and native arrow-key
        // selection, with one selected item exposed through its group label.
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        kicker.set_hexpand(true);
        header.append(&kicker);
        header.append(&sort_toggle);
        root.append(&header);

        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.append(&rows);

        let reveal_button = gtk4::Button::with_label("Show all top tracks");
        reveal_button.add_css_class("flat");
        reveal_button.add_css_class("stats-songs-reveal");
        reveal_button.set_halign(gtk4::Align::Start);
        root.append(&reveal_button);

        let full_rows = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let expanded_content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        expanded_content.append(&full_rows);
        let expanded = card(&expanded_content);
        expanded.set_hexpand(true);
        let revealer = gtk4::Revealer::new();
        revealer.set_reveal_child(false);
        revealer.set_child(Some(&expanded));
        revealer.set_visible(false);
        revealer.connect_child_revealed_notify(|revealer| {
            if !revealer.is_child_revealed() && !revealer.reveals_child() {
                revealer.set_visible(false);
            }
        });

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
        let summary = SummaryRenderer {
            rows,
            play_buttons,
            bars: summary_bars,
            row_clicks,
            cover_loader,
            generations: cover_generations,
            metadata,
            on_play_track,
            on_play_next,
            on_add_to_queue,
            context_actions,
        };

        reveal_button.connect_clicked(glib::clone!(
            #[weak]
            revealer,
            move |button| {
                let reveal = !revealer.reveals_child();
                if reveal {
                    // A hidden revealer cannot animate. Join the section flow
                    // before starting the transition; collapse removes it only
                    // from `child-revealed` after the animation finishes.
                    revealer.set_visible(true);
                    revealer.set_reveal_child(true);
                } else {
                    revealer.set_reveal_child(false);
                }
                button.set_label(if reveal {
                    "Hide top tracks"
                } else {
                    "Show all top tracks"
                });
            }
        ));

        sort_toggle.connect_active_name_notify({
            let full_rows = full_rows.clone();
            let snapshot = snapshot.clone();
            let sort_by = sort_by.clone();
            let summary = summary.clone();
            move |toggle| {
                let active_name = toggle.active_name();
                let value = sort_for_toggle_name(active_name.as_deref());
                sort_by.set(value);
                let snapshot = snapshot.borrow().clone();
                if let Some(snapshot) = snapshot {
                    summary.render(&snapshot, value);
                    render_full_rows(
                        &full_rows,
                        &snapshot,
                        value,
                        &summary.cover_loader,
                        &summary.generations,
                        &summary.metadata,
                    );
                }
            }
        });

        Self {
            root,
            summary,
            revealer,
            reveal_button,
            full_rows,
            sort_toggle,
            snapshot,
            sort_by,
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
        self.summary.render(snapshot, self.sort_by.get());
        render_full_rows(
            &self.full_rows,
            snapshot,
            self.sort_by.get(),
            &self.summary.cover_loader,
            &self.summary.generations,
            &self.summary.metadata,
        );
    }
}

fn sort_for_toggle_name(name: Option<&str>) -> SortBy {
    if name == Some("time") {
        SortBy::Time
    } else {
        SortBy::Plays
    }
}

impl SummaryRenderer {
    fn render(&self, snapshot: &StatsSnapshot, sort_by: SortBy) {
        clear(&self.rows);
        self.context_actions.borrow_mut().clear();
        self.play_buttons.borrow_mut().clear();
        self.bars.borrow_mut().clear();
        self.row_clicks.borrow_mut().clear();
        let tracks = snapshot.top_tracks_sorted(sort_by);
        let leader = tracks.first().map_or(0, |track| metric(track, sort_by));
        let token = self.generations.next_summary();
        for track in tracks.iter().take(SONG_ROW_LIMIT) {
            let row = self.song_row(track, leader, sort_by, token);
            self.rows.append(&row);
        }
    }

    fn song_row(&self, track: &TopTrack, leader: i64, sort_by: SortBy, token: u64) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.add_css_class("stats-song-row");
        row.set_hexpand(true);
        // a11y-semantics: role=group name=track-row state=focusable action=enter/shift-f10
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
            &self.generations.summary,
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
        bar.set_value(relative_value(metric(track, sort_by), leader));
        bar.set_width_request(110);
        bar.set_height_request(8);
        bar.set_valign(gtk4::Align::Center);
        self.bars.borrow_mut().push(bar.clone());
        body.append(&bar);
        let plays = label(
            &format!("{} plays", format_thousands(track.play_count)),
            "stats-play-count",
        );
        plays.set_xalign(1.0);
        body.append(&plays);
        row.append(&body);
        // input-parity: ACC-8 keyboard=enter-row
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
        // input-parity: ACC-8 keyboard=menu-shift-f10
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
}

impl StatsSongsCard {
    pub(in crate::ui) fn set_on_play_track(&self, callback: impl Fn(i64) + 'static) {
        *self.summary.on_play_track.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_play_next(&self, callback: impl Fn(i64) + 'static) {
        *self.summary.on_play_next.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_add_to_queue(&self, callback: impl Fn(i64) + 'static) {
        *self.summary.on_add_to_queue.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn summary_bars(&self) -> Vec<gtk4::LevelBar> {
        self.summary.bars.borrow().clone()
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
            .valign(gtk4::Align::Center)
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
        text.append(&stats_metadata_links::link(
            &track.title,
            "stats-item-title",
            StatsMetadataTarget::Track(track.track_id),
            metadata,
        ));
        text.append(&stats_metadata_links::link(
            &track.artist,
            "stats-item-subtitle",
            StatsMetadataTarget::Artist {
                track_id: track.track_id,
                artist: track.effective_artist.clone(),
            },
            metadata,
        ));
        row.append(&text);
        let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        bar.add_css_class("stats-compact-song-bar");
        bar.set_size_request(120, 8);
        bar.set_valign(gtk4::Align::Center);
        bar.set_accessible_role(gtk4::AccessibleRole::Presentation);
        let fill = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        fill.add_css_class("stats-compact-song-bar-fill");
        fill.set_size_request(
            (120.0 * relative_value(metric(track, sort_by), leader)).round() as i32,
            8,
        );
        bar.append(&fill);
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
#[path = "stats_songs_card_tests.rs"]
mod tests;
