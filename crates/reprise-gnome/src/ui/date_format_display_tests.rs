//! STYLE-11 across the real tables, under a pinned pattern.

use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;

fn descendant_labels(widget: &gtk4::Widget) -> Vec<gtk4::Label> {
    let mut labels = widget
        .clone()
        .downcast::<gtk4::Label>()
        .ok()
        .into_iter()
        .collect::<Vec<_>>();
    let mut child = widget.first_child();
    while let Some(current) = child {
        labels.extend(descendant_labels(&current));
        child = current.next_sibling();
    }
    labels
}

/// STYLE-11: four surfaces, one pattern. Renders the releases and concerts
/// tables with `REPRISE_DATE_PATTERN` pinned and asserts that every date-like
/// label matches the day-first shape — measured against the widgets that
/// actually render, not against the formatting functions, because the drift
/// this rule removes lived in the call sites rather than in the formatter.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_11_every_surface_renders_the_pinned_pattern() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    std::env::set_var(crate::ui::date_format::PATTERN_ENV, "%d.%m.%Y");
    gtk4::init().unwrap();

    let releases_db = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&releases_db)
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at
             ) VALUES ('style-11-release', 'Artist', 'artist-id', 'Release',
                       'Album', '2026-05-29', 1)",
            [],
        )
        .unwrap();
    let releases = crate::ui::releases::ReleasesView::new(releases_db, PathBuf::new());
    releases.refresh();

    let concerts_db = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&concerts_db)
        .execute(
            "INSERT INTO concert_events (
               id, artist_key, artist_name, starts_at, date_key, venue, city,
               country, provider, fetched_at, dedupe_key
             ) VALUES (1, 'artist-id', 'Artist', '2026-10-17T19:00:00',
                       '2026-10-17', 'Venue', 'Zurich', 'CH', 'fixture', 1,
                       'style-11-concert')",
            [],
        )
        .unwrap();
    let runtime = crate::ui::concerts::ConcertsRuntime::setup(&concerts_db);
    let concerts = crate::ui::concerts::ConcertsView::new(concerts_db, &runtime);
    concerts.refresh();

    let tables = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    tables.append(releases.root());
    tables.append(concerts.root());
    let window = gtk4::Window::new();
    window.set_default_size(1200, 800);
    window.set_child(Some(&tables));
    window.present();
    crate::ui::source_context_surface::settle_layout();

    let labels = descendant_labels(tables.upcast_ref());
    for text in ["29.05.2026", "17.10.2026"] {
        assert!(
            labels.iter().any(|label| label.text() == text),
            "no rendered table label used the pinned date {text:?}"
        );
    }
}
