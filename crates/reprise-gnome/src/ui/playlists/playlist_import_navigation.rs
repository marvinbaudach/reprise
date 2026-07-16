use reprise_core::view_source::ViewSource;

pub(in crate::ui) fn target_for_import(playlist_id: i64) -> ViewSource {
    ViewSource::Playlist(playlist_id)
}

#[cfg(test)]
mod tests {
    use reprise_core::view_source::ViewSource;

    #[test]
    fn successful_import_selects_the_created_playlist_source() {
        assert_eq!(super::target_for_import(42), ViewSource::Playlist(42));
    }
}
