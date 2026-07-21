use std::fmt::Write;

use md5::{Digest, Md5};
use rusqlite::{params, Connection, OptionalExtension};

use super::{plan_mix, MixDraft, MixIntent, MixPlannerError, MixSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaylistCommit {
    pub playlist_id: i64,
}

pub fn plan_mix_draft(
    conn: &Connection,
    intent: &MixIntent,
    now: i64,
    ttl_seconds: i64,
) -> Result<MixDraft, MixPlannerError> {
    if now < 0 || ttl_seconds <= 0 {
        return Err(MixPlannerError::InvalidIntent("draft lifetime is invalid"));
    }
    let tx = conn.unchecked_transaction()?;
    let mut draft = plan_mix(&tx, intent)?;
    let mut snapshots = Vec::with_capacity(draft.tracks.len());
    for track in &draft.tracks {
        let snapshot: (i64, i64) = tx.query_row(
            "SELECT file_mtime, file_size FROM tracks WHERE id = ?1",
            [track.track_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        snapshots.push(snapshot);
    }
    draft.draft_id = persisted_draft_id(&draft, &snapshots)?;
    let expires_at = now.saturating_add(ttl_seconds);
    tx.execute(
        "DELETE FROM mix_drafts WHERE draft_id = ?1 AND status = 'current' AND expires_at <= ?2",
        params![draft.draft_id, now],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO mix_drafts
         (draft_id, draft_json, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            draft.draft_id,
            serde_json::to_string(&draft)?,
            now,
            expires_at
        ],
    )?;
    for (position, (track, (mtime, size))) in draft.tracks.iter().zip(snapshots).enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO mix_draft_tracks
             (draft_id, position, track_id, source_mtime, source_size)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![draft.draft_id, position as i64, track.track_id, mtime, size],
        )?;
    }
    tx.commit()?;
    load_mix_draft(conn, &draft.draft_id, now)
}

fn persisted_draft_id(
    draft: &MixDraft,
    snapshots: &[(i64, i64)],
) -> Result<String, MixPlannerError> {
    let mut digest = Md5::new();
    digest.update(draft.intent.to_json()?.as_bytes());
    for (track, (mtime, size)) in draft.tracks.iter().zip(snapshots) {
        digest.update(track.track_id.to_le_bytes());
        digest.update(mtime.to_le_bytes());
        digest.update(size.to_le_bytes());
    }
    let mut id = String::with_capacity(32);
    for byte in digest.finalize() {
        let _ = write!(id, "{byte:02x}");
    }
    Ok(id)
}

pub fn cleanup_expired_mix_drafts(
    conn: &Connection,
    now: i64,
    limit: usize,
) -> Result<u64, MixPlannerError> {
    let bounded = limit.min(100);
    if bounded == 0 {
        return Ok(0);
    }
    let changed = conn.execute(
        "DELETE FROM mix_drafts WHERE draft_id IN (
           SELECT draft_id FROM mix_drafts
           WHERE status = 'current' AND expires_at <= ?1
           ORDER BY expires_at, draft_id LIMIT ?2
         )",
        params![now, bounded as i64],
    )?;
    Ok(changed as u64)
}

pub fn load_mix_draft(
    conn: &Connection,
    draft_id: &str,
    now: i64,
) -> Result<MixDraft, MixPlannerError> {
    let json = conn
        .query_row(
            "SELECT draft_json FROM mix_drafts
             WHERE draft_id = ?1 AND status = 'current' AND expires_at > ?2",
            params![draft_id, now],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(MixPlannerError::InvalidIntent(
            "mix draft is stale or unavailable",
        ))?;
    Ok(serde_json::from_str(&json)?)
}

pub fn approve_mix_draft(
    conn: &mut Connection,
    draft_id: &str,
    playlist_name: &str,
    idempotency_key: &str,
    now: i64,
) -> Result<PlaylistCommit, MixPlannerError> {
    if idempotency_key.trim().is_empty() {
        return Err(MixPlannerError::InvalidIntent(
            "idempotency key is required",
        ));
    }
    let tx = conn.transaction()?;
    let row = tx
        .query_row(
            "SELECT draft_json, expires_at, status, approved_playlist_id
             FROM mix_drafts WHERE draft_id = ?1",
            [draft_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or(MixPlannerError::InvalidIntent("mix draft is unavailable"))?;
    if row.2 == "approved" {
        return row
            .3
            .map(|playlist_id| PlaylistCommit { playlist_id })
            .ok_or(MixPlannerError::InvalidIntent(
                "approved draft has no playlist",
            ));
    }
    if row.1 <= now {
        return Err(MixPlannerError::InvalidIntent("mix draft has expired"));
    }
    let draft: MixDraft = serde_json::from_str(&row.0)?;
    revalidate_selected(&tx, &draft)?;
    let ids = draft
        .tracks
        .iter()
        .map(|track| track.track_id)
        .collect::<Vec<_>>();
    let playlist_id = crate::library::playlists::create_with_tracks_in(&tx, playlist_name, &ids)?;
    tx.execute(
        "UPDATE mix_drafts SET status = 'approved', approved_playlist_id = ?1,
         idempotency_key = ?2 WHERE draft_id = ?3 AND status = 'current'",
        params![playlist_id, idempotency_key, draft_id],
    )?;
    tx.commit()?;
    Ok(PlaylistCommit { playlist_id })
}

fn revalidate_selected(conn: &Connection, draft: &MixDraft) -> Result<(), MixPlannerError> {
    for track in &draft.tracks {
        let snapshot = conn.query_row(
            "SELECT d.source_mtime, d.source_size, t.file_mtime, t.file_size, t.artist,
                    t.album, t.missing_since IS NULL AND t.removed_at IS NULL,
                    a.status = 'ready' AND a.source_mtime = t.file_mtime AND a.source_size = t.file_size
                    AND a.extractor_version = ?3 AND a.profile_version = ?4
             FROM mix_draft_tracks d JOIN tracks t ON t.id = d.track_id
             LEFT JOIN track_audio_analysis a ON a.track_id = t.id
             WHERE d.draft_id = ?1 AND d.track_id = ?2",
            params![
                draft.draft_id,
                track.track_id,
                crate::audio_analysis::CURRENT_EXTRACTOR_VERSION,
                crate::sound_profile::CURRENT_PROFILE_VERSION
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, bool>(6)?, row.get::<_, Option<bool>>(7)?.unwrap_or(false))),
        )?;
        if snapshot.0 != snapshot.2 || snapshot.1 != snapshot.3 || !snapshot.6 {
            return Err(MixPlannerError::InvalidIntent(
                "selected track changed after preview",
            ));
        }
        if draft.intent.criteria().requires_profile() && !snapshot.7 {
            return Err(MixPlannerError::InvalidIntent(
                "selected analysis changed after preview",
            ));
        }
        let in_source = match draft.intent.source() {
            MixSource::Library => true,
            MixSource::Tracks(ids) => ids.contains(&track.track_id),
            MixSource::Artist(name) => crate::library::group_key::normalize_group_key(&snapshot.4) == crate::library::group_key::normalize_group_key(name),
            MixSource::Album(name) => crate::library::group_key::normalize_group_key(&snapshot.5) == crate::library::group_key::normalize_group_key(name),
            MixSource::Playlist(id) => conn.query_row("SELECT EXISTS(SELECT 1 FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2)", params![id, track.track_id], |row| row.get(0))?,
        };
        if !in_source {
            return Err(MixPlannerError::InvalidIntent(
                "selected track left the requested source",
            ));
        }
    }
    Ok(())
}
