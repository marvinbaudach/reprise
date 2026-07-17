use std::path::Path;

use rusqlite::Connection;
use rusqlite::OptionalExtension;

use super::scanner::ScanError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelinkMismatch {
    pub old_duration_ms: i64,
    pub new_duration_ms: i64,
    pub old_title: String,
    pub new_title: Option<String>,
}

pub fn probe_relink(
    conn: &Connection,
    track_id: i64,
    new_path: &Path,
) -> Result<Option<RelinkMismatch>, ScanError> {
    let (old_duration_ms, old_title): (i64, String) = conn.query_row(
        "SELECT duration_ms, title FROM tracks WHERE id = ?1",
        [track_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let meta = super::scanner::track_meta::read_meta(new_path)?;
    let new_title = (!meta.title.is_empty()).then_some(meta.title);
    let duration_mismatch = old_duration_ms.abs_diff(meta.duration_ms) > 2_000;
    let title_mismatch = new_title.as_deref().is_some_and(|title| title != old_title);
    if !duration_mismatch && !title_mismatch {
        return Ok(None);
    }
    Ok(Some(RelinkMismatch {
        old_duration_ms,
        new_duration_ms: meta.duration_ms,
        old_title,
        new_title,
    }))
}

pub fn relink_track(
    conn: &mut Connection,
    track_id: i64,
    new_path: &Path,
) -> Result<(), ScanError> {
    let meta = super::scanner::track_meta::read_meta(new_path)?;
    let title = if meta.title.is_empty() {
        new_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string()
    } else {
        meta.title.clone()
    };
    let (file_size, device, inode) = super::scanner::file_stat(new_path).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("relink source disappeared: {}", new_path.display()),
        )
    })?;
    let mount_point =
        super::mounts::mount_point_of(new_path).map(|path| path.to_string_lossy().into_owned());

    let tx = conn.transaction()?;
    let still_missing = tx
        .query_row(
            &format!(
                "SELECT 1 FROM tracks WHERE id = ?1 AND {}",
                crate::queries::MISSING
            ),
            [track_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !still_missing {
        return Err(ScanError::RelinkTargetChanged { track_id });
    }
    super::scanner::move_detect::apply_file_identity(
        &tx,
        track_id,
        new_path,
        &title,
        &meta,
        false,
        &super::scanner::move_detect::FileIdentity {
            file_mtime: super::scanner::file_mtime(new_path),
            file_size: file_size as i64,
            device: Some(device as i64),
            inode: Some(inode as i64),
            mount_point,
        },
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    fn fixture_copy(dir: &Path, name: &str) -> std::path::PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let destination = dir.join(name);
        std::fs::copy(source, &destination).unwrap();
        destination
    }

    fn tag_file(path: &Path, title: &str) {
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.into());
        tag.save_to_path(path, lofty::config::WriteOptions::default())
            .unwrap();
    }

    fn imported_missing_track(
        title: &str,
    ) -> (tempfile::TempDir, Connection, i64, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let old_path = fixture_copy(temp.path(), "old.flac");
        tag_file(&old_path, title);
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        super::super::scanner::scan_folder(&mut conn, temp.path()).unwrap();
        let track_id = conn
            .query_row(
                "SELECT id FROM tracks WHERE path = ?1",
                [old_path.to_string_lossy().as_ref()],
                |row| row.get(0),
            )
            .unwrap();
        let new_path = temp.path().join("new.flac");
        std::fs::rename(old_path, &new_path).unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = 10, missing_reason = 'deleted' WHERE id = ?1",
            [track_id],
        )
        .unwrap();
        (temp, conn, track_id, new_path)
    }

    #[test]
    fn probe_accepts_duration_delta_at_matcher_tolerance() {
        let (_temp, conn, track_id, new_path) = imported_missing_track("Same recording");
        conn.execute(
            "UPDATE tracks SET duration_ms = duration_ms + 2000 WHERE id = ?1",
            [track_id],
        )
        .unwrap();

        assert_eq!(probe_relink(&conn, track_id, &new_path).unwrap(), None);
    }

    #[test]
    fn probe_reports_old_and_new_values_for_a_mismatch() {
        let (_temp, conn, track_id, new_path) = imported_missing_track("Old title");
        tag_file(&new_path, "New title");
        conn.execute(
            "UPDATE tracks SET duration_ms = duration_ms + 3000 WHERE id = ?1",
            [track_id],
        )
        .unwrap();
        let old_duration_ms = conn
            .query_row(
                "SELECT duration_ms FROM tracks WHERE id = ?1",
                [track_id],
                |row| row.get(0),
            )
            .unwrap();
        let new_duration_ms = super::super::scanner::track_meta::read_meta(&new_path)
            .unwrap()
            .duration_ms;

        assert_eq!(
            probe_relink(&conn, track_id, &new_path).unwrap(),
            Some(RelinkMismatch {
                old_duration_ms,
                new_duration_ms,
                old_title: "Old title".into(),
                new_title: Some("New title".into()),
            })
        );
    }

    #[test]
    fn probe_warns_when_only_the_readable_title_changed() {
        let (_temp, conn, track_id, new_path) = imported_missing_track("Old title");
        tag_file(&new_path, "Different title");

        let mismatch = probe_relink(&conn, track_id, &new_path)
            .unwrap()
            .expect("a readable changed title must warn even at matching duration");
        assert_eq!(mismatch.old_title, "Old title");
        assert_eq!(mismatch.new_title.as_deref(), Some("Different title"));
    }

    #[test]
    fn relink_preserves_user_data_and_refreshes_the_existing_row() {
        let (_temp, mut conn, track_id, new_path) = imported_missing_track("Relinked title");
        conn.execute(
            "UPDATE tracks SET rating = 4, play_count = 9 WHERE id = ?1",
            [track_id],
        )
        .unwrap();

        relink_track(&mut conn, track_id, &new_path).unwrap();

        let row: (
            i64,
            String,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT id, path, rating, play_count, missing_since, removed_at, mount_point \
                 FROM tracks WHERE id = ?1",
                [track_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, track_id);
        assert_eq!(row.1, new_path.to_string_lossy());
        assert_eq!((row.2, row.3), (4, 9));
        assert_eq!((row.4, row.5), (None, None));
        assert!(row.6.is_some(), "relink must record the reachable mount");
    }

    #[test]
    fn stale_relink_does_not_overwrite_a_track_resurrected_while_dialog_was_open() {
        let (_temp, mut conn, track_id, new_path) = imported_missing_track("Original title");
        let original_path: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
                row.get(0)
            })
            .unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = NULL, missing_reason = NULL WHERE id = ?1",
            [track_id],
        )
        .unwrap();

        assert!(matches!(
            relink_track(&mut conn, track_id, &new_path),
            Err(ScanError::RelinkTargetChanged { track_id: changed }) if changed == track_id
        ));
        let path_after: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(path_after, original_path);
    }
}
