use super::*;

fn release(artist: &str, album: &str) -> String {
    serde_json::json!({
        "releases": [{
            "id": "11111111-1111-1111-1111-111111111111",
            "score": 100,
            "title": album,
            "artist-credit": [{"name": artist}],
        }]
    })
    .to_string()
}

fn assert_match(result: ReleaseSearchResult) {
    let ReleaseSearchResult::Match(ids) = result else {
        panic!("expected a matching release");
    };
    assert_eq!(ids, ["11111111-1111-1111-1111-111111111111"]);
}

#[test]
fn en_dash_and_hyphen_release_titles_match_in_both_directions() {
    assert_match(parse_best_release(
        &release("Aphex Twin", "Selected Ambient Works 85–92"),
        "Aphex Twin",
        "Selected Ambient Works 85-92",
    ));
    assert_match(parse_best_release(
        &release("Aphex Twin", "Selected Ambient Works 85-92"),
        "Aphex Twin",
        "Selected Ambient Works 85–92",
    ));
}

#[test]
fn typographic_variants_match_for_album_and_artist_names() {
    for (plain, variant) in [
        ("Name 85-92", "Name 85‐92"),
        ("Name 85-92", "Name 85‑92"),
        ("Name 85-92", "Name 85–92"),
        ("Name 85-92", "Name 85—92"),
        ("Name 85-92", "Name 85−92"),
        ("Artist's", "Artist’s"),
        ("Artist's", "Artist‘s"),
        ("Artist's", "Artistʼs"),
        ("Artist's", "Artist`s"),
        ("Artist's", "Artist´s"),
        ("\"Opening", "“Opening"),
        ("Closing\"", "Closing”"),
        ("Wait...", "Wait…"),
    ] {
        assert_match(parse_best_release(
            &release("Plain Artist", variant),
            "Plain Artist",
            plain,
        ));
        assert_match(parse_best_release(
            &release(variant, "Plain Album"),
            plain,
            "Plain Album",
        ));
    }
    assert_eq!(
        parse_best_release(
            &release("Aphex Twin", "Selected Ambient Works 85-93"),
            "Aphex Twin",
            "Selected Ambient Works 85-92",
        ),
        ReleaseSearchResult::NoMatch
    );
    assert_eq!(
        parse_best_release(
            &release("Other Artist", "Selected Ambient Works 85–92"),
            "Aphex Twin",
            "Selected Ambient Works 85-92",
        ),
        ReleaseSearchResult::NoMatch
    );
}

#[test]
fn typographic_release_match_downloads_without_a_negative_marker() {
    let cache_root = tempfile::tempdir().unwrap();
    let dir = downloaded_dir_in(cache_root.path());
    let artist = "Aphex Twin";
    let album = "Selected Ambient Works 85-92";
    let key = album_key(artist, album);
    let body = release(artist, "Selected Ambient Works 85–92");

    let outcome = fetch_and_cache_with_in(
        &dir,
        artist,
        album,
        None,
        &[],
        &mut |_| Some(body.clone()),
        &mut |_| CaaFetchResult::Found(b"cover".to_vec(), "jpg"),
    );

    assert!(matches!(outcome, CoverFetchOutcome::Downloaded(_)));
    assert!(!negative_marker_path_in(&dir, &key).exists());
}
