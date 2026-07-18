use reprise_core::modules::ModuleDescriptor;

use super::strings;

pub(in crate::ui) fn plugin_applies_live(descriptor: &ModuleDescriptor) -> bool {
    descriptor.applies_live
}

pub(in crate::ui) fn plugin_title(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::LISTENBRAINZ,
        "lastfm" => strings::LASTFM,
        "new_releases" => strings::NEW_RELEASES,
        "cover_download" => strings::COVER_DOWNLOAD,
        "artist_portraits" => strings::ARTIST_PORTRAITS,
        "online_lyrics" => strings::ONLINE_LYRICS,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

pub(in crate::ui) fn plugin_description(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::PLUGIN_LISTENBRAINZ_DESCRIPTION,
        "lastfm" => strings::PLUGIN_LASTFM_DESCRIPTION,
        "new_releases" => strings::NEW_RELEASES_DESCRIPTION,
        "cover_download" => strings::COVER_DOWNLOAD_DESCRIPTION,
        "artist_portraits" => strings::ARTIST_PORTRAITS_DESCRIPTION,
        "online_lyrics" => strings::ONLINE_LYRICS_DESCRIPTION,
        _ => return descriptor.description.to_string(),
    };
    strings::text(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr_7_new_releases_plugin_uses_privacy_copy_and_live_toggle_id() {
        let descriptor = &reprise_core::modules::NEW_RELEASES_MODULE;

        assert_eq!(plugin_title(descriptor), "New Releases");
        assert!(plugin_description(descriptor).contains("contacts MusicBrainz"));
        assert!(plugin_applies_live(descriptor));
    }
}
