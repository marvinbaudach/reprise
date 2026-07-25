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

pub fn dedupe_key(date_key: &str, city: &str, venue: &str) -> String {
    format!(
        "{}|{}|{}",
        date_key.trim(),
        normalize_component(city),
        normalize_component(venue)
    )
}

pub fn merge(events: Vec<ProviderEvent>) -> Vec<ProviderEvent> {
    let mut merged = Vec::with_capacity(events.len());
    let mut positions = HashMap::new();
    for event in events {
        let key = dedupe_key(&event.date_key, &event.city, &event.venue);
        if let Some(&position) = positions.get(&key) {
            let existing: &ProviderEvent = &merged[position];
            if existing.provider == ProviderKind::Ticketmaster
                && event.provider == ProviderKind::Bandsintown
            {
                merged[position] = event;
            }
            continue;
        }
        positions.insert(key, merged.len());
        merged.push(event);
    }
    merged
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
