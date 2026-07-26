//! Sidebar query projection and row-set rebuilding.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::artist_news;
use reprise_core::concerts;
use reprise_core::library::playlists;
use reprise_core::library::settings;
use reprise_core::modules::{
    self, CONCERTS_MODULE, NEW_RELEASES_MODULE, PODCASTS_MODULE, RADIO_MODULE,
};
use reprise_core::queries;
use reprise_core::view_source::ViewSource;
use reprise_core::{podcasts, radio};

use super::sidebar_dnd;
use super::sidebar_export;
use super::sidebar_issue_cleanup;
use super::sidebar_presentation::{self, NavIcon};
use super::strings;
use super::surface::remember_issue_focus_entry;
use super::{find_row, resolve_select_source, select_row_in_its_listbox, RowEntry, Shared};

pub(in crate::ui) fn rebuild(shared: &Rc<Shared>, force_select: Option<ViewSource>, reason: &str) {
    let refresh_number = shared.refresh_count.get() + 1;
    shared.refresh_count.set(refresh_number);
    tracing::debug!(
        refresh_number,
        reason,
        "sidebar refresh #{refresh_number} ({reason})"
    );

    let (
        music_count,
        missing_count,
        new_missing_count,
        active_import_error_count,
        dismissed_import_error_count,
        new_import_error_count,
        playlist_rows,
        smart_rows,
        podcasts_enabled,
        podcasts_count,
        radio_enabled,
        radio_count,
        releases_enabled,
        releases_count,
        concerts_enabled,
        concerts_count,
    ) = {
        let conn = shared.conn.borrow();
        let today = chrono::Local::now().date_naive();
        let music_count =
            queries::query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap_or(0);
        let missing_count = queries::count_missing(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to count missing files for sidebar visibility");
            0
        });
        let last_viewed_missing =
            settings::get_last_viewed_missing(&conn).unwrap_or_else(|error| {
                tracing::error!(%error, "failed to read Missing-files last-viewed timestamp");
                0
            });
        let new_missing_count = queries::count_new_missing(&conn, last_viewed_missing)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to count new missing files for sidebar badge");
                0
            });
        let active_import_error_count = queries::count_import_errors_active(&conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to count active import errors for sidebar visibility");
                0
            });
        let dismissed_import_error_count = queries::count_dismissed_import_errors(&conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to count dismissed import errors for sidebar reachability");
                0
            });
        let last_viewed_import_errors = settings::get_last_viewed_import_errors(&conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to read Import-errors last-viewed timestamp");
                0
            });
        let new_import_error_count =
            queries::count_new_import_errors(&conn, last_viewed_import_errors).unwrap_or_else(
                |error| {
                    tracing::error!(%error, "failed to count new import errors for sidebar badge");
                    0
                },
            );
        let playlist_rows = playlists::list(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to list playlists for sidebar");
            Vec::new()
        });
        // Each smart list carries its own live track count so the sidebar can
        // badge it like a manual playlist (SIDEBAR-badge parity). The count is
        // the rule-based `ViewSource::Smart` count — the same query the list
        // itself resolves to — so the badge always matches what opening the
        // list shows. Paired with its row here (rather than as a parallel vec)
        // so the count travels with the row it belongs to.
        let smart_rows: Vec<(playlists::SmartPlaylist, i64)> = playlists::list_smart(&conn)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to list smart playlists for sidebar");
                Vec::new()
            })
            .into_iter()
            .map(|smart| {
                let count =
                    queries::query_track_count(&conn, &ViewSource::Smart(smart.id), "", &[])
                        .unwrap_or_else(|error| {
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
        let podcasts_enabled = modules::is_enabled(&conn, &PODCASTS_MODULE).unwrap_or(false);
        let podcasts_count = if podcasts_enabled {
            podcasts::query::count_unplayed(&conn).map_or_else(
                |error| {
                    tracing::error!(%error, "failed to count unplayed podcast episodes");
                    0
                },
                |count| i64::try_from(count).unwrap_or(i64::MAX),
            )
        } else {
            0
        };
        let radio_enabled = modules::is_enabled(&conn, &RADIO_MODULE).unwrap_or(false);
        let radio_count = if radio_enabled {
            radio::station::count_stations(&conn).map_or_else(
                |error| {
                    tracing::error!(%error, "failed to count favorite radio stations");
                    0
                },
                |count| i64::try_from(count).unwrap_or(i64::MAX),
            )
        } else {
            0
        };
        let releases_enabled = modules::is_enabled(&conn, &NEW_RELEASES_MODULE).unwrap_or(false);
        let releases_count = if releases_enabled {
            artist_news::persisted_releases_filter(&conn)
                .and_then(|filter| artist_news::count_releases_view(&conn, &filter, today))
                .unwrap_or_else(|error| {
                    tracing::error!(%error, "failed to count Releases rows for sidebar badge");
                    0
                })
        } else {
            0
        };
        let concerts_enabled = modules::is_enabled(&conn, &CONCERTS_MODULE).unwrap_or(false);
        let concerts_count = if concerts_enabled {
            concerts::config::persisted_filter(&conn)
                .and_then(|filter| {
                    let location = concerts::config::location(&conn)?;
                    concerts::count_upcoming(&conn, &filter, location.as_ref(), today)
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
            active_import_error_count,
            dismissed_import_error_count,
            new_import_error_count,
            playlist_rows,
            smart_rows,
            podcasts_enabled,
            podcasts_count,
            radio_enabled,
            radio_count,
            releases_enabled,
            releases_count,
            concerts_enabled,
            concerts_count,
        )
    };
    let queue_count = (shared.queue_len_provider)() as i64;
    let playlist_count = playlist_rows.len();

    shared.listbox.remove_all();
    shared.issues_listbox.remove_all();
    shared.rows.borrow_mut().clear();
    *shared.new_playlist_row.borrow_mut() = None;
    *shared.import_playlist_row.borrow_mut() = None;

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

    sidebar_presentation::append_header(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_PLAYLISTS),
    );
    for playlist in &playlist_rows {
        add_row(
            shared,
            ViewSource::Playlist(playlist.id),
            &playlist.name,
            sidebar_presentation::nonzero_count(playlist.track_count),
            NavIcon::Playlist,
        );
    }
    let action_rows = sidebar_presentation::append_playlist_action_rows(&shared.listbox);
    *shared.new_playlist_row.borrow_mut() = Some(action_rows.new_playlist);
    *shared.import_playlist_row.borrow_mut() = Some(action_rows.import_playlist);

    sidebar_presentation::append_header(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_SMART),
    );
    for (smart, count) in &smart_rows {
        add_row(
            shared,
            ViewSource::Smart(smart.id),
            &smart.name,
            sidebar_presentation::nonzero_count(*count),
            sidebar_presentation::smart_icon(&smart.sort_field),
        );
    }
    if releases_enabled {
        add_row(
            shared,
            ViewSource::Releases,
            &strings::text(strings::RELEASES),
            sidebar_presentation::nonzero_count(releases_count),
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

    // INST-13: the instrumental conversions view is reachable from the sidebar
    // only while the experimental switch is on (INST-11) — the same gate that
    // adds its content page, so the row can never select a missing page.
    if crate::ui::instrumental::experimental_enabled(&shared.conn.borrow()) {
        add_row(
            shared,
            ViewSource::Conversions,
            &strings::text(strings::CONVERSION_TITLE),
            None,
            NavIcon::Conversions,
        );
    }

    // Dismissed import errors stay reachable through the triage view's
    // collapsed footer, even though they never contribute to the badge.
    let has_import_errors = active_import_error_count > 0 || dismissed_import_error_count > 0;
    let has_issues = has_import_errors || missing_count > 0;
    shared.issues_listbox.set_visible(has_issues);
    if has_issues {
        sidebar_presentation::append_problem_header(&shared.issues_listbox);
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
    }

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

fn add_row(
    shared: &Rc<Shared>,
    source: ViewSource,
    title: &str,
    count: Option<i64>,
    icon: NavIcon,
) {
    let row = sidebar_presentation::build_nav_row(title, count, icon);
    match &source {
        ViewSource::Playlist(playlist_id) => {
            sidebar_dnd::wire_playlist_drop_target(shared, &row, *playlist_id, title);
            sidebar_export::wire_playlist_context_menu(shared, &row, *playlist_id, title);
        }
        ViewSource::Queue => {
            sidebar_dnd::wire_queue_drop_target(shared, &row);
        }
        ViewSource::Conversions => {
            sidebar_dnd::wire_conversion_drop_target(shared, &row);
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

#[allow(dead_code)]
fn add_row_with_badge(
    shared: &Rc<Shared>,
    source: ViewSource,
    title: &str,
    badge_text: &str,
    icon: NavIcon,
) {
    let row = sidebar_presentation::build_nav_row_with_badge(title, badge_text, icon);
    shared.listbox.append(&row);
    shared
        .rows
        .borrow_mut()
        .push((row, source, title.to_string()));
}
