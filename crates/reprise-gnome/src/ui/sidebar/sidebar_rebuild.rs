//! Sidebar query projection and row-set rebuilding.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::artist_news;
use reprise_core::concerts;
use reprise_core::library::playlists;
use reprise_core::library::settings;
use reprise_core::modules::{self, CONCERTS_MODULE, NEW_RELEASES_MODULE, RADIO_MODULE};
use reprise_core::online_sources;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;
use reprise_core::{podcasts, radio};

use super::sidebar_dnd;
use super::sidebar_export;
use super::sidebar_issue_cleanup;
use super::sidebar_module_menu;
use super::sidebar_playlist_quick_add;
use super::sidebar_presentation::{self, NavIcon};
use super::strings;
use super::surface::remember_issue_focus_entry;
use super::{
    find_row, has_sidebar_row, resolve_select_source, select_row_in_its_listbox, RowEntry, Shared,
};

pub(in crate::ui) fn rebuild(shared: &Rc<Shared>, force_select: Option<ViewSource>, reason: &str) {
    let refresh_number = shared.refresh_count.get() + 1;
    shared.refresh_count.set(refresh_number);
    tracing::debug!(
        refresh_number,
        reason,
        "sidebar refresh #{refresh_number} ({reason})"
    );
    let today = chrono::Local::now().date_naive();

    let (
        music_count,
        missing_count,
        new_missing_count,
        pending_doctor_count,
        active_import_error_count,
        dismissed_import_error_count,
        new_import_error_count,
        playlist_rows,
        smart_rows,
        podcasts_enabled,
        podcasts_count,
        youtube_enabled,
        youtube_count,
        radio_enabled,
        radio_count,
        releases_enabled,
        concerts_enabled,
        concerts_count,
    ) = {
        let conn = &shared.conn;
        let music_count =
            queries::query_track_count(conn, &ViewSource::Library, "", &[]).unwrap_or(0);
        let missing_count = queries::count_missing(conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to count missing files for sidebar visibility");
            0
        });
        let last_viewed_missing = settings::get_last_viewed_missing(conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to read Missing-files last-viewed timestamp");
            0
        });
        let new_missing_count = queries::count_new_missing(conn, last_viewed_missing)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to count new missing files for sidebar badge");
                0
            });
        let pending_doctor_count =
            queries::count_pending_doctor_findings(conn).unwrap_or_else(|error| {
                tracing::error!(%error, "failed to count pending Library Doctor findings");
                0
            });
        let active_import_error_count = queries::count_import_errors_active(conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to count active import errors for sidebar visibility");
                0
            });
        let dismissed_import_error_count = queries::count_dismissed_import_errors(conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to count dismissed import errors for sidebar reachability");
                0
            });
        let last_viewed_import_errors = settings::get_last_viewed_import_errors(conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to read Import-errors last-viewed timestamp");
                0
            });
        let new_import_error_count =
            queries::count_new_import_errors(conn, last_viewed_import_errors).unwrap_or_else(
                |error| {
                    tracing::error!(%error, "failed to count new import errors for sidebar badge");
                    0
                },
            );
        let playlist_rows = playlists::list(conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to list playlists for sidebar");
            Vec::new()
        });
        // Each smart list carries its own live track count so the sidebar can
        // badge it like a manual playlist (SIDEBAR-badge parity). The count is
        // the rule-based `ViewSource::Smart` count — the same query the list
        // itself resolves to — so the badge always matches what opening the
        // list shows. Paired with its row here (rather than as a parallel vec)
        // so the count travels with the row it belongs to.
        let smart_rows: Vec<(playlists::SmartPlaylist, i64)> = playlists::list_smart(conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to list smart playlists for sidebar");
                Vec::new()
            })
            .into_iter()
            .map(|smart| {
                let source = if smart.role.as_deref() == Some(playlists::RECENTLY_ADDED_ROLE) {
                    ViewSource::RecentlyAdded
                } else {
                    ViewSource::Smart(smart.id)
                };
                let count =
                    queries::query_track_count(conn, &source, "", &[]).unwrap_or_else(|error| {
                        tracing::error!(
                            %error,
                            smart_id = smart.id,
                            "failed to count smart playlist tracks for sidebar badge"
                        );
                        0
                    });
                (smart, count)
            })
            .collect();
        // NET-1a / issue #96: YouTube is a peer of Podcasts (RSS), not a
        // sub-setting of it — each hides its own sidebar entry
        // independently, and both additionally require the global
        // online-sources gate, matching every other online-source row here.
        let podcasts_enabled =
            online_sources::network_allowed(conn, &modules::PODCASTS_MODULE).unwrap_or(false);
        let podcasts_count = if podcasts_enabled {
            podcasts::query::count_unplayed_for_kind(conn, podcasts::PodcastKind::Rss).map_or_else(
                |error| {
                    tracing::error!(%error, "failed to count unplayed podcast episodes");
                    0
                },
                |count| i64::try_from(count).unwrap_or(i64::MAX),
            )
        } else {
            0
        };
        let youtube_enabled =
            online_sources::network_allowed(conn, &modules::YOUTUBE_MODULE).unwrap_or(false);
        let youtube_count = if youtube_enabled {
            podcasts::query::count_unplayed_for_kind(conn, podcasts::PodcastKind::Youtube)
                .map_or_else(
                    |error| {
                        tracing::error!(%error, "failed to count unplayed YouTube episodes");
                        0
                    },
                    |count| i64::try_from(count).unwrap_or(i64::MAX),
                )
        } else {
            0
        };
        let radio_enabled = online_sources::network_allowed(conn, &RADIO_MODULE).unwrap_or(false);
        let radio_count = if radio_enabled {
            radio::station::count_stations(conn).map_or_else(
                |error| {
                    tracing::error!(%error, "failed to count favorite radio stations");
                    0
                },
                |count| i64::try_from(count).unwrap_or(i64::MAX),
            )
        } else {
            0
        };
        let releases_enabled = modules::is_enabled(conn, &NEW_RELEASES_MODULE).unwrap_or(false);
        let concerts_enabled = modules::is_enabled(conn, &CONCERTS_MODULE).unwrap_or(false);
        let concerts_count = if concerts_enabled {
            concerts::config::persisted_filter(conn)
                .and_then(|filter| {
                    let location = concerts::config::location(conn)?;
                    concerts::count_upcoming(conn, &filter, location.as_ref(), today)
                })
                .unwrap_or_else(|error| {
                    tracing::error!(%error, "failed to count Concerts rows for sidebar badge");
                    0
                })
        } else {
            0
        };
        (
            music_count,
            missing_count,
            new_missing_count,
            pending_doctor_count,
            active_import_error_count,
            dismissed_import_error_count,
            new_import_error_count,
            playlist_rows,
            smart_rows,
            podcasts_enabled,
            podcasts_count,
            youtube_enabled,
            youtube_count,
            radio_enabled,
            radio_count,
            releases_enabled,
            concerts_enabled,
            concerts_count,
        )
    };
    let queue_count = (shared.queue_len_provider)() as i64;
    let playlist_count = playlist_rows.len();

    shared.listbox.remove_all();
    shared.issues_listbox.remove_all();
    shared.rows.borrow_mut().clear();
    *shared.queue_count_label.borrow_mut() = None;
    *shared.releases_count_label.borrow_mut() = None;
    *shared.playlist_add_button.borrow_mut() = None;

    sidebar_presentation::append_header(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_LIBRARY),
    );
    add_row(
        shared,
        ViewSource::Library,
        &strings::text(strings::SIDEBAR_MUSIC),
        sidebar_presentation::nonzero_count(music_count),
        NavIcon::Library,
    );
    if podcasts_enabled {
        add_row(
            shared,
            ViewSource::Podcasts,
            &strings::text(strings::PODCASTS),
            sidebar_presentation::nonzero_count(podcasts_count),
            NavIcon::Podcasts,
        );
    }
    if youtube_enabled {
        add_row(
            shared,
            ViewSource::Youtube,
            &strings::text(strings::YOUTUBE),
            sidebar_presentation::nonzero_count(youtube_count),
            NavIcon::Youtube,
        );
    }
    if radio_enabled {
        add_row(
            shared,
            ViewSource::Radio,
            &strings::text(strings::RADIO),
            sidebar_presentation::nonzero_count(radio_count),
            NavIcon::Radio,
        );
    }
    add_row(
        shared,
        ViewSource::Queue,
        &strings::text(strings::SIDEBAR_QUEUE),
        sidebar_presentation::nonzero_count(queue_count),
        NavIcon::Queue,
    );

    let playlist_add_button = sidebar_presentation::append_header_with_action(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_PLAYLISTS),
        &strings::text(strings::SIDEBAR_NEW_PLAYLIST),
        {
            let shared = shared.clone();
            move || sidebar_playlist_quick_add::begin(&shared)
        },
    );
    *shared.playlist_add_button.borrow_mut() = Some(playlist_add_button);
    for playlist in &playlist_rows {
        add_row(
            shared,
            ViewSource::Playlist(playlist.id),
            &playlist.name,
            sidebar_presentation::nonzero_count(playlist.track_count),
            NavIcon::Playlist,
        );
    }
    sidebar_presentation::append_header(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_SMART),
    );
    for (smart, count) in &smart_rows {
        let source = if smart.role.as_deref() == Some(playlists::RECENTLY_ADDED_ROLE) {
            ViewSource::RecentlyAdded
        } else {
            ViewSource::Smart(smart.id)
        };
        add_row(
            shared,
            source,
            &smart.name,
            sidebar_presentation::nonzero_count(*count),
            sidebar_presentation::smart_icon(&smart.sort_field),
        );
    }
    if releases_enabled {
        let releases_count = request_releases_count(shared, today);
        add_row(
            shared,
            ViewSource::Releases,
            &strings::text(strings::RELEASES),
            releases_count.and_then(sidebar_presentation::nonzero_count),
            NavIcon::Releases,
        );
    }
    if concerts_enabled {
        add_row(
            shared,
            ViewSource::Concerts,
            &strings::text(strings::CONCERTS),
            sidebar_presentation::nonzero_count(concerts_count),
            NavIcon::Concerts,
        );
    }
    add_row(
        shared,
        ViewSource::MyStats,
        &strings::text(strings::SIDEBAR_MY_STATS),
        None,
        NavIcon::MyStats,
    );

    // Dismissed import errors stay reachable through the triage view's
    // collapsed footer, even though they never contribute to the badge.
    let has_import_errors = active_import_error_count > 0 || dismissed_import_error_count > 0;
    let has_issues =
        has_import_errors || missing_count > 0 || doctor_issue_visible(pending_doctor_count);
    shared.issues_listbox.set_visible(has_issues);
    if has_issues {
        if has_import_errors {
            add_issue_row(
                shared,
                ViewSource::ImportErrors,
                &strings::text(strings::SIDEBAR_IMPORT_ERRORS),
                new_import_error_count,
                NavIcon::ImportErrors,
            );
        }
        if missing_count > 0 {
            add_issue_row(
                shared,
                ViewSource::Missing,
                &strings::text(strings::SIDEBAR_MISSING_FILES),
                new_missing_count,
                NavIcon::Missing,
            );
        }
        if doctor_issue_visible(pending_doctor_count) {
            add_issue_action_row(
                shared,
                &strings::text(strings::LIBRARY_DOCTOR),
                pending_doctor_count,
                NavIcon::LibraryDoctor,
                "win.library-doctor-findings",
            );
        }
    }

    crate::ui::startup_report::event("sidebar_rebuild");
    tracing::debug!(
        playlists = playlist_count,
        missing = missing_count,
        new_missing = new_missing_count,
        active_import_errors = active_import_error_count,
        dismissed_import_errors = dismissed_import_error_count,
        new_import_errors = new_import_error_count,
        "sidebar built: {playlist_count} playlists, missing={missing_count}, active_import_errors={active_import_error_count}"
    );

    let requested_source = force_select.unwrap_or_else(|| shared.current_source.borrow().clone());
    if !has_sidebar_row(&requested_source) {
        // UX FIL-1c: album/artist/genre scopes are opened from inside the
        // track list and never get a row. Their absence from the row set is
        // the normal state, so falling back to Library here would route the
        // user out of the scope they are looking at — which is exactly what
        // queue mutations used to trigger this rebuild, dropping the scope
        // chip and re-showing the whole library. Leave the selection empty
        // instead.
        tracing::debug!(
            scope = %requested_source.label(),
            "scope view has no sidebar row; leaving the selection empty"
        );
        return;
    }
    let requested_row = find_row(shared, &requested_source);
    let (select_source, fell_back) =
        resolve_select_source(requested_source.clone(), requested_row.is_some());
    if fell_back {
        tracing::debug!(
            vanished_source = %requested_source.label(),
            "selected source vanished; falling back to Library"
        );
    }
    let row_to_select = if fell_back {
        find_row(shared, &select_source)
    } else {
        requested_row
    };
    if let Some(row) = row_to_select {
        select_row_in_its_listbox(&row);
    }
}

pub(in crate::ui) const fn doctor_issue_visible(pending_count: u32) -> bool {
    pending_count > 0
}

/// Starts the expensive Releases projection away from GTK's main thread.
/// In-memory databases cannot be reopened by a worker, so display tests use
/// the same query synchronously; production databases are always file-backed.
fn request_releases_count(shared: &Rc<Shared>, today: chrono::NaiveDate) -> Option<i64> {
    let generation = shared.releases_count_generation.get() + 1;
    shared.releases_count_generation.set(generation);

    let Some(database_path) = shared.conn.path() else {
        return artist_news::persisted_releases_filter(&shared.conn)
            .and_then(|filter| artist_news::count_releases_view(&shared.conn, &filter, today))
            .map_err(|error| {
                tracing::error!(%error, "failed to count Releases rows for sidebar badge");
                error
            })
            .ok();
    };

    let receiver = match crate::ui::one_shot_task::spawn("reprise-sidebar-releases", move || {
        let db =
            reprise_core::db::Db::open_ready(&database_path).map_err(|error| error.to_string())?;
        let filter =
            artist_news::persisted_releases_filter(&db).map_err(|error| error.to_string())?;
        artist_news::count_releases_view(&db, &filter, today).map_err(|error| error.to_string())
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            tracing::error!(%error, "failed to start Releases sidebar count worker");
            return None;
        }
    };
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        let result = receiver.recv().await;
        let Some(shared) = weak.upgrade() else {
            return;
        };
        if shared.releases_count_generation.get() != generation {
            return;
        }
        match result {
            Ok(Ok(count)) => {
                let label = shared.releases_count_label.borrow().clone();
                if let Some(label) = label {
                    sidebar_presentation::update_live_count_label(
                        &label,
                        sidebar_presentation::nonzero_count(count),
                    );
                }
            }
            Ok(Err(error)) => {
                tracing::error!(%error, "failed to count Releases rows for sidebar badge");
            }
            Err(error) => {
                tracing::error!(%error, "Releases sidebar count worker closed without a result");
            }
        }
    });
    None
}

fn add_row(
    shared: &Rc<Shared>,
    source: ViewSource,
    title: &str,
    count: Option<i64>,
    icon: NavIcon,
) {
    let editing_playlist_id = match &source {
        ViewSource::Playlist(playlist_id)
            if shared.playlist_quick_edit_id.get() == Some(*playlist_id) =>
        {
            Some(*playlist_id)
        }
        _ => None,
    };
    let (row, editor) = if editing_playlist_id.is_some() {
        let (row, editor) = sidebar_presentation::build_editable_playlist_row(title, count);
        (row, Some(editor))
    } else if matches!(source, ViewSource::Queue | ViewSource::Releases) {
        let (row, count_label) = sidebar_presentation::build_live_count_nav_row(title, count, icon);
        if matches!(source, ViewSource::Queue) {
            *shared.queue_count_label.borrow_mut() = Some(count_label);
        } else {
            *shared.releases_count_label.borrow_mut() = Some(count_label);
        }
        (row, None)
    } else {
        (
            sidebar_presentation::build_nav_row(title, count, icon),
            None,
        )
    };
    sidebar_module_menu::wire(shared, &row, &source, title);
    match &source {
        ViewSource::Playlist(playlist_id) => {
            sidebar_dnd::wire_playlist_drop_target(shared, &row, *playlist_id, title);
            sidebar_export::wire_playlist_context_menu(shared, &row, *playlist_id, title);
        }
        ViewSource::Queue => {
            sidebar_dnd::wire_queue_drop_target(shared, &row);
        }
        ViewSource::ImportErrors | ViewSource::Missing => {
            sidebar_issue_cleanup::wire_issue_context_menu(shared, &row, source.clone());
        }
        _ => {}
    }
    let target = if matches!(source, ViewSource::ImportErrors | ViewSource::Missing) {
        &shared.issues_listbox
    } else {
        &shared.listbox
    };
    target.append(&row);
    if let (Some(playlist_id), Some(editor)) = (editing_playlist_id, editor) {
        sidebar_playlist_quick_add::wire_editor(shared, &row, playlist_id, &editor);
    }
    let entry: RowEntry = (row, source, title.to_string());
    shared.rows.borrow_mut().push(entry);
}

fn add_issue_row(
    shared: &Rc<Shared>,
    source: ViewSource,
    title: &str,
    new_count: u32,
    icon: NavIcon,
) {
    let presentation = sidebar_presentation::issue_row_presentation(new_count, icon);
    let row = sidebar_presentation::build_issue_nav_row(title, presentation, icon);
    sidebar_issue_cleanup::wire_issue_context_menu(shared, &row, source.clone());
    shared.issues_listbox.append(&row);
    remember_issue_focus_entry(&shared.issues_listbox, &row);
    shared
        .rows
        .borrow_mut()
        .push((row, source, title.to_string()));
}

pub(super) fn add_issue_action_row(
    shared: &Rc<Shared>,
    title: &str,
    count: u32,
    icon: NavIcon,
    action: &'static str,
) {
    let presentation = sidebar_presentation::issue_row_presentation(count, icon);
    let row = sidebar_presentation::build_issue_nav_row(title, presentation, icon);
    row.set_selectable(false);
    // a11y-semantics: role=list-item name=library-doctor state=focusable action=activate
    row.set_focusable(true);
    // input-parity: ACC-8 keyboard=issue-action-row-enter
    row.connect_activate(move |row| activate_issue_action(row, action));
    // Pointer and assistive-technology activation arrive through the real
    // button built by `build_issue_nav_row`; it activates this same row signal
    // and therefore shares the keyboard path above.
    shared.issues_listbox.append(&row);
    remember_issue_focus_entry(&shared.issues_listbox, &row);
}

fn activate_issue_action(row: &gtk4::ListBoxRow, action: &'static str) {
    if let Err(error) = row.activate_action(action, None) {
        tracing::error!(%error, action, "failed to activate sidebar issue action");
    }
}
