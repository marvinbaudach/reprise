//! Desktop file-open dispatch shared by primary startup and forwarded
//! single-instance requests. Audio files are deliberately resolved against
//! existing library rows only; opening a file must never silently import it.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::queries;

use super::player_controller::PlayerController;
use super::playlist_io;
use super::sidebar::Sidebar;
use super::{strings, toasts};

const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];
const PLAYLIST_EXTENSIONS: [&str; 2] = ["m3u", "m3u8"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenFileKind {
    Audio,
    Playlist,
    Unsupported,
}

#[derive(Debug, PartialEq, Eq)]
struct AudioResolution {
    ids: Vec<i64>,
    unresolved: Vec<PathBuf>,
}

fn classify_path(path: &Path) -> OpenFileKind {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return OpenFileKind::Unsupported;
    };
    let extension = extension.to_ascii_lowercase();
    if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        OpenFileKind::Audio
    } else if PLAYLIST_EXTENSIONS.contains(&extension.as_str()) {
        OpenFileKind::Playlist
    } else {
        OpenFileKind::Unsupported
    }
}

fn track_id_for_open_path(conn: &Connection, path: &Path) -> Option<i64> {
    let path_text = path.to_string_lossy();
    match queries::track_id_for_path(conn, &path_text) {
        Ok(Some(id)) => return Some(id),
        Ok(None) => {}
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "file-open track lookup failed");
            return None;
        }
    }

    let Ok(canonical) = path.canonicalize() else {
        return None;
    };
    if canonical == path {
        return None;
    }
    let canonical_text = canonical.to_string_lossy();
    match queries::track_id_for_path(conn, &canonical_text) {
        Ok(id) => id,
        Err(error) => {
            tracing::error!(
                %error,
                path = %canonical.display(),
                "canonical file-open track lookup failed"
            );
            None
        }
    }
}

fn resolve_audio_ids(conn: &Connection, paths: &[PathBuf]) -> AudioResolution {
    let mut ids = Vec::with_capacity(paths.len());
    let mut unresolved = Vec::new();
    for path in paths {
        match track_id_for_open_path(conn, path) {
            Some(id) => ids.push(id),
            None => unresolved.push(path.clone()),
        }
    }
    AudioResolution { ids, unresolved }
}

#[derive(Clone)]
pub(crate) struct FileOpenHandler {
    window: adw::ApplicationWindow,
    conn: Rc<RefCell<Connection>>,
    player: Option<Rc<PlayerController>>,
    toast_overlay: adw::ToastOverlay,
    sidebar: Rc<Sidebar>,
}

impl FileOpenHandler {
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        conn: Rc<RefCell<Connection>>,
        player: Option<Rc<PlayerController>>,
        toast_overlay: &adw::ToastOverlay,
        sidebar: Rc<Sidebar>,
    ) -> Self {
        Self {
            window: window.clone(),
            conn,
            player,
            toast_overlay: toast_overlay.clone(),
            sidebar,
        }
    }

    pub(crate) fn present(&self) {
        self.window.present();
    }

    pub(crate) fn open(&self, files: &[gio::File]) {
        self.present();

        let mut audio_paths = Vec::new();
        let mut playlist_paths = Vec::new();
        let mut unsupported = 0;
        for file in files {
            let Some(path) = file.path() else {
                unsupported += 1;
                continue;
            };
            if !path.is_file() {
                unsupported += 1;
                continue;
            }
            match classify_path(&path) {
                OpenFileKind::Audio => audio_paths.push(path),
                OpenFileKind::Playlist => playlist_paths.push(path),
                OpenFileKind::Unsupported => unsupported += 1,
            }
        }

        for path in playlist_paths {
            let result = playlist_io::import_playlist(&self.conn, &path);
            playlist_io::apply_import_result(result, &self.toast_overlay, &self.sidebar);
        }

        if !audio_paths.is_empty() {
            let resolution = resolve_audio_ids(&self.conn.borrow(), &audio_paths);
            if !resolution.ids.is_empty() {
                match &self.player {
                    Some(player) => {
                        tracing::info!(
                            count = resolution.ids.len(),
                            "playing files opened through desktop association"
                        );
                        player.play_from_view(
                            resolution.ids,
                            0,
                            crate::ui::playback::play_origin::PlayOrigin::library(),
                        );
                    }
                    None => toasts::show(
                        &self.toast_overlay,
                        &strings::file_open_playback_unavailable_toast(),
                    ),
                }
            }
            if !resolution.unresolved.is_empty() {
                toasts::show(
                    &self.toast_overlay,
                    &strings::file_open_not_in_library_toast(resolution.unresolved.len()),
                );
            }
        }

        if unsupported > 0 {
            toasts::show(
                &self.toast_overlay,
                &strings::file_open_unsupported_toast(unsupported),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use reprise_core::db;

    use super::{classify_path, resolve_audio_ids, OpenFileKind};

    #[test]
    fn supported_extensions_are_classified_case_insensitively() {
        for extension in ["mp3", "FLAC", "Ogg", "OPUS", "m4a", "AAC", "wav"] {
            assert_eq!(
                classify_path(Path::new(&format!("track.{extension}"))),
                OpenFileKind::Audio
            );
        }
        for extension in ["m3u", "M3U8"] {
            assert_eq!(
                classify_path(Path::new(&format!("mix.{extension}"))),
                OpenFileKind::Playlist
            );
        }
        assert_eq!(
            classify_path(Path::new("cover.jpg")),
            OpenFileKind::Unsupported
        );
        assert_eq!(
            classify_path(Path::new("extensionless")),
            OpenFileKind::Unsupported
        );
    }

    #[test]
    fn audio_resolution_preserves_order_and_canonicalizes_paths_without_inserting() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();
        let root = std::env::temp_dir().join(format!(
            "reprise-file-open-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let first = root.join("first.flac");
        let second = root.join("second.flac");
        std::fs::write(&first, b"fixture").unwrap();
        std::fs::write(&second, b"fixture").unwrap();
        let first_canonical = first.canonicalize().unwrap();
        let second_text = second.to_string_lossy().into_owned();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, ?1, 'First', '', 0)",
            [first_canonical.to_string_lossy().as_ref()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (2, ?1, 'Second', '', 0)",
            [&second_text],
        )
        .unwrap();

        let through_parent = nested.join("..").join("first.flac");
        let unknown = root.join("unknown.flac");
        let result = resolve_audio_ids(&conn, &[second.clone(), through_parent, unknown.clone()]);

        assert_eq!(result.ids, vec![2, 1]);
        assert_eq!(result.unresolved, vec![unknown]);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2, "opening files must never insert library rows");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn desktop_and_appstream_advertise_the_same_supported_media() {
        let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let desktop = std::fs::read_to_string(data.join("org.reprise.Reprise.desktop")).unwrap();
        let metainfo =
            std::fs::read_to_string(data.join("org.reprise.Reprise.metainfo.xml")).unwrap();
        let mime_types = [
            "audio/mpeg",
            "audio/flac",
            "audio/ogg",
            "audio/x-opus+ogg",
            "audio/mp4",
            "audio/aac",
            "audio/x-wav",
            "audio/x-mpegurl",
            "application/vnd.apple.mpegurl",
        ];

        assert!(desktop.contains("Exec=reprise %F"));
        for mime_type in mime_types {
            assert!(
                desktop.contains(mime_type),
                "desktop file is missing {mime_type}"
            );
            assert!(
                metainfo.contains(&format!("<mediatype>{mime_type}</mediatype>")),
                "AppStream metadata is missing {mime_type}"
            );
        }
    }
}
