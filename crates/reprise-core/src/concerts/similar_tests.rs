use std::cell::RefCell;

use rusqlite::{params, Connection};

use super::candidates::seed_artists;
use super::config::SimilarConfig;
use super::similar::{similar_candidates, SimilarFetch};
use super::{
    lastfm_similar_url, listenbrainz_similar_url, parse_lastfm_similar, parse_listenbrainz_similar,
    ProviderError, SimilarArtist, LB_SIMILAR_ALGORITHM,
};

fn conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn seed_play(conn: &Connection, artist: &str, mbid: Option<&str>, played_at: i64) {
    conn.execute(
        "INSERT INTO listen_events (
           track_id, played_at, ms_played, artist, artist_mbid
         ) VALUES (1, ?1, 1, ?2, ?3)",
        params![played_at, artist, mbid],
    )
    .unwrap();
}

#[test]
fn listenbrainz_url_uses_the_current_plural_artist_mbids_contract() {
    let url = listenbrainz_similar_url("abc/123");
    assert!(url.contains("artist_mbids=abc%2F123"));
    assert!(url.contains(&format!("algorithm={LB_SIMILAR_ALGORITHM}")));
}

#[test]
fn listenbrainz_parser_accepts_current_rows_and_a_results_wrapper() {
    let body = r#"{"results":[
      {"artist_mbid":"two","name":"Second","score":7},
      {"artist_mbid":"one","name":"First","score":9.5},
      {"artist_mbid":"seed","name":"Seed","score":null}
    ]}"#;
    let parsed = parse_listenbrainz_similar(body).unwrap();
    assert_eq!(
        parsed,
        vec![
            SimilarArtist {
                name: "First".into(),
                mbid: Some("one".into()),
                score: 9.5,
            },
            SimilarArtist {
                name: "Second".into(),
                mbid: Some("two".into()),
                score: 7.0,
            },
        ]
    );
    assert_eq!(parse_listenbrainz_similar("[]").unwrap(), Vec::new());
    assert!(matches!(
        parse_listenbrainz_similar("{broken"),
        Err(ProviderError::Parse)
    ));
}

#[test]
fn lastfm_parser_accepts_string_and_number_matches_and_applies_threshold() {
    let body = r#"{"similarartists":{"artist":[
      {"name":"String Match","mbid":"","match":"0.8"},
      {"name":"Number Match","mbid":"number","match":0.4},
      {"name":"Too Low","match":"0.399"}
    ]}}"#;
    let parsed = parse_lastfm_similar(body).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name, "String Match");
    assert_eq!(parsed[0].mbid, None);
    assert_eq!(parsed[1].score, 0.4);
    assert!(matches!(
        parse_lastfm_similar("not json"),
        Err(ProviderError::Parse)
    ));
}

#[test]
fn lastfm_url_encodes_artist_and_never_exposes_it_as_a_path() {
    let url = lastfm_similar_url("Earth, Wind & Fire", "secret", 10);
    assert!(url.contains("artist=Earth%2C+Wind+%26+Fire"));
    assert!(url.contains("api_key=secret"));
    assert!(url.contains("limit=10"));
}

#[derive(Default)]
struct FakeFetch {
    calls: RefCell<Vec<String>>,
}

impl SimilarFetch for FakeFetch {
    fn listenbrainz(&self, mbid: &str) -> Result<Vec<SimilarArtist>, ProviderError> {
        self.calls.borrow_mut().push(format!("lb:{mbid}"));
        Ok(vec![
            SimilarArtist {
                name: "Already Local".into(),
                mbid: Some("local".into()),
                score: 1.0,
            },
            SimilarArtist {
                name: "Shared Similar".into(),
                mbid: Some("shared".into()),
                score: 0.9,
            },
            SimilarArtist {
                name: "Lower".into(),
                mbid: Some("lower".into()),
                score: 0.5,
            },
        ])
    }

    fn lastfm(
        &self,
        name: &str,
        _api_key: &str,
        _limit: usize,
    ) -> Result<Vec<SimilarArtist>, ProviderError> {
        self.calls.borrow_mut().push(format!("lastfm:{name}"));
        Ok(vec![
            SimilarArtist {
                name: "Shared Similar".into(),
                mbid: None,
                score: 0.95,
            },
            SimilarArtist {
                name: "Name Fallback".into(),
                mbid: None,
                score: 0.8,
            },
        ])
    }
}

#[test]
fn candidate_selection_uses_mbid_source_then_name_fallback_and_deduplicates() {
    let conn = conn();
    seed_play(&conn, "MBID Seed", Some("seed-mbid"), 100);
    seed_play(&conn, "Name Seed", None, 100);
    seed_play(&conn, "Already Local", Some("local"), 100);
    let seeds = seed_artists(&conn, 0, 5).unwrap();
    let fetch = FakeFetch::default();

    let candidates = similar_candidates(
        &conn,
        &seeds,
        &seeds,
        &fetch,
        SimilarConfig {
            enabled: true,
            count: 2,
        },
        Some("bundled-key"),
    )
    .unwrap();

    let calls = fetch.calls.borrow();
    assert!(calls.contains(&"lb:seed-mbid".to_owned()));
    assert!(calls.contains(&"lastfm:Name Seed".to_owned()));
    assert!(!calls.contains(&"lastfm:Already Local".to_owned()));
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Shared Similar", "Name Fallback", "Lower"]
    );
    assert_eq!(candidates[0].similar_to.as_deref(), Some("Name Seed"));
    assert!(candidates.iter().all(|candidate| candidate.is_similar));
}

#[test]
fn seeds_without_mbid_are_skipped_when_no_bundled_key_exists() {
    let conn = conn();
    seed_play(&conn, "Name Seed", None, 100);
    let seeds = seed_artists(&conn, 0, 5).unwrap();
    let fetch = FakeFetch::default();

    let candidates = similar_candidates(
        &conn,
        &seeds,
        &seeds,
        &fetch,
        SimilarConfig {
            enabled: true,
            count: 10,
        },
        None,
    )
    .unwrap();

    assert!(candidates.is_empty());
    assert!(fetch.calls.borrow().is_empty());
}

struct ManyFetch;

impl SimilarFetch for ManyFetch {
    fn listenbrainz(&self, mbid: &str) -> Result<Vec<SimilarArtist>, ProviderError> {
        Ok((0..30)
            .map(|index| SimilarArtist {
                name: format!("{mbid} Similar {index:02}"),
                mbid: None,
                score: f64::from(30 - index),
            })
            .collect())
    }

    fn lastfm(
        &self,
        _name: &str,
        _api_key: &str,
        _limit: usize,
    ) -> Result<Vec<SimilarArtist>, ProviderError> {
        unreachable!("every seed has an MBID")
    }
}

#[test]
fn candidate_selection_caps_each_seed_at_twenty_five_and_the_run_at_fifty() {
    let conn = conn();
    for index in 0..5 {
        seed_play(
            &conn,
            &format!("Seed {index}"),
            Some(&format!("seed-{index}")),
            100,
        );
    }
    let seeds = seed_artists(&conn, 0, 5).unwrap();

    let candidates = similar_candidates(
        &conn,
        &seeds,
        &seeds,
        &ManyFetch,
        SimilarConfig {
            enabled: true,
            count: 25,
        },
        None,
    )
    .unwrap();

    assert_eq!(candidates.len(), 50);
    assert!(candidates
        .iter()
        .all(|candidate| !candidate.name.ends_with("Similar 25")));
}
