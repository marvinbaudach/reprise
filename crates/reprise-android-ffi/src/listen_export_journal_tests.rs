use reprise_core::device_sync::listen_report::{ListenReport, ListenReportAcknowledgement};

use crate::listen_export_journal::{
    prepare_report, record_listen, record_rating, FILE_NAME as EXPORT_FILE_NAME,
};

#[test]
fn plays_and_ratings_share_a_separate_export_sequence_and_encode_as_rpt_back() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");

    let play_sequence = record_listen(
        &database_path,
        "Artist/Album/01 Train.opus",
        1_777_777_001,
        123_456,
    )
    .unwrap();
    let rating_sequence = record_rating(
        &database_path,
        "Artist/Album/01 Train.opus",
        5,
        1_777_777_002,
    )
    .unwrap();

    let bytes = prepare_report(&database_path, None).unwrap();
    let report = ListenReport::decode(&bytes).unwrap();
    assert_eq!((play_sequence, rating_sequence), (1, 2));
    assert_eq!(report.listens.len(), 1);
    assert_eq!(report.listens[0].sequence, 1);
    assert_eq!(report.listens[0].device_path, "Artist/Album/01 Train.opus");
    assert_eq!(report.listens[0].played_at, 1_777_777_001);
    assert_eq!(report.listens[0].ms_played, 123_456);
    assert_eq!(report.ratings.len(), 1);
    assert_eq!(report.ratings[0].sequence, 2);
    assert_eq!(report.ratings[0].rating, 5);
    assert_eq!(report.ratings[0].rated_at, 1_777_777_002);
    assert_ne!(EXPORT_FILE_NAME, crate::play_journal::FILE_NAME);
    assert!(directory.path().join(EXPORT_FILE_NAME).is_file());
}

#[test]
fn only_a_valid_acknowledgement_prunes_and_the_export_sequence_never_restarts() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    record_listen(&database_path, "one.opus", 10, 20).unwrap();
    record_rating(&database_path, "two.opus", 4, 30).unwrap();

    let missing = ListenReport::decode(&prepare_report(&database_path, None).unwrap()).unwrap();
    assert_eq!(missing.highest_sequence(), Some(2));
    let truncated =
        ListenReport::decode(&prepare_report(&database_path, Some(b"RPT-ACKN")).unwrap()).unwrap();
    assert_eq!(
        truncated, missing,
        "a truncated acknowledgement acknowledges nothing"
    );

    let acknowledged = ListenReportAcknowledgement::new(1).encode();
    let remaining =
        ListenReport::decode(&prepare_report(&database_path, Some(&acknowledged)).unwrap())
            .unwrap();
    assert!(remaining.listens.is_empty());
    assert_eq!(remaining.ratings[0].sequence, 2);
    assert_eq!(
        ListenReport::decode(&prepare_report(&database_path, None).unwrap()).unwrap(),
        remaining,
        "a later missing acknowledgement must not discard the remaining entry"
    );

    let all_acknowledged = ListenReportAcknowledgement::new(2).encode();
    let empty =
        ListenReport::decode(&prepare_report(&database_path, Some(&all_acknowledged)).unwrap())
            .unwrap();
    assert_eq!(empty, ListenReport::default());
    assert_eq!(
        record_listen(&database_path, "three.opus", 40, 50).unwrap(),
        3
    );
}

#[test]
fn rating_after_reinstall_survives_the_previous_install_acknowledgement() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let previous_install_acknowledgement = ListenReportAcknowledgement::new(1).encode();
    assert_eq!(
        ListenReport::decode(
            &prepare_report(
                &database_path,
                Some(previous_install_acknowledgement.as_slice()),
            )
            .unwrap(),
        )
        .unwrap(),
        ListenReport::default(),
    );

    record_rating(
        &database_path,
        "Artist/Album/01 Train.opus",
        5,
        1_777_777_002,
    )
    .unwrap();
    let before_acknowledgement =
        ListenReport::decode(&prepare_report(&database_path, None).unwrap()).unwrap();
    assert_eq!(before_acknowledgement.ratings.len(), 1);
    assert_eq!(before_acknowledgement.ratings[0].sequence, 2);
    assert_eq!(
        before_acknowledgement.ratings[0].device_path,
        "Artist/Album/01 Train.opus",
    );

    let after_acknowledgement = ListenReport::decode(
        &prepare_report(
            &database_path,
            Some(previous_install_acknowledgement.as_slice()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(after_acknowledgement.ratings.len(), 1);
    assert_eq!(after_acknowledgement.ratings[0].sequence, 2);
    assert_eq!(
        after_acknowledgement.ratings[0].device_path,
        "Artist/Album/01 Train.opus",
    );
}

#[test]
fn an_exhausted_previous_install_sequence_does_not_break_empty_report_publishing() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let exhausted_acknowledgement = ListenReportAcknowledgement::new(u64::MAX).encode();

    assert_eq!(
        ListenReport::decode(
            &prepare_report(&database_path, Some(exhausted_acknowledgement.as_slice())).unwrap(),
        )
        .unwrap(),
        ListenReport::default(),
    );
}
