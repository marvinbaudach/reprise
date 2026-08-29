use super::{begin_lyrics_pass, lyrics_last_full_sweep, lyrics_watermark, LyricsPass, LyricsScope};

#[test]
fn a_completed_narrow_pass_writes_only_its_captured_start_time_to_the_watermark() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let full = begin_lyrics_pass(&db, LyricsScope::Everything);
    let full_started_at = full.started_at;

    let narrow = LyricsPass {
        started_at: 1_000,
        scope: LyricsScope::AddedSince(0),
    };
    narrow.record_completed_or_warn(&db);

    assert_eq!(lyrics_watermark(&db), Some(1_000));
    assert_eq!(lyrics_last_full_sweep(&db), Some(full_started_at));
}
