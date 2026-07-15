use reprise_core::modules::ModuleDescriptor;

use super::strings;

pub(super) fn plugin_applies_live(id: &str) -> bool {
    matches!(id, "artist_news" | "listenbrainz" | "lastfm")
}

pub(super) fn plugin_title(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::LISTENBRAINZ,
        "lastfm" => strings::LASTFM,
        "artist_news" => strings::ARTIST_NEWS,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

pub(super) fn plugin_description(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::PLUGIN_LISTENBRAINZ_DESCRIPTION,
        "lastfm" => strings::PLUGIN_LASTFM_DESCRIPTION,
        "artist_news" => strings::ARTIST_NEWS_DESCRIPTION,
        _ => return descriptor.description.to_string(),
    };
    strings::text(message)
}
