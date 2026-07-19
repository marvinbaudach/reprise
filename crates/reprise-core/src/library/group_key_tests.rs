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
    ];

    for fixture in fixtures {
        let once = normalize_group_key(fixture);
        assert_eq!(normalize_group_key(&once), once, "fixture: {fixture:?}");
    }
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
