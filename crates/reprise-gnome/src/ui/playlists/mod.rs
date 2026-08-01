pub(in crate::ui) use reprise_view::playlists as playlist_import_navigation;
pub(crate) mod playlist_io;

pub(in crate::ui) mod playlist_io_names {
    use std::path::Path;

    pub(in crate::ui) use reprise_view::playlists::display_name;

    use crate::ui::strings;

    pub(in crate::ui) fn playlist_name_from_file(file_path: &Path) -> String {
        let fallback = strings::text(strings::IMPORTED_PLAYLIST_FALLBACK_NAME);
        reprise_view::playlists::playlist_name_from_file(file_path, &fallback)
    }
}

#[allow(unused_imports)]
use super::*;
