//! Read-only Concerts cache listing.

use chrono::Local;
use reprise_core::concerts::{self, ConcertFilter, ConcertRow};
use reprise_core::db::Db;
use serde_json::{json, Value};

use crate::cli::ConcertsAction;
use crate::error::CliError;
use crate::output::{print_json, sanitize_for_terminal};

pub fn run(db: &Db, action: &ConcertsAction, json_output: bool) -> Result<(), CliError> {
    match action {
        ConcertsAction::List { all, limit } => list(db, *all, *limit, json_output),
    }
}

fn list(db: &Db, all: bool, limit: Option<usize>, json_output: bool) -> Result<(), CliError> {
    let filter = if all {
        ConcertFilter {
            include_similar: true,
            ..ConcertFilter::default()
        }
    } else {
        concerts::config::persisted_filter(db)?
    };
    let location = concerts::config::location(db)?;
    let mut events =
        concerts::query_events(db, &filter, location.as_ref(), Local::now().date_naive())?;
    if let Some(limit) = limit {
        events.truncate(limit);
    }
    if json_output {
        let events = events.iter().map(event_json).collect::<Vec<_>>();
        print_json(&json!({
            "events": events,
            "filter_applied": !all,
            "latest_fetch_at": concerts::latest_fetch_at(db)?,
        }));
    } else {
        for event in events {
            println!("{}", human_line(&event));
        }
    }
    Ok(())
}

fn event_json(event: &ConcertRow) -> Value {
    json!({
        "date": event.date_key,
        "starts_at": event.starts_at,
        "artist": event.artist_name,
        "venue": event.venue,
        "city": event.city,
        "region": event.region,
        "country": event.country,
        "distance_km": event.distance_km,
        "ticket_url": event.ticket_url,
        "ticket_source": event.ticket_source,
        "event_url": event.event_url,
        "provider": event.provider,
        "is_similar": event.is_similar,
        "similar_to": event.similar_to,
    })
}

fn human_line(event: &ConcertRow) -> String {
    let country = event
        .country
        .as_deref()
        .filter(|country| !country.trim().is_empty())
        .map_or_else(String::new, |country| {
            format!(" ({})", sanitize_for_terminal(country))
        });
    let distance = event.distance_km.map_or_else(String::new, |distance| {
        format!(" · {:.0} km", distance.round())
    });
    let target = event
        .ticket_url
        .as_deref()
        .or(event.event_url.as_deref())
        .map_or_else(String::new, |url| {
            format!(" · {}", sanitize_for_terminal(url))
        });
    format!(
        "{}  {} — {}, {}{}{}{}",
        sanitize_for_terminal(&event.date_key),
        sanitize_for_terminal(&event.artist_name),
        sanitize_for_terminal(&event.venue),
        sanitize_for_terminal(&event.city),
        country,
        distance,
        target
    )
}
