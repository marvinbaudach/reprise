//! `music_similar_tracks` and `music_sound_profile`: ranking, the two honest
//! "nothing to compare yet" states, and the per-request option overrides.
//!
//! The fixtures store real [`SoundFeatures`] through the core facade rather
//! than a hand-written blob, so the tools rank the same bytes the app's
//! backfill writes.

mod common;

use common::{assert_no_leaks, structured_ok, McpClient};
use reprise_core::sound_features::SoundFeatures;
use reprise_core::sound_rhythm::RhythmFeatures;
use reprise_core::spectrogram::SPECTROGRAM_BAND_COUNT;
use rusqlite::params;
use serde_json::{json, Value};
use tempfile::TempDir;

/// One track to seed, with the album artist the same-album exclusion needs.
struct SoundTrack {
    title: &'static str,
    artist: &'static str,
    album: &'static str,
    /// The band that carries all the energy — tracks sharing a band sound
    /// alike, tracks on distant bands do not.
    band: usize,
    /// Whether a derived profile is stored for this track.
    profiled: bool,
}

/// A profile with all its energy in one band, so distance is a function of
/// nothing but that band — the same shape `sound_neighbours_tests` uses.
fn features(band: usize) -> SoundFeatures {
    let mut band_mean = [0.0; SPECTROGRAM_BAND_COUNT];
    band_mean[band] = 1.0;
    SoundFeatures {
        band_mean,
        centroid_mean: band as f32,
        centroid_var: 1.0,
        frame_crest_db: band as f32,
        rhythm: RhythmFeatures::still(),
        tempo: None,
    }
}

/// Seeds a migrated library and returns the assigned ids in order. Fixture SQL
/// is explicitly permitted under `tests/` (`scripts/check-architecture.sh`);
/// the profiles themselves go in through the core facade.
fn seed_sound_library(path: &std::path::Path, tracks: &[SoundTrack]) -> Vec<i64> {
    let db = reprise_core::db::Db::open_migrated(Some(path)).expect("open+migrate fixture db");
    let conn = common::fixture_connection(path);
    let mut ids = Vec::with_capacity(tracks.len());
    for track in tracks {
        conn.execute(
            "INSERT INTO tracks \
             (path, title, artist, album, album_artist, genre, duration_ms, added_at) \
             VALUES (?1, ?2, ?3, ?4, ?3, 'Test', 180000, 0)",
            params![
                format!("/music/{}/{}.flac", track.artist, track.title),
                track.title,
                track.artist,
                track.album,
            ],
        )
        .expect("insert fixture track");
        let id = conn.last_insert_rowid();
        if track.profiled {
            reprise_core::db::set_track_sound_features(&db, id, &features(track.band))
                .expect("store fixture sound profile");
        }
        ids.push(id);
    }
    ids
}

/// Anchor plus four neighbours: one on the anchor's own album, one by the same
/// artist elsewhere, and two by other people at increasing distance.
fn ranked_library(dir: &TempDir) -> (std::path::PathBuf, Vec<i64>) {
    let path = dir.path().join("reprise.db");
    let ids = seed_sound_library(
        &path,
        &[
            SoundTrack {
                title: "Anchor",
                artist: "Anchor Artist",
                album: "Anchor Album",
                band: 0,
                profiled: true,
            },
            SoundTrack {
                title: "Same Album",
                artist: "Anchor Artist",
                album: "Anchor Album",
                band: 0,
                profiled: true,
            },
            SoundTrack {
                title: "Same Artist",
                artist: "Anchor Artist",
                album: "Side Project",
                band: 0,
                profiled: true,
            },
            SoundTrack {
                title: "Near Stranger",
                artist: "Near Stranger",
                album: "Elsewhere",
                band: 1,
                profiled: true,
            },
            SoundTrack {
                title: "Far Stranger",
                artist: "Far Stranger",
                album: "Far Away",
                band: 9,
                profiled: true,
            },
        ],
    );
    (path, ids)
}

fn match_titles(result: &Value) -> Vec<String> {
    result["matches"]
        .as_array()
        .expect("matches array")
        .iter()
        .map(|row| row["title"].as_str().expect("match title").to_owned())
        .collect()
}

#[test]
fn similar_tracks_ranks_nearest_first_over_the_whole_profiled_library() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = ranked_library(&dir);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_similar_tracks", json!({ "track_id": ids[0] }));
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
    let result = structured_ok(&response);

    assert_eq!(result["status"], "ranked");
    assert_eq!(result["profiles_ready"], 5);
    assert_eq!(result["library_tracks"], 5);
    // Four other profiles carry the ranks, even though the same-album track is
    // then filtered out of the list (SIM-3).
    assert_eq!(result["compared_tracks"], 4);
    assert_eq!(
        match_titles(&result),
        ["Same Artist", "Near Stranger", "Far Stranger"],
        "the same-album track is excluded by default, the rest rank nearest-first"
    );
    let distances: Vec<f64> = result["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["distance"].as_f64().expect("distance"))
        .collect();
    assert!(
        distances.windows(2).all(|pair| pair[0] <= pair[1]),
        "distances must not decrease down the list: {distances:?}"
    );
    let nearest = &result["matches"][0];
    assert_eq!(nearest["track_id"], ids[2]);
    assert_eq!(nearest["artist"], "Anchor Artist");
    assert_eq!(nearest["album"], "Side Project");
    assert!(nearest["percentile"].as_f64().expect("percentile") > 0.0);
    assert!(nearest.get("path").is_none(), "a match must carry no path");
    assert!(result["readiness_hint"]
        .as_str()
        .expect("readiness hint")
        .contains("5 of 5"));
}

#[test]
fn similar_tracks_reports_a_track_that_has_no_profile_of_its_own() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    let ids = seed_sound_library(
        &path,
        &[
            SoundTrack {
                title: "Profiled",
                artist: "Someone",
                album: "An Album",
                band: 0,
                profiled: true,
            },
            SoundTrack {
                title: "Unprofiled",
                artist: "Someone Else",
                album: "Another Album",
                band: 0,
                profiled: false,
            },
        ],
    );
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_similar_tracks", json!({ "track_id": ids[1] }));
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
    let result = structured_ok(&response);

    assert_eq!(result["status"], "track_not_analysed");
    assert_eq!(result["matches"].as_array().unwrap().len(), 0);
    assert_eq!(result["profiles_ready"], 1);
    assert_eq!(result["library_tracks"], 2);
    assert_eq!(result["compared_tracks"], 0);
    assert!(result["readiness_hint"]
        .as_str()
        .expect("readiness hint")
        .contains("no sound profile yet"));

    // The same track through the profile tool says the same thing.
    let profile =
        structured_ok(&client.call_tool("music_sound_profile", json!({ "track_id": ids[1] })));
    assert_eq!(profile["status"], "track_not_analysed");
    assert_eq!(profile["axes"], Value::Null);
}

#[test]
fn similar_tracks_reports_a_library_that_has_no_profiles_at_all() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    let ids = seed_sound_library(
        &path,
        &[
            SoundTrack {
                title: "One",
                artist: "Someone",
                album: "An Album",
                band: 0,
                profiled: false,
            },
            SoundTrack {
                title: "Two",
                artist: "Someone",
                album: "An Album",
                band: 0,
                profiled: false,
            },
        ],
    );
    let mut client = McpClient::start(&path);

    let result =
        structured_ok(&client.call_tool("music_similar_tracks", json!({ "track_id": ids[0] })));
    assert_eq!(
        result["status"], "no_profiles_yet",
        "an unanalysed library is a different answer from an unanalysed track"
    );
    assert_eq!(result["profiles_ready"], 0);
    assert_eq!(result["library_tracks"], 2);
    assert_eq!(result["matches"].as_array().unwrap().len(), 0);
    // The module is off by default, which is why nothing is being derived.
    assert_eq!(result["module_enabled"], false);
    let hint = result["readiness_hint"].as_str().expect("readiness hint");
    assert!(
        hint.contains("No sound profile has been derived yet"),
        "{hint}"
    );
    assert!(
        hint.contains("Sound Similarity module is switched off"),
        "{hint}"
    );

    let profile =
        structured_ok(&client.call_tool("music_sound_profile", json!({ "track_id": ids[0] })));
    assert_eq!(profile["status"], "no_profiles_yet");
    assert_eq!(profile["axes"], Value::Null);
}

#[test]
fn similar_tracks_applies_the_requested_options_over_the_shipped_defaults() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = ranked_library(&dir);
    let mut client = McpClient::start(&path);

    // Keeping the same album back in puts it at the top of the list.
    let kept = structured_ok(&client.call_tool(
        "music_similar_tracks",
        json!({ "track_id": ids[0], "exclude_same_album": false }),
    ));
    assert_eq!(kept["exclude_same_album"], false);
    assert!(match_titles(&kept).contains(&"Same Album".to_owned()));

    // Excluding the artist drops both of that artist's tracks.
    let no_artist = structured_ok(&client.call_tool(
        "music_similar_tracks",
        json!({ "track_id": ids[0], "exclude_same_artist": true }),
    ));
    assert_eq!(no_artist["exclude_same_artist"], true);
    assert_eq!(
        match_titles(&no_artist),
        ["Near Stranger", "Far Stranger"],
        "no track by the anchor's own artist may remain"
    );

    // The limit caps the list without changing the population behind it.
    let capped = structured_ok(&client.call_tool(
        "music_similar_tracks",
        json!({ "track_id": ids[0], "limit": 1 }),
    ));
    assert_eq!(capped["limit"], 1);
    assert_eq!(capped["matches"].as_array().unwrap().len(), 1);
    assert_eq!(capped["compared_tracks"], 4);

    // A weighting is echoed back, and an unknown one is a caller-visible error
    // rather than a silent fall back to the default ranking.
    let weighted = structured_ok(&client.call_tool(
        "music_similar_tracks",
        json!({ "track_id": ids[0], "weighting": "dynamics" }),
    ));
    assert_eq!(weighted["weighting"], "dynamics");

    let unknown = client.call_tool(
        "music_similar_tracks",
        json!({ "track_id": ids[0], "weighting": "loudness" }),
    );
    let error = unknown["result"].as_object().expect("tool result");
    assert_eq!(error["isError"], Value::Bool(true));
    assert!(error["content"][0]["text"]
        .as_str()
        .expect("error text")
        .contains("unknown weighting"));
}

#[test]
fn sound_profile_returns_the_three_axes_and_the_file_line() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = ranked_library(&dir);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_sound_profile", json!({ "track_id": ids[4] }));
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
    let result = structured_ok(&response);

    assert_eq!(result["status"], "ready");
    let axes = &result["axes"];
    // The seeded profiles rise band by band, so the last one sits at the top
    // of both library-wide axes.
    assert_eq!(axes["timbre"].as_f64().expect("timbre"), 100.0);
    assert_eq!(axes["dynamics"].as_f64().expect("dynamics"), 100.0);
    assert_eq!(axes["tempo"], Value::Null, "no stable tempo was estimated");

    let file = &result["file"];
    assert_eq!(file["format"], "FLAC");
    assert!(file.get("path").is_none(), "the file line carries no path");
    assert_eq!(file["occupied_upper_hz"], Value::Null);
    assert_eq!(result["profiles_ready"], 5);
}

#[test]
fn sound_tools_reject_a_track_that_is_not_in_the_library() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = ranked_library(&dir);
    let mut client = McpClient::start(&path);

    for tool in ["music_similar_tracks", "music_sound_profile"] {
        let response = client.call_tool(tool, json!({ "track_id": 999_999 }));
        let result = response["result"].as_object().expect("tool result");
        assert_eq!(result["isError"], Value::Bool(true), "{tool} must refuse");
        assert!(result["content"][0]["text"]
            .as_str()
            .expect("error text")
            .contains("not present in the library"));
    }
}
