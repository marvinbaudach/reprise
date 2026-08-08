use super::*;

fn sample_report() -> ListenReport {
    ListenReport::new(
        vec![ListenEntry {
            sequence: 7,
            device_path: "Artist/Album/01 Song.opus".into(),
            played_at: 1_754_600_001,
            ms_played: 183_421,
        }],
        vec![RatingEntry {
            sequence: u64::MAX,
            device_path: "Artist/Album/02 Next.opus".into(),
            rating: 5,
            rated_at: 1_754_600_002,
        }],
    )
}

#[test]
fn report_round_trips_both_counted_sections_and_full_width_sequences() {
    let report = sample_report();

    let encoded = report.encode().unwrap();

    assert_eq!(&encoded[..8], b"RPT-BACK");
    assert_eq!(u16::from_le_bytes([encoded[8], encoded[9]]), FORMAT_VERSION);
    assert_eq!(ListenReport::decode(&encoded).unwrap(), report);
}

#[test]
fn acknowledgement_round_trips_the_full_width_high_water_mark() {
    let acknowledgement = ListenReportAcknowledgement::new(u64::MAX);

    let encoded = acknowledgement.encode();

    assert_eq!(
        ListenReportAcknowledgement::decode(&encoded).unwrap(),
        acknowledgement
    );
}

#[test]
fn acknowledgement_decode_rejects_wrong_magic_version_and_truncation() {
    let encoded = ListenReportAcknowledgement::new(41).encode();
    let mut wrong_magic = encoded.clone();
    wrong_magic[0] ^= 0xff;
    let mut future = encoded.clone();
    future[8..10].copy_from_slice(&9_u16.to_le_bytes());

    assert_eq!(
        ListenReportAcknowledgement::decode(&wrong_magic),
        Err(ListenReportError::InvalidMagic)
    );
    assert_eq!(
        ListenReportAcknowledgement::decode(&future),
        Err(ListenReportError::UnsupportedVersion(9))
    );
    assert_eq!(
        ListenReportAcknowledgement::decode(&encoded[..encoded.len() - 1]),
        Err(ListenReportError::UnexpectedEnd)
    );
}

#[test]
fn report_decode_rejects_wrong_magic_as_an_ordinary_error() {
    let mut encoded = sample_report().encode().unwrap();
    encoded[0] ^= 0xff;

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::InvalidMagic)
    );
}

#[test]
fn report_decode_rejects_an_unknown_version_as_an_ordinary_error() {
    let mut encoded = sample_report().encode().unwrap();
    encoded[8..10].copy_from_slice(&9_u16.to_le_bytes());

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::UnsupportedVersion(9))
    );
}

#[test]
fn report_decode_rejects_a_truncated_body_as_an_ordinary_error() {
    let mut encoded = sample_report().encode().unwrap();
    encoded.pop();

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::UnexpectedEnd)
    );
}

#[test]
fn report_decode_rejects_a_declared_path_larger_than_the_buffer() {
    let mut encoded = sample_report().encode().unwrap();
    let first_path_length = 10 + 4 + 8;
    encoded[first_path_length..first_path_length + 4].copy_from_slice(&u32::MAX.to_le_bytes());

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::UnexpectedEnd)
    );
}
