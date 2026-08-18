use std::collections::HashMap;

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use url::Url;

use super::provider::{ProviderEvent, ProviderKind};

pub fn normalize_component(value: &str) -> String {
    value
        .trim()
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn dedupe_key(artist_key: &str, date_key: &str, city: &str) -> String {
    format!(
        "{}|{}|{}",
        normalize_component(artist_key),
        normalize_component(date_key),
        normalize_component(city)
    )
}

pub fn merge(artist_key: &str, events: Vec<ProviderEvent>) -> Vec<ProviderEvent> {
    let mut merged = Vec::with_capacity(events.len());
    let mut positions = HashMap::new();
    for event in events {
        let key = dedupe_key(artist_key, &event.date_key, &event.city);
        if let Some(&position) = positions.get(&key) {
            let existing: &ProviderEvent = &merged[position];
            if ProviderKind::listing_winner_is_incoming(
                existing.provider,
                existing.ticket_url.as_deref(),
                event.provider,
                event.ticket_url.as_deref(),
            ) {
                merged[position] = event;
            }
            continue;
        }
        positions.insert(key, merged.len());
        merged.push(event);
    }
    merged
}

impl ProviderKind {
    pub(crate) fn listing_winner_is_incoming(
        existing_provider: Self,
        existing_ticket_url: Option<&str>,
        incoming_provider: Self,
        incoming_ticket_url: Option<&str>,
    ) -> bool {
        if existing_provider == incoming_provider {
            let existing_is_owned =
                provider_owns_ticket_url(existing_provider, existing_ticket_url);
            let incoming_is_owned =
                provider_owns_ticket_url(incoming_provider, incoming_ticket_url);
            return incoming_is_owned && !existing_is_owned;
        }

        existing_provider == Self::Ticketmaster && incoming_provider == Self::Bandsintown
    }
}

pub(super) fn provider_owns_ticket_url(
    provider: ProviderKind,
    ticket_url: Option<&str>,
) -> bool {
    let expected_label = match provider {
        ProviderKind::Bandsintown => "bandsintown",
        ProviderKind::Ticketmaster => "ticketmaster",
    };
    ticket_url
        .and_then(|value| Url::parse(value).ok())
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| registrable_domain_label(&host) == Some(expected_label))
}

fn registrable_domain_label(host: &str) -> Option<&str> {
    let mut labels = host.rsplit('.');
    let top_level = labels.next()?;
    let candidate = labels.next()?;
    if top_level.len() == 2 && matches!(candidate, "co" | "com" | "net" | "org") {
        labels.next()
    } else {
        Some(candidate)
    }
}

pub fn ticket_source_label(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    for (needle, label) in [
        ("eventim.", "Eventim"),
        ("ticketmaster.", "Ticketmaster"),
        ("bandsintown.com", "Bandsintown"),
        ("seetickets.", "See Tickets"),
        ("ticketcorner.", "Ticketcorner"),
    ] {
        if host == needle.trim_end_matches('.') || host.contains(needle) {
            return Some(label.to_owned());
        }
    }
    fallback_domain_label(&host)
}

fn fallback_domain_label(host: &str) -> Option<String> {
    let labels = host
        .split('.')
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    if labels.len() < 2 {
        return None;
    }
    let suffix_offset = if labels.last().is_some_and(|label| label.len() == 2)
        && labels
            .get(labels.len().saturating_sub(2))
            .is_some_and(|label| matches!(*label, "co" | "com" | "net" | "org"))
    {
        3
    } else {
        2
    };
    let label = labels.get(labels.len().checked_sub(suffix_offset)?)?;
    let mut characters = label.chars();
    let first = characters.next()?.to_uppercase().collect::<String>();
    Some(format!("{first}{}", characters.as_str()))
}

#[cfg(test)]
#[path = "dedupe_tests.rs"]
mod tests;
