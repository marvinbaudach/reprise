use super::*;

/// One artist tagged with a hyphen on two tracks and with an en dash on a
/// third is one group, so the scan offers to unify the odd one out. Jay Z with
/// a space is a different spelling and is left alone.
#[test]
fn doc_1a_local_grouping_folds_typographic_dashes() {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrated_connection();
    let artists = [
        (1, "Jay-Z"),
        (2, "Jay-Z"),
        (3, "Jay\u{2013}Z"),
        (4, "Jay Z"),
    ];
    for (id, artist) in artists {
        let path = fixture_copy(dir.path(), &format!("{id}.flac"));
        write_tags(&path, "Track", artist, "Album", artist, "Rock");
        insert_track(&conn, id, &path, "stale");
    }

    let scan = scan_selection(&conn, vec![1, 2, 3, 4]);

    let artist_fixes = scan
        .proposals
        .iter()
        .filter(|proposal| proposal.field == DoctorField::Artist)
        .collect::<Vec<_>>();
    assert_eq!(artist_fixes.len(), 1);
    assert_eq!(artist_fixes[0].track_id, 3);
    assert_eq!(artist_fixes[0].proposed, DoctorValue::Text("Jay-Z".into()));
    assert_eq!(artist_fixes[0].source, ProposalSource::Local);
    assert_eq!(
        artist_fixes[0].problem_class,
        ProblemClass::CasingWhitespace
    );
}
