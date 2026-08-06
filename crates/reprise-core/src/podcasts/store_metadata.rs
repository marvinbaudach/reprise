//! Metadata learned from YouTube's existing full extraction.

use rusqlite::params;

use crate::db::Db;

pub fn save_youtube_resolution(
    db: &Db,
    episode_id: i64,
    duration_secs: Option<i64>,
    media_category: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "UPDATE podcast_episodes
         SET duration_secs = CASE
               WHEN duration_secs IS NULL AND ?2 > 0 THEN ?2
               ELSE duration_secs
             END,
             media_category = COALESCE(NULLIF(?3, ''), media_category)
         WHERE id = ?1
           AND EXISTS (
             SELECT 1 FROM podcast_subscriptions s
             WHERE s.id = podcast_episodes.subscription_id
               AND s.kind = 'youtube'
           )",
        params![episode_id, duration_secs, media_category],
    )?;
    transaction.commit()
}
