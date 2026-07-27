//! Pure decisions for the shared track-row context menu.
//!
//! Keeping selection summarization and action sensitivity independent of GTK
//! widgets makes every context testable without a display. Runtime
//! [`ViewSource`] values collapse onto the five menu contexts: Missing,
//! Smart, My Stats, Import Errors, and Device defensively use
//! [`MenuContext::LibraryTracks`]. Device destructive semantics remain a
//! separate product decision; this module preserves today's library-like
//! treatment until that decision is made.

use reprise_core::library::playlists::PlaylistSummary;
use reprise_core::models::Track;
use reprise_core::view_source::ViewSource;

use gtk4::gio;
use gtk4::gio::prelude::*;

use crate::ui::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum MenuContext {
    LibraryTracks,
    AlbumDetail,
    ArtistDetail,
    Playlist,
    Queue,
}

impl MenuContext {
    pub(in crate::ui) fn from_source(source: &ViewSource) -> Self {
        match source {
            ViewSource::Album { .. } => Self::AlbumDetail,
            ViewSource::Artist(_) => Self::ArtistDetail,
            ViewSource::Playlist(_) => Self::Playlist,
            ViewSource::Queue => Self::Queue,
            ViewSource::Library
            | ViewSource::RecentlyAdded
            | ViewSource::Smart(_)
            | ViewSource::Missing
            | ViewSource::ImportErrors
            | ViewSource::MyStats
            | ViewSource::Releases
            | ViewSource::Concerts
            | ViewSource::Podcasts
            | ViewSource::Radio
            | ViewSource::Conversions
            | ViewSource::Genre(_)
            | ViewSource::Device { .. } => Self::LibraryTracks,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct SelectionSummary {
    pub count: usize,
    pub any_missing: bool,
    pub all_missing: bool,
    pub same_album: bool,
    pub same_artist: bool,
    pub same_folder: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct PlaylistEntry {
    pub id: i64,
    pub name: String,
    pub is_current: bool,
}

/// Maps the raw `playlists::list` rows onto the menu's [`PlaylistEntry`]
/// view: sorted case-insensitively by name, with the row matching the open
/// `ViewSource::Playlist` marked current (rendered grey and unactionable —
/// a playlist can't add to itself). Pure so the adapter only has to fetch
/// the rows; the order/current decisions are testable without a display.
pub(in crate::ui) fn playlist_entries(
    playlists: &[PlaylistSummary],
    source: &ViewSource,
) -> Vec<PlaylistEntry> {
    let mut entries: Vec<PlaylistEntry> = playlists
        .iter()
        .map(|playlist| PlaylistEntry {
            id: playlist.id,
            is_current: matches!(source, ViewSource::Playlist(id) if *id == playlist.id),
            name: playlist.name.clone(),
        })
        .collect();
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct ActionStates {
    pub enqueue: bool,
    pub go_to_album: bool,
    pub go_to_artist: bool,
    pub show_in_files: bool,
    pub trash: bool,
    pub edit_tags: bool,
}

pub(in crate::ui) struct MenuInputs<'a> {
    pub context: MenuContext,
    pub selection: &'a SelectionSummary,
    pub playlists: &'a [PlaylistEntry],
    pub is_missing_view: bool,
}

pub(in crate::ui) fn summarize_selection(tracks: &[Track]) -> SelectionSummary {
    let Some(first) = tracks.first() else {
        return SelectionSummary {
            count: 0,
            any_missing: false,
            all_missing: false,
            same_album: false,
            same_artist: false,
            same_folder: false,
        };
    };

    let normalize = |value: &str| value.trim().to_lowercase();
    let first_album = (normalize(&first.album), normalize(&first.album_artist));
    let first_artist = normalize(&first.album_artist);
    let first_folder = std::path::Path::new(&first.path).parent();

    SelectionSummary {
        count: tracks.len(),
        any_missing: tracks.iter().any(Track::is_missing),
        all_missing: tracks.iter().all(Track::is_missing),
        same_album: tracks
            .iter()
            .all(|track| (normalize(&track.album), normalize(&track.album_artist)) == first_album),
        same_artist: tracks
            .iter()
            .all(|track| normalize(&track.album_artist) == first_artist),
        same_folder: tracks
            .iter()
            .all(|track| std::path::Path::new(&track.path).parent() == first_folder),
    }
}

pub(in crate::ui) fn action_states(
    _context: MenuContext,
    selection: &SelectionSummary,
) -> ActionStates {
    ActionStates {
        enqueue: selection.count > 0 && !selection.any_missing,
        go_to_album: selection.count > 0 && selection.same_album,
        go_to_artist: selection.count > 0 && selection.same_artist,
        show_in_files: selection.count > 0 && !selection.any_missing && selection.same_folder,
        trash: selection.count > 0 && !selection.any_missing,
        edit_tags: selection.count > 0 && !selection.all_missing,
    }
}

pub(in crate::ui) fn build_track_menu(inputs: &MenuInputs<'_>) -> gio::Menu {
    const GROUP: &str = "tracklist";
    let menu = gio::Menu::new();

    let primary = gio::Menu::new();
    match inputs.context {
        MenuContext::Queue => {
            append_action(&primary, strings::CONTEXT_MENU_MOVE_TO_TOP, "move-to-top");
        }
        _ => {
            append_action(&primary, strings::CONTEXT_MENU_PLAY_NEXT, "play-next");
            append_action(&primary, strings::CONTEXT_MENU_ADD_TO_QUEUE, "add-to-queue");
        }
    }
    menu.append_section(None, &primary);

    let selection_actions = gio::Menu::new();
    let playlist_submenu = gio::Menu::new();
    for playlist in inputs.playlists {
        let item = gio::MenuItem::new(Some(&playlist.name), None);
        if !playlist.is_current {
            item.set_action_and_target_value(
                Some(&format!("{GROUP}.add-to-playlist")),
                Some(&playlist.id.to_variant()),
            );
        }
        playlist_submenu.append_item(&item);
    }
    playlist_submenu.append(
        Some(&strings::text(strings::CONTEXT_MENU_NEW_PLAYLIST)),
        Some(&format!("{GROUP}.new-playlist")),
    );
    selection_actions.append_submenu(
        Some(&strings::text(strings::CONTEXT_MENU_ADD_TO_PLAYLIST)),
        &playlist_submenu,
    );
    append_action(&selection_actions, strings::EDIT_TAGS, "edit-tags");
    menu.append_section(None, &selection_actions);

    let navigation = gio::Menu::new();
    if inputs.context != MenuContext::AlbumDetail {
        append_action(
            &navigation,
            strings::CONTEXT_MENU_GO_TO_ALBUM,
            "go-to-album",
        );
    }
    if inputs.context != MenuContext::ArtistDetail {
        append_action(
            &navigation,
            strings::CONTEXT_MENU_GO_TO_ARTIST,
            "go-to-artist",
        );
    }
    menu.append_section(None, &navigation);

    let files = gio::Menu::new();
    append_action(&files, strings::CONTEXT_MENU_SHOW_IN_FILES, "show-in-files");
    if inputs.selection.any_missing && !inputs.is_missing_view {
        append_action(
            &files,
            strings::CONTEXT_MENU_SHOW_IN_MISSING,
            "show-in-missing-files",
        );
    }
    menu.append_section(None, &files);

    let destructive = gio::Menu::new();
    match inputs.context {
        MenuContext::Playlist => destructive.append(
            Some(&strings::remove_from_playlist_label(inputs.selection.count)),
            Some(&format!("{GROUP}.remove-from-playlist")),
        ),
        MenuContext::Queue => destructive.append(
            Some(&strings::remove_from_queue_label(inputs.selection.count)),
            Some(&format!("{GROUP}.remove-from-queue")),
        ),
        MenuContext::LibraryTracks | MenuContext::AlbumDetail | MenuContext::ArtistDetail => {
            destructive.append(
                Some(&strings::remove_from_library_label(inputs.selection.count)),
                Some(&format!("{GROUP}.remove-selected-from-library")),
            );
            destructive.append(
                Some(&strings::move_to_trash_label(inputs.selection.count)),
                Some(&format!("{GROUP}.trash-selected-tracks")),
            );
        }
    }
    menu.append_section(None, &destructive);

    menu
}

fn append_action(menu: &gio::Menu, label: &str, action: &str) {
    menu.append(
        Some(&strings::text(label)),
        Some(&format!("tracklist.{action}")),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::glib;

    fn track(id: i64, album: &str, album_artist: &str, path: &str, missing: bool) -> Track {
        Track {
            id,
            path: path.into(),
            title: format!("Track {id}"),
            artist: album_artist.into(),
            album: album.into(),
            album_artist: album_artist.into(),
            year: None,
            track_no: None,
            genre: String::new(),
            duration_ms: 0,
            bitrate_kbps: None,
            rating: 0,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
            file_mtime: 0,
            missing_since: missing.then_some(1),
            missing_reason: None,
            untagged: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
            is_ai: false,
        }
    }

    fn same_selection() -> SelectionSummary {
        SelectionSummary {
            count: 3,
            any_missing: false,
            all_missing: false,
            same_album: true,
            same_artist: true,
            same_folder: true,
        }
    }

    fn selection(count: usize) -> SelectionSummary {
        SelectionSummary {
            count,
            any_missing: false,
            all_missing: false,
            same_album: true,
            same_artist: true,
            same_folder: true,
        }
    }

    fn menu_item_label(model: &gio::MenuModel, item: i32) -> Option<String> {
        model
            .item_attribute_value(item, "label", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>())
    }

    fn menu_item_action(model: &gio::MenuModel, item: i32) -> Option<String> {
        model
            .item_attribute_value(item, "action", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>())
    }

    fn collect_labels(model: &gio::MenuModel, labels: &mut Vec<String>) {
        for item in 0..model.n_items() {
            if let Some(label) = menu_item_label(model, item) {
                labels.push(label);
            }
            for link in ["section", "submenu"] {
                if let Some(child) = model.item_link(item, link) {
                    collect_labels(&child, labels);
                }
            }
        }
    }

    fn menu_labels(menu: &gio::Menu) -> Vec<String> {
        let mut labels = Vec::new();
        collect_labels(menu.upcast_ref(), &mut labels);
        labels
    }

    fn add_to_playlist_submenu(menu: &gio::Menu) -> gio::MenuModel {
        fn find(model: &gio::MenuModel) -> Option<gio::MenuModel> {
            for item in 0..model.n_items() {
                if menu_item_label(model, item).as_deref() == Some("Add to playlist") {
                    return model.item_link(item, "submenu");
                }
                if let Some(section) = model.item_link(item, "section") {
                    if let Some(found) = find(&section) {
                        return Some(found);
                    }
                }
            }
            None
        }
        find(menu.upcast_ref()).expect("Add to playlist submenu")
    }

    #[test]
    fn ctx_4_nav_disabled_on_mixed_selection() {
        let same = same_selection();
        let mixed = SelectionSummary {
            same_album: false,
            same_artist: false,
            same_folder: false,
            ..same
        };
        let a = action_states(MenuContext::LibraryTracks, &same);
        assert!(a.go_to_album && a.go_to_artist);
        let b = action_states(MenuContext::LibraryTracks, &mixed);
        assert!(
            !b.go_to_album && !b.go_to_artist,
            "mixed album/artist selection greys nav out, not hidden"
        );
        let empty = SelectionSummary { count: 0, ..same };
        let c = action_states(MenuContext::LibraryTracks, &empty);
        assert!(!c.go_to_album && !c.enqueue && !c.edit_tags);

        // CTX-4 clause (a), model-level: the open context omits its own nav
        // entry (grey-out is sensitivity, above; this is presence). A detail
        // view drops the destination it already shows; the flat views keep
        // both.
        let playlists = [];
        let nav_labels = |context| {
            menu_labels(&build_track_menu(&MenuInputs {
                context,
                selection: &same,
                playlists: &playlists,
                is_missing_view: false,
            }))
        };
        let album = nav_labels(MenuContext::AlbumDetail);
        assert!(album.iter().any(|label| label == "Go to artist"));
        assert!(!album.iter().any(|label| label == "Go to album"));
        let artist = nav_labels(MenuContext::ArtistDetail);
        assert!(artist.iter().any(|label| label == "Go to album"));
        assert!(!artist.iter().any(|label| label == "Go to artist"));
        for context in [
            MenuContext::LibraryTracks,
            MenuContext::Playlist,
            MenuContext::Queue,
        ] {
            let labels = nav_labels(context);
            assert!(
                labels.iter().any(|label| label == "Go to album"),
                "{context:?} keeps Go to album"
            );
            assert!(
                labels.iter().any(|label| label == "Go to artist"),
                "{context:?} keeps Go to artist"
            );
        }
    }

    #[test]
    fn from_source_maps_every_view_source() {
        use reprise_core::view_source::ViewSource;

        let cases = [
            (
                ViewSource::Album {
                    album: "Blue".into(),
                    album_artist: "Joni".into(),
                },
                MenuContext::AlbumDetail,
            ),
            (ViewSource::Artist("Joni".into()), MenuContext::ArtistDetail),
            (ViewSource::Playlist(7), MenuContext::Playlist),
            (ViewSource::Queue, MenuContext::Queue),
            (ViewSource::Library, MenuContext::LibraryTracks),
            (ViewSource::Smart(3), MenuContext::LibraryTracks),
            (ViewSource::Missing, MenuContext::LibraryTracks),
            (ViewSource::ImportErrors, MenuContext::LibraryTracks),
            (ViewSource::MyStats, MenuContext::LibraryTracks),
            (
                ViewSource::Device {
                    serial: "pixel-8".into(),
                },
                MenuContext::LibraryTracks,
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(MenuContext::from_source(&source), expected, "{source:?}");
        }
    }

    #[test]
    fn ctx_10_show_in_files_same_folder_only() {
        let base = SelectionSummary {
            count: 2,
            any_missing: false,
            all_missing: false,
            same_album: false,
            same_artist: false,
            same_folder: true,
        };
        assert!(action_states(MenuContext::LibraryTracks, &base).show_in_files);
        let diff_folder = SelectionSummary {
            same_folder: false,
            ..base
        };
        assert!(!action_states(MenuContext::LibraryTracks, &diff_folder).show_in_files);
        let missing = SelectionSummary {
            any_missing: true,
            ..base
        };
        assert!(
            !action_states(MenuContext::LibraryTracks, &missing).show_in_files,
            "missing file cannot be revealed"
        );
    }

    #[test]
    fn summarize_marks_same_album_folder_and_missing() {
        let tracks = [
            track(1, "Blue", "Joni", "/m/blue/01.flac", false),
            track(2, "Blue", "joni ", "/m/blue/02.flac", false),
        ];
        let summary = summarize_selection(&tracks);
        assert!(
            summary.same_album
                && summary.same_artist
                && summary.same_folder
                && !summary.any_missing
                && summary.count == 2
        );
        let mixed = [
            tracks[0].clone(),
            track(3, "Red", "Neil", "/m/red/01.flac", true),
        ];
        let summary = summarize_selection(&mixed);
        assert!(
            !summary.same_album
                && !summary.same_folder
                && summary.any_missing
                && !summary.all_missing
        );
    }

    #[test]
    fn ctx_1_builder_per_context() {
        let selection = selection(1);
        let playlists = [PlaylistEntry {
            id: 1,
            name: "Alpha".into(),
            is_current: false,
        }];
        for context in [
            MenuContext::LibraryTracks,
            MenuContext::AlbumDetail,
            MenuContext::ArtistDetail,
            MenuContext::Playlist,
            MenuContext::Queue,
        ] {
            let menu = build_track_menu(&MenuInputs {
                context,
                selection: &selection,
                playlists: &playlists,
                is_missing_view: false,
            });
            assert!(menu.n_items() >= 4, "{context:?} has all sections");
            let labels = menu_labels(&menu);
            assert!(labels.iter().any(|label| label == "Edit tags…"));
            assert!(labels.iter().any(|label| label == "Add to playlist"));
        }
    }

    #[test]
    fn ctx_3_no_play_entry() {
        let selection = selection(2);
        let playlists = [];
        for context in [
            MenuContext::LibraryTracks,
            MenuContext::Playlist,
            MenuContext::Queue,
        ] {
            let labels = menu_labels(&build_track_menu(&MenuInputs {
                context,
                selection: &selection,
                playlists: &playlists,
                is_missing_view: false,
            }));
            assert!(
                !labels.iter().any(|label| label == "Play"),
                "{context:?} must not offer Play"
            );
        }
        let queue = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::Queue,
            selection: &selection,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert_eq!(queue.first().map(String::as_str), Some("Move to top"));
        // Queue trades the transport pair for Move to top; it offers neither.
        assert!(!queue.iter().any(|label| label == "Play next"));
        assert!(!queue.iter().any(|label| label == "Add to queue"));
        let library = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::LibraryTracks,
            selection: &selection,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert_eq!(library.first().map(String::as_str), Some("Play next"));
        // Move to top is Queue-only — the flat views never carry it.
        assert!(!library.iter().any(|label| label == "Move to top"));
        let playlist = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::Playlist,
            selection: &selection,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert!(!playlist.iter().any(|label| label == "Move to top"));
    }

    #[test]
    fn ctx_2_no_global_entries() {
        let selection = selection(1);
        let playlists = [];
        for context in [
            MenuContext::LibraryTracks,
            MenuContext::AlbumDetail,
            MenuContext::ArtistDetail,
            MenuContext::Playlist,
            MenuContext::Queue,
        ] {
            let labels = menu_labels(&build_track_menu(&MenuInputs {
                context,
                selection: &selection,
                playlists: &playlists,
                is_missing_view: false,
            }));
            assert!(
                !labels.iter().any(|label| label == "Rescan library"),
                "{context:?} must carry no global entry"
            );
        }
    }

    #[test]
    fn ctx_5a_playlist_and_queue_have_no_library_remove() {
        let selection = selection(1);
        let playlists = [];
        let labels = |context| {
            menu_labels(&build_track_menu(&MenuInputs {
                context,
                selection: &selection,
                playlists: &playlists,
                is_missing_view: false,
            }))
        };
        let library = labels(MenuContext::LibraryTracks);
        assert!(library
            .iter()
            .any(|label| label.starts_with("Remove from library")));
        assert!(library.iter().any(|label| label.starts_with("Move")));
        let playlist = labels(MenuContext::Playlist);
        assert!(!playlist
            .iter()
            .any(|label| label.starts_with("Remove from library")));
        assert!(!playlist.iter().any(|label| label.contains("Trash")));
        assert!(playlist
            .iter()
            .any(|label| label.starts_with("Remove from playlist")));
        let queue = labels(MenuContext::Queue);
        assert!(!queue
            .iter()
            .any(|label| label.starts_with("Remove from library")));
        assert!(!queue.iter().any(|label| label.contains("Trash")));
        assert!(queue
            .iter()
            .any(|label| label.starts_with("Remove from queue")));
    }

    #[test]
    fn ctx_6_count_currency_only_destructive() {
        let selection = selection(3);
        let playlists = [];
        let library = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::LibraryTracks,
            selection: &selection,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert!(library
            .iter()
            .any(|label| label == "Remove 3 from library…"));
        assert!(library.iter().any(|label| label == "Move 3 to Trash…"));
        assert!(
            library.iter().any(|label| label == "Edit tags…"),
            "non-destructive stays unnumbered"
        );
        assert!(library.iter().any(|label| label == "Add to queue"));
        let playlist = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::Playlist,
            selection: &selection,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert!(playlist
            .iter()
            .any(|label| label == "Remove 3 from playlist"));

        // A single selection drops the digit entirely — no "Move 1 to
        // Trash…"; the destructive labels revert to their bare singular.
        let one = self::selection(1);
        let library_one = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::LibraryTracks,
            selection: &one,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert!(library_one
            .iter()
            .any(|label| label == "Remove from library…"));
        assert!(library_one.iter().any(|label| label == "Move to Trash…"));
        let playlist_one = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::Playlist,
            selection: &one,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert!(playlist_one
            .iter()
            .any(|label| label == "Remove from playlist"));
        let queue_one = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::Queue,
            selection: &one,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert!(queue_one.iter().any(|label| label == "Remove from queue"));
    }

    #[test]
    fn ctx_8_missing_rows_disable_actions_and_add_show_in_missing() {
        let playlists = [];
        let missing = SelectionSummary {
            count: 2,
            any_missing: true,
            all_missing: false,
            same_album: false,
            same_artist: false,
            same_folder: false,
        };
        let labels = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::Playlist,
            selection: &missing,
            playlists: &playlists,
            is_missing_view: false,
        }));
        assert!(labels.iter().any(|label| label == "Show in Missing files"));
        let in_missing = menu_labels(&build_track_menu(&MenuInputs {
            context: MenuContext::LibraryTracks,
            selection: &missing,
            playlists: &playlists,
            is_missing_view: true,
        }));
        assert!(!in_missing
            .iter()
            .any(|label| label == "Show in Missing files"));
        let states = action_states(MenuContext::Playlist, &missing);
        assert!(!states.enqueue && !states.show_in_files && !states.trash && states.edit_tags);
        let all_missing = SelectionSummary {
            all_missing: true,
            ..missing
        };
        assert!(!action_states(MenuContext::LibraryTracks, &all_missing).edit_tags);
    }

    #[test]
    fn ctx_9_add_to_playlist_alphabetical_current_grayed() {
        let selection = selection(1);
        let mut playlists = vec![
            PlaylistEntry {
                id: 2,
                name: "Beta".into(),
                is_current: false,
            },
            PlaylistEntry {
                id: 1,
                name: "Alpha".into(),
                is_current: false,
            },
            PlaylistEntry {
                id: 3,
                name: "Zulu".into(),
                is_current: true,
            },
        ];
        playlists.sort_by_key(|entry| entry.name.to_lowercase());
        let menu = build_track_menu(&MenuInputs {
            context: MenuContext::Playlist,
            selection: &selection,
            playlists: &playlists,
            is_missing_view: false,
        });
        let submenu = add_to_playlist_submenu(&menu);
        let names: Vec<_> = (0..submenu.n_items())
            .filter_map(|item| menu_item_label(&submenu, item))
            .collect();
        assert_eq!(names, ["Alpha", "Beta", "Zulu", "New playlist…"]);
        assert!(
            menu_item_action(&submenu, 2).is_none(),
            "current playlist item is not actionable"
        );
        assert!(menu_item_action(&submenu, 0).is_some());
    }

    #[test]
    fn playlist_entries_sort_case_insensitively_and_mark_current() {
        let summary = |id: i64, name: &str| PlaylistSummary {
            id,
            name: name.into(),
            track_count: 0,
        };
        // Out of order and mixed-case on purpose, so the assertion pins the
        // case-insensitive sort key rather than the input order.
        let rows = [summary(3, "Zulu"), summary(1, "alpha"), summary(2, "Beta")];

        let in_playlist = playlist_entries(&rows, &ViewSource::Playlist(2));
        let names: Vec<_> = in_playlist.iter().map(|entry| entry.name.clone()).collect();
        assert_eq!(names, ["alpha", "Beta", "Zulu"]);
        assert_eq!(
            in_playlist
                .iter()
                .filter(|entry| entry.is_current)
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            [2],
            "only the open playlist's row is current"
        );

        // Any non-playlist source leaves every row actionable.
        let in_library = playlist_entries(&rows, &ViewSource::Library);
        assert!(in_library.iter().all(|entry| !entry.is_current));
    }
}
