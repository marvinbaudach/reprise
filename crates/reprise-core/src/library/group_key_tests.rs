use super::{fold_groups, normalize_group_key, GroupInput};

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
fn dedup_mbid_beats_name() {
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
        GroupInput {
            raw: "Stage Name",
            mbid: Some("artist-2"),
            plays: 1,
            ms: 100,
            last_played_at: 30,
        },
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].plays, 5);
    assert_eq!(groups[0].variant_count, 2);
    assert_eq!(groups[1].plays, 1);
    assert_ne!(groups[0].key, groups[1].key);
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
