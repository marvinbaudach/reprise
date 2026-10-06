use super::{fold_groups, normalize_group_key, GroupInput, KeyResolver};

#[test]
fn dedup_casing_whitespace_merges_one_artist() {
    let groups = fold_groups(&[
        GroupInput {
            raw: "Lorna Shore",
            mbid: None,
            plays: 5,
            ms: 500,
            last_played_at: 30,
        },
        GroupInput {
            raw: "lorna shore ",
            mbid: None,
            plays: 3,
            ms: 300,
            last_played_at: 20,
        },
        GroupInput {
            raw: "Lorna\tShore",
            mbid: None,
            plays: 2,
            ms: 200,
            last_played_at: 10,
        },
    ]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].label, "Lorna Shore");
    assert_eq!(groups[0].plays, 10);
    assert_eq!(groups[0].ms, 1_000);
    assert_eq!(groups[0].variant_count, 3);
}

#[test]
fn dedup_mbid_merges_unrelated_spellings() {
    let groups = fold_groups(&[
        GroupInput {
            raw: "Stage Name",
            mbid: Some("artist-1"),
            plays: 3,
            ms: 300,
            last_played_at: 10,
        },
        GroupInput {
            raw: "Legal Name",
            mbid: Some("artist-1"),
            plays: 2,
            ms: 200,
            last_played_at: 20,
        },
    ]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].key, "mbid:artist-1");
    assert_eq!(groups[0].plays, 5);
    assert_eq!(groups[0].variant_count, 2);
}

/// The MBID must never split what the name fold already merged: a single
/// spelling that carries an MBID stays in the group of its unlabelled twin.
#[test]
fn dedup_mbid_never_splits_one_name_group() {
    let groups = fold_groups(&[
        GroupInput {
            raw: "Sigur R\u{00f3}s",
            mbid: Some("sigur-ros-1"),
            plays: 3,
            ms: 300,
            last_played_at: 10,
        },
        GroupInput {
            raw: "Sigur Ros",
            mbid: None,
            plays: 2,
            ms: 200,
            last_played_at: 20,
        },
    ]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].key, "mbid:sigur-ros-1");
    assert_eq!(groups[0].plays, 5);
    assert_eq!(groups[0].variant_count, 2);
}

/// Several MBIDs under one name group resolve to the most-played one, with a
/// lexicographic tiebreak, so the key never depends on row order.
#[test]
fn dedup_competing_mbids_resolve_by_plays_then_alphabetically() {
    let dominant = fold_groups(&[
        GroupInput {
            raw: "Stage Name",
            mbid: Some("z-artist"),
            plays: 3,
            ms: 300,
            last_played_at: 10,
        },
        GroupInput {
            raw: "stage name",
            mbid: Some("a-artist"),
            plays: 1,
            ms: 100,
            last_played_at: 30,
        },
    ]);
    assert_eq!(dominant.len(), 1);
    assert_eq!(dominant[0].key, "mbid:z-artist");
    assert_eq!(dominant[0].plays, 4);

    let tied = fold_groups(&[
        GroupInput {
            raw: "Stage Name",
            mbid: Some("z-artist"),
            plays: 2,
            ms: 200,
            last_played_at: 10,
        },
        GroupInput {
            raw: "stage name",
            mbid: Some("a-artist"),
            plays: 2,
            ms: 200,
            last_played_at: 30,
        },
    ]);
    assert_eq!(tied.len(), 1);
    assert_eq!(tied[0].key, "mbid:a-artist");
}

#[test]
fn key_resolver_falls_back_to_the_name_key_for_unknown_spellings() {
    let resolver = KeyResolver::build([GroupInput {
        raw: "Known",
        mbid: Some("known-1"),
        plays: 1,
        ms: 1,
        last_played_at: 1,
    }]);

    assert_eq!(resolver.key_for(" known "), "mbid:known-1");
    assert_eq!(resolver.key_for("Stranger"), "name:stranger");
}

#[test]
fn key_resolver_names_for_key_covers_both_key_shapes() {
    let resolver = KeyResolver::build([
        GroupInput {
            raw: "Alias",
            mbid: Some("shared-1"),
            plays: 1,
            ms: 1,
            last_played_at: 1,
        },
        GroupInput {
            raw: "Other Alias",
            mbid: Some("shared-1"),
            plays: 1,
            ms: 1,
            last_played_at: 1,
        },
    ]);

    let by_mbid = resolver.names_for_key("mbid:shared-1");
    assert!(by_mbid.contains("alias"));
    assert!(by_mbid.contains("other alias"));
    assert_eq!(
        resolver.names_for_key("name:alias"),
        ["alias".to_string()].into_iter().collect()
    );
    assert!(resolver.names_for_key("mbid:absent").is_empty());
}

#[test]
fn dedup_no_fuzzy() {
    let groups = fold_groups(&[
        input("Lorna Shore", 4),
        input("Lorna Shore Band", 3),
        input("Weezer", 2),
        input("Weezer (Blue Album)", 1),
    ]);

    assert_eq!(groups.len(), 4);
}

#[test]
fn dedup_folds_diacritics_via_nfkd() {
    let groups = fold_groups(&[
        input("Björk", 3),
        input("Bjo\u{308}rk", 2),
        input("bjork", 1),
    ]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].plays, 6);
    assert_eq!(groups[0].variant_count, 3);
}

#[test]
fn dedup_label_tiebreak_is_total_order() {
    let first = GroupInput {
        raw: "same artist",
        mbid: None,
        plays: 2,
        ms: 100,
        last_played_at: 50,
    };
    let second = GroupInput {
        raw: "Same Artist",
        mbid: None,
        plays: 2,
        ms: 100,
        last_played_at: 50,
    };

    let forward = fold_groups(&[first, second]);
    let reverse = fold_groups(&[second, first]);

    assert_eq!(forward, reverse);
    assert_eq!(forward[0].label, "Same Artist");
    assert_eq!(fold_groups(&[first, second]), forward);
}

#[test]
fn normalize_group_key_is_idempotent() {
    let fixtures = [
        "",
        "   ",
        "Lorna\t Shore ",
        "Björk",
        "Bjo\u{308}rk",
        "\u{308}",
        "  ÉLAN   VITAL ",
        "Jay\u{2010}Z",
        "Guns N\u{b4} Roses",
        "\u{2033}Live\u{2033}",
        "\u{2034}",
        "A \u{b4}",
    ];

    for fixture in fixtures {
        let once = normalize_group_key(fixture);
        assert_eq!(normalize_group_key(&once), once, "fixture: {fixture:?}");
    }
}

/// Every character the fold maps, against the ASCII form it must land on. The
/// cover matcher folds the same table, so a change here is a change there.
#[test]
fn normalize_group_key_folds_typographic_dashes_apostrophes_and_quotes() {
    let dashes = [
        '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}',
        '\u{fe58}', '\u{fe63}', '\u{ff0d}',
    ];
    for dash in dashes {
        assert_eq!(
            normalize_group_key(&format!("Jay{dash}Z")),
            "jay-z",
            "dash U+{:04X}",
            u32::from(dash)
        );
    }
    let apostrophes = [
        '\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}', '\u{2032}', '\u{2bc}', '`', '\u{b4}',
    ];
    for apostrophe in apostrophes {
        assert_eq!(
            normalize_group_key(&format!("Guns N{apostrophe} Roses")),
            "guns n' roses",
            "apostrophe U+{:04X}",
            u32::from(apostrophe)
        );
    }
    for quote in ['\u{201c}', '\u{201d}', '\u{201e}', '\u{201f}', '\u{2033}'] {
        assert_eq!(
            normalize_group_key(&format!("{quote}Live{quote}")),
            "\"live\"",
            "quote U+{:04X}",
            u32::from(quote)
        );
    }
}

#[test]
fn dedup_folds_typographic_punctuation() {
    let groups = fold_groups(&[
        input("Rock \u{2013} Live", 3),
        input("Rock - Live", 2),
        input("Guns N\u{2019} Roses", 2),
        input("Guns N' Roses", 1),
        input("\u{201c}Heroes\u{201d}", 2),
        input("\"Heroes\"", 1),
    ]);

    assert_eq!(groups.len(), 3);
    assert!(groups.iter().all(|group| group.variant_count == 2));
}

/// Folding is exactly the table: a dash is not a space, an apostrophe is not
/// a double quote, and the digits around a dash still have to match.
#[test]
fn dedup_does_not_fold_what_is_merely_similar_to_typographic_punctuation() {
    let groups = fold_groups(&[
        input("Jay-Z", 1),
        input("Jay Z", 1),
        input("JayZ", 1),
        input("Guns N' Roses", 1),
        input("Guns N Roses", 1),
        input("Say \"Hi\"", 1),
        input("Say 'Hi'", 1),
        input("Selected Ambient Works 85\u{2013}92", 1),
        input("Selected Ambient Works 85\u{2013}93", 1),
    ]);

    assert_eq!(groups.len(), 9);
}

fn input(raw: &'static str, plays: i64) -> GroupInput<'static> {
    GroupInput {
        raw,
        mbid: None,
        plays,
        ms: plays * 100,
        last_played_at: plays,
    }
}
