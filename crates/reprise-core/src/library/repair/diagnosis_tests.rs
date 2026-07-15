use std::path::PathBuf;

use super::*;

#[test]
fn classify_duplicate_ilst() {
    let warnings = vec![CapturedWarning {
        target: "lofty::mp4::moov".into(),
        message: "Multiple `ilst` atoms found, combining them".into(),
    }];
    let issues = classify_warnings(&warnings);
    assert_eq!(issues, vec![Issue::DuplicateIlst]);
}

#[test]
fn classify_corrupt_id3_frame_header() {
    let warnings = vec![CapturedWarning {
        target: "lofty::id3::v2::frame::read".into(),
        message: "Failed to read frame header, skipping: ID3v2: Failed to parse a frame ID: 0 x[54, 65, 78, 74]".into(),
    }];
    let issues = classify_warnings(&warnings);
    assert_eq!(issues, vec![Issue::CorruptId3Frames]);
}

#[test]
fn classify_missing_vbr_header() {
    let warnings = vec![CapturedWarning {
        target: "lofty::mpeg::properties".into(),
        message: "MPEG: Using bitrate to estimate duration".into(),
    }];
    let issues = classify_warnings(&warnings);
    assert_eq!(issues, vec![Issue::MissingVbrHeader]);
}

#[test]
fn classify_multiple_issues() {
    let warnings = vec![
        CapturedWarning {
            target: "lofty::id3::v2::frame::read".into(),
            message: "Failed to read frame header, skipping".into(),
        },
        CapturedWarning {
            target: "lofty::id3::v2::frame::read".into(),
            message: "Failed to parse a frame ID: 0".into(),
        },
        CapturedWarning {
            target: "lofty::mpeg::properties".into(),
            message: "MPEG: Using bitrate to estimate duration".into(),
        },
    ];
    let issues = classify_warnings(&warnings);
    // CorruptId3Frames should appear only once despite two matching warnings
    assert_eq!(issues, vec![Issue::CorruptId3Frames, Issue::MissingVbrHeader]);
}

#[test]
fn classify_unrelated_warning() {
    let warnings = vec![CapturedWarning {
        target: "lofty::ogg".into(),
        message: "some other warning".into(),
    }];
    let issues = classify_warnings(&warnings);
    assert!(issues.is_empty());
}

#[test]
fn classify_empty() {
    let issues = classify_warnings(&[]);
    assert!(issues.is_empty());
}

#[test]
fn diagnose_healthy_flac_has_no_issues() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let result = diagnose(&path).unwrap();
    assert!(result.issues.is_empty());
}
