use super::*;
use crate::ui::tag_editor_state::number_patch;

const TINY_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, b'I', b'D', b'A', b'T', 0x78, 0xda, 0x63, 0x64, 0xf8, 0x0f, 0x00,
    0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D',
    0xae, 0x42, 0x60, 0x82,
];

fn synchsafe(size: usize) -> [u8; 4] {
    [
        ((size >> 21) & 0x7f) as u8,
        ((size >> 14) & 0x7f) as u8,
        ((size >> 7) & 0x7f) as u8,
        (size & 0x7f) as u8,
    ]
}

fn mp3_with_embedded_cover(path: &std::path::Path) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reprise-platform-linux/tests/fixtures/sine.mp3");
    let audio = std::fs::read(source).unwrap();
    let mut picture = vec![
        0, b'i', b'm', b'a', b'g', b'e', b'/', b'p', b'n', b'g', 0, 3, 0,
    ];
    picture.extend_from_slice(TINY_PNG);
    let mut frame = b"APIC".to_vec();
    frame.extend_from_slice(&(picture.len() as u32).to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&picture);
    let mut tagged = b"ID3\x03\0\0".to_vec();
    tagged.extend_from_slice(&synchsafe(frame.len()));
    tagged.extend_from_slice(&frame);
    tagged.extend_from_slice(&audio);
    std::fs::write(path, tagged).unwrap();
}

#[test]
fn number_patch_distinguishes_unchanged_clear_set_and_invalid() {
    assert_eq!(number_patch(false, "bad"), Ok(None));
    assert_eq!(number_patch(true, ""), Ok(Some(None)));
    assert_eq!(number_patch(true, " 42 "), Ok(Some(Some(42))));
    assert!(number_patch(true, "forty-two").is_err());
    assert!(number_patch(true, "0").is_err());
}

#[test]
fn navigate_direction_has_expected_variants() {
    let prev = NavigateDirection::Previous;
    let next = NavigateDirection::Next;
    assert_ne!(prev, next);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_6_tag_editor_presents_before_its_cover_io() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let track_path = temp.path().join("covered.mp3");
    mp3_with_embedded_cover(&track_path);
    let source = reprise_core::cover::resolve_source(&track_path).expect("embedded cover source");
    assert!(matches!(
        source,
        reprise_core::cover::CoverSource::Embedded(_)
    ));
    reprise_core::cover::thumbnail(&source, ThumbnailSize::Grid)
        .expect("embedded cover must decode");
    let parent = adw::ApplicationWindow::builder()
        .default_width(800)
        .default_height(600)
        .build();
    parent.present();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let cover_loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let track = SessionTrack {
        id: 1,
        path: track_path,
        tags: reprise_core::library::tag_edit::EditableTags::default(),
        rating: 0,
    };

    let presented = present(
        &parent,
        &conn,
        vec![track],
        &[None],
        None,
        &cover_loader,
        PresentCallbacks {
            on_write_started: Rc::new(|| {}),
            on_saved: |_, _, _, _| {},
        },
    )
    .expect("non-empty editor");

    assert!(presented.dialog.is_mapped());
    assert!(presented.cover_picture.paintable().is_none());
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        presented.cover_picture.paintable().is_some()
    });
    assert!(presented.cover_picture.paintable().is_some());
    presented.dialog.close();
    parent.close();
}
