use std::fs;

use tempfile::tempdir;

use super::*;

/// Rhythmbox writes `type="ignore"` for files its own scanner ignores. They
/// are not songs and not podcasts or streams, so the prescan must not count
/// them as entries Reprise skipped.
#[test]
fn start_10_entries_rhythmbox_ignores_are_not_counted_as_skipped() {
    let dir = tempdir().unwrap();
    let xml = r#"<?xml version="1.0"?>
<rhythmdb version="2.0">
  <entry type="song"><location>file:///music/song.ogg</location><rating>4</rating></entry>
  <entry type="ignore"><title>cover.jpg</title><location>file:///music/cover.jpg</location><mtime>1700000000</mtime></entry>
  <entry type="ignore"><title>notes.txt</title><location>file:///music/notes.txt</location><mtime>1700000001</mtime></entry>
  <entry type="ignore"><title>folder.png</title><location>file:///music/folder.png</location><mtime>1700000002</mtime></entry>
  <entry type="iradio"><title>Radio</title><location>http://radio.example/stream</location></entry>
  <entry type="podcast-post"><title>Episode</title><location>file:///podcast.ogg</location></entry>
</rhythmdb>"#;
    let rhythmdb = dir.path().join("rhythmdb.xml");
    fs::write(&rhythmdb, xml).unwrap();
    let conn = crate::db::Db::open_in_memory().unwrap();

    let result = prescan_rhythmdb(
        &rhythmdb,
        &dir.path().join("playlists.xml"),
        &conn,
        Some("/music"),
    )
    .unwrap();

    assert_eq!(result.total_entries, 6);
    assert_eq!(result.song_entries, 1);
    assert_eq!(result.non_song_entries, 2);
}

/// Only podcast and radio entries are "podcasts or radio streams". Other
/// Rhythmbox-internal types, such as `import-error` or a type this version
/// does not know, are bookkeeping, not something Reprise skipped.
#[test]
fn start_10_entries_only_podcast_and_radio_types_count_as_podcasts_or_radio() {
    let dir = tempdir().unwrap();
    let xml = r#"<?xml version="1.0"?>
<rhythmdb version="2.0">
  <entry type="song"><location>file:///music/song.ogg</location></entry>
  <entry type="import-error"><title>broken.ogg</title><location>file:///music/broken.ogg</location></entry>
  <entry type="future-internal-type"><location>file:///music/other</location></entry>
  <entry><location>file:///music/untyped</location></entry>
  <entry type="iradio"><title>Radio</title><location>http://radio.example/stream</location></entry>
  <entry type="podcast-feed"><title>Feed</title><location>http://podcast.example/feed</location></entry>
  <entry type="podcast-post"><title>Episode</title><location>file:///podcast.ogg</location></entry>
  <entry type="podcast-search"><title>Search</title><location>http://podcast.example/search</location></entry>
</rhythmdb>"#;
    let rhythmdb = dir.path().join("rhythmdb.xml");
    fs::write(&rhythmdb, xml).unwrap();
    let conn = crate::db::Db::open_in_memory().unwrap();

    let result = prescan_rhythmdb(
        &rhythmdb,
        &dir.path().join("playlists.xml"),
        &conn,
        Some("/music"),
    )
    .unwrap();

    assert_eq!(result.total_entries, 8);
    assert_eq!(result.song_entries, 1);
    assert_eq!(result.non_song_entries, 4);
}
