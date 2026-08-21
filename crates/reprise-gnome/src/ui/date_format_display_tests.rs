//! STYLE-11 across the real tables, under a pinned pattern.

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

/// STYLE-11: the real concerts view, with `REPRISE_DATE_PATTERN` pinned to the
/// day-first shape — measured against the widget that actually renders, not
/// against the formatting function, because the drift this rule removes lived
/// in the call sites rather than in the formatter. The concerts table is the
/// surface that used to write `Sat, Oct 17` here and `Sat, 17 Oct` in the
/// Updates panel for the same event.
///
/// The releases table is proved by
/// `releases_columns::tests::style_11_the_releases_date_column_renders_the_pinned_pattern`
/// instead of here: its view stays on the "No discography data yet" empty
/// state unless a whole fetch pipeline has run, so a full-view fixture asserts
/// against an empty table and passes for the wrong reason.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_11_the_concerts_view_renders_the_pinned_pattern() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    std::env::set_var(crate::ui::date_format::PATTERN_ENV, "%d.%m.%Y");
    gtk4::init().unwrap();

    // The process resolves its date format exactly once, so the override above
    // only takes effect if this test is the first read in the process — which
    // is why display tests here run one per process (`--exact`). Assert it
    // rather than trust it: on a machine whose own locale is already day-first
    // a batch run would otherwise pass for the wrong reason.
    assert_eq!(
        crate::ui::date_format::current().date,
        reprise_core::format::DatePattern::from_platform("%d.%m.%Y"),
        "another test resolved the date format first; run this one with --exact"
    );

    // The concerts view only queries `date_key >= today`, so the seeded event
    // has to stay ahead of the real clock — a pinned calendar date renders
    // nothing at all once that day passes, and the assert below would then
    // blame the date pattern for an empty table.
    let event_date = chrono::Local::now().date_naive() + chrono::Duration::days(30);
    let date_key = event_date.format("%Y-%m-%d").to_string();
    let expected_label = event_date.format("%d.%m.%Y").to_string();
    let concerts_db = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&concerts_db)
        .execute(
            "INSERT INTO concert_events (
               id, artist_key, artist_name, starts_at, date_key, venue, city,
               country, provider, fetched_at, dedupe_key
             ) VALUES (1, 'artist-id', 'Artist', ?1,
                       ?2, 'Venue', 'Zurich', 'CH', 'fixture', 1,
                       'style-11-concert')",
            rusqlite::params![format!("{date_key}T19:00:00"), date_key],
        )
        .unwrap();
    let runtime = crate::ui::concerts::ConcertsRuntime::setup(&concerts_db);
    let concerts = crate::ui::concerts::ConcertsView::new(
        concerts_db,
        &runtime,
        &Rc::new(crate::ui::location_broadcast::LocationBroadcast::default()),
    );
    concerts.refresh();

    let tables = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    tables.append(concerts.root());
    let window = gtk4::Window::new();
    window.set_default_size(1200, 800);
    window.set_child(Some(&tables));
    window.present();
    crate::ui::source_context_surface::settle_layout();

    let labels = descendant_labels(tables.upcast_ref());
    assert!(
        labels.iter().any(|label| label.text() == expected_label),
        "no rendered concert label read {expected_label}, the pinned day-first \
         pattern; rendered labels were {:?}",
        labels
            .iter()
            .map(|label| label.text().to_string())
            .collect::<Vec<_>>()
    );
}
