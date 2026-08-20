use super::*;

fn visual_fixture_row(
    id: i64,
    artist: &str,
    venue: &str,
    city: &str,
    distance_km: f64,
    availability: reprise_core::concerts::TicketAvailability,
    similar_to: Option<&str>,
) -> ConcertRow {
    ConcertRow {
        id,
        availability,
        date_key: format!("2026-08-{}", 19 + id),
        starts_at: format!("2026-08-{}T19:30:00", 19 + id),
        artist_name: artist.into(),
        venue: venue.into(),
        city: city.into(),
        region: None,
        country: Some("CH".into()),
        latitude: None,
        longitude: None,
        distance_km: Some(distance_km),
        ticket_url: Some(format!("https://tickets.example/{id}")),
        ticket_source: Some(if id % 2 == 0 {
            "Bandsintown".into()
        } else {
            "Ticketmaster".into()
        }),
        event_url: Some(format!("https://events.example/{id}")),
        provider: "fixture".into(),
        is_similar: similar_to.is_some(),
        similar_to: similar_to.map(str::to_owned),
    }
}

#[test]
#[ignore = "visual fixture; run through the isolated CUA session"]
fn concerts_visual_acceptance_fixture() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceLight);
    crate::ui::style::install_css_string_for_test(&format!(
        "{}\n{}",
        crate::ui::style::theme::theme_css(
            crate::ui::style::theme::Theme::DEFAULT,
            false,
            crate::ui::style::accent::AccentSource::App,
        ),
        crate::ui::style::app_css_for_test(),
    ));

    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::location::store(&conn, 47.3769, 8.5417, "Zürich", Some("CH")).unwrap();
    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::FILTER_RADIUS_KEY,
        "500",
    )
    .unwrap();
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let rows = vec![
        visual_fixture_row(
            1,
            "Lorna Shore",
            "Hallenstadion",
            "Zürich",
            14.8,
            reprise_core::concerts::TicketAvailability::OnSale,
            None,
        ),
        visual_fixture_row(
            2,
            "Architects",
            "Komplex 457",
            "Zürich",
            28.0,
            reprise_core::concerts::TicketAvailability::OffSale,
            Some("Bring Me the Horizon"),
        ),
        visual_fixture_row(
            3,
            "Spiritbox",
            "Z7 Konzertfabrik",
            "Pratteln",
            500.0,
            reprise_core::concerts::TicketAvailability::Unknown,
            None,
        ),
        visual_fixture_row(
            4,
            "Sleep Token",
            "Olympiahalle",
            "München",
            500.1,
            reprise_core::concerts::TicketAvailability::OnSale,
            None,
        ),
        visual_fixture_row(
            5,
            "Bad Omens",
            "Mercedes-Benz Arena",
            "Berlin",
            850.0,
            reprise_core::concerts::TicketAvailability::OffSale,
            None,
        ),
    ];
    let mode = std::env::var("REPRISE_SMOKE_CONCERTS_FIXTURE").unwrap_or_else(|_| "table".into());
    let shown_rows = if mode == "end" {
        rows.into_iter().take(3).collect()
    } else {
        rows
    };
    view.shared.model.replace(shown_rows);
    view.shared.stack.set_visible_child_name(LIST_PAGE);
    if mode == "source" {
        view.column_model().set_visible("source", true);
    }
    let now = chrono::Utc::now().timestamp();
    view.shared.footer.apply(match mode.as_str() {
        "cached" => FeedFooterState::Cached { at: now },
        "fetching" => FeedFooterState::Fetching {
            checked: 137,
            total: 415,
        },
        "failed" => FeedFooterState::Failed { latest: now },
        "offline" => FeedFooterState::Offline { latest: now },
        _ => FeedFooterState::Loaded { at: now },
    });

    let window = gtk4::Window::builder()
        .title("Reprise Concerts visual acceptance")
        .default_width(1380)
        .default_height(760)
        .child(view.root())
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();
    if mode == "end" {
        view.shared
            .end_of_results
            .update(super::super::super::concerts_end_of_results::Input {
                shown: 3,
                total: 415,
                query: String::new(),
                facets_restrict: true,
                radius_km: Some(500.0),
                city: Some("Zürich".into()),
            });
        crate::ui::source_context_surface::settle_layout();
    }
    let hold_ms = std::env::var("REPRISE_SMOKE_HOLD_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(250);
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(hold_ms),
    ));
    window.close();
}
