use reprise_core::modules::ModuleDescriptor;

use super::strings;

pub(super) fn plugin_applies_live(id: &str) -> bool {
    matches!(
        id,
        "cover_download" | "artist_news" | "listenbrainz" | "lastfm"
    )
}

pub(super) fn plugin_title(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "cover_download" => strings::DOWNLOAD_MISSING_COVERS,
        "listenbrainz" => strings::LISTENBRAINZ,
        "lastfm" => strings::LASTFM,
        "artist_news" => strings::ARTIST_NEWS,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

pub(super) fn plugin_description(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "mpris" => strings::PLUGIN_MPRIS_DESCRIPTION,
        "cover_download" => strings::PLUGIN_COVER_DESCRIPTION,
        "listenbrainz" => strings::PLUGIN_LISTENBRAINZ_DESCRIPTION,
        "lastfm" => strings::PLUGIN_LASTFM_DESCRIPTION,
        "artist_news" => strings::ARTIST_NEWS_DESCRIPTION,
        _ => return descriptor.description.to_string(),
    };
    strings::text(message)
}
