use reprise_core::view_source::ViewSource;

/// An empty playlist is only a new destination. Refresh the sidebar while
/// preserving its current source so the user can keep selecting tracks to
/// fill it.
pub(super) fn refresh_target_after_empty_creation() -> Option<ViewSource> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_playlist_creation_keeps_the_current_library_source() {
        assert_eq!(super::refresh_target_after_empty_creation(), None);
    }
}
