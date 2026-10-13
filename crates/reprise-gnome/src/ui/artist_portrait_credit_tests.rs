use super::*;

use crate::ui::artist_portrait_tiles::ArtistPortraitTiles;
use crate::ui::now_playing::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::updates::release_cover::LazyReleaseCover;

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8, 0xAF, 0xAF, 0x07,
    0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

const EXPECTED_TEXT: &str = "Photo: <Photographer & editor> · CC BY-SA 4.0 · Wikimedia Commons";
const FILE_PAGE: &str = "https://commons.wikimedia.org/wiki/File:Portrait.jpg";

fn commons_credit() -> PortraitCredit {
    PortraitCredit {
        author: "<Photographer & editor>".into(),
        license: "CC BY-SA 4.0".into(),
        license_url: "https://creativecommons.org/licenses/by-sa/4.0/".into(),
        file_page_url: FILE_PAGE.into(),
    }
}

fn menu_label(menu: &gio::Menu) -> Option<String> {
    menu.item_attribute_value(0, "label", Some(glib::VariantTy::STRING))
        .and_then(|value| value.get::<String>())
}

fn has_secondary_click(root: &gtk4::Overlay) -> bool {
    let controllers = root.observe_controllers();
    (0..controllers.n_items()).any(|index| {
        controllers
            .item(index)
            .and_downcast::<gtk4::GestureClick>()
            .is_some_and(|gesture| gesture.button() == gtk4::gdk::BUTTON_SECONDARY)
    })
}

fn credited_tiles(portrait: std::path::PathBuf) -> Rc<ArtistPortraitTiles> {
    let tiles = ArtistPortraitTiles::for_test_with_credit(
        move |_| Some(portrait.clone()),
        move |_| Some(commons_credit()),
    );
    tiles.set_sources(
        crate::ui::cover_loader::CoverLoader::new(
            crate::ui::cover_download_worker::setup_for_test(),
        ),
        ArtistPortraitRuntime::for_test(false, |_| None),
    );
    tiles
}

fn portrait_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let portrait = dir.path().join("portrait.png");
    std::fs::write(&portrait, TINY_PNG).unwrap();
    portrait
}

fn settle_credit(tile: &LazyReleaseCover) {
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || tile.shows_image() && tile.widget().tooltip_text().is_some(),
    ));
}

fn popover_in(root: &gtk4::Overlay) -> Option<gtk4::PopoverMenu> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(popover) = widget.downcast::<gtk4::PopoverMenu>() {
            return Some(popover);
        }
    }
    None
}

fn menu_key(root: &gtk4::Overlay) -> Option<gtk4::EventControllerKey> {
    menu_controllers(root)
        .into_iter()
        .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
}

#[test]
fn art_5_credit_links_open_only_https_commons_pages_without_user_info() {
    assert_eq!(validated_credit_url(FILE_PAGE).as_deref(), Some(FILE_PAGE));
    assert_eq!(
        validated_credit_url("HTTPS://Commons.Wikimedia.ORG/wiki/File:A.jpg").as_deref(),
        Some("HTTPS://Commons.Wikimedia.ORG/wiki/File:A.jpg")
    );
    for refused in [
        "http://commons.wikimedia.org/wiki/File:Portrait.jpg",
        "javascript:alert(1)",
        "https://",
        "https://evil.example/wiki/File:Portrait.jpg",
        "https://commons.wikimedia.org.evil.example/wiki/File:Portrait.jpg",
        "https://commons.wikimedia.org@evil.example/wiki/File:Portrait.jpg",
        "https://user:pass@commons.wikimedia.org/wiki/File:Portrait.jpg",
        "https://upload.wikimedia.org/wiki/File:Portrait.jpg",
    ] {
        assert!(validated_credit_url(refused).is_none(), "{refused}");
    }
}

#[test]
fn art_5_credit_links_remember_a_commons_page_and_forget_it_for_an_uncredited_portrait() {
    let links = CreditLinks::default();

    links.remember("Lorna Shore", Some(&commons_credit()));
    assert_eq!(links.url_for("Lorna Shore").as_deref(), Some(FILE_PAGE));
    assert!(links.url_for("Someone Else").is_none());

    let mut untrusted = commons_credit();
    untrusted.file_page_url = "https://evil.example/".into();
    links.remember("Lorna Shore", Some(&untrusted));
    assert!(links.url_for("Lorna Shore").is_none());

    links.remember("Lorna Shore", Some(&commons_credit()));
    links.remember("Lorna Shore", None);
    assert!(links.url_for("Lorna Shore").is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn art_5_a_commons_portrait_credits_the_releases_tile_without_claiming_input() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let tiles = credited_tiles(portrait_file(&cache));
    let tile = LazyReleaseCover::new("", "Lorna Shore", 160);

    tiles.show(&tile);
    settle_credit(&tile);

    let root = tile.widget();
    assert_eq!(root.tooltip_text().as_deref(), Some(EXPECTED_TEXT));
    assert_eq!(
        root.accessible_role(),
        gtk4::AccessibleRole::Generic,
        "the tile keeps its role; the row menu owns the interaction"
    );
    assert!(!has_secondary_click(root));
    assert!(menu_controllers(root).is_empty());
    assert_eq!(
        tiles.credit_links().url_for("Lorna Shore").as_deref(),
        Some(FILE_PAGE)
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn art_5_a_remapped_tile_keeps_its_credit() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let tiles = credited_tiles(portrait_file(&cache));
    let tile = LazyReleaseCover::new("", "Lorna Shore", 160);
    tiles.show(&tile);
    settle_credit(&tile);

    // What a scroll out and back in does: the map handler calls show() again
    // for a cell that is still bound to the same artist.
    tiles.show(&tile);

    assert_eq!(tile.widget().tooltip_text().as_deref(), Some(EXPECTED_TEXT));
    assert!(tile.shows_image());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn art_5_a_recycled_cell_bound_to_a_release_cover_drops_the_credit() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let tiles = credited_tiles(portrait_file(&cache));
    let tile = LazyReleaseCover::new("", "Lorna Shore", 160);
    tiles.show(&tile);
    settle_credit(&tile);

    tile.set_release("mbid-with-a-real-cover", "Someone Else");

    assert!(tile.widget().tooltip_text().is_none());
    tile.set_release("", "Lorna Shore");
    tiles.show(&tile);
    settle_credit(&tile);
    tile.set_artist_key("Concert Artist");
    assert!(tile.widget().tooltip_text().is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn art_5_a_deezer_portrait_has_no_credit_and_no_input_handlers() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let portrait = portrait_file(&cache);
    let tiles =
        ArtistPortraitTiles::for_test_with_credit(move |_| Some(portrait.clone()), |_| None);
    tiles.set_sources(
        crate::ui::cover_loader::CoverLoader::new(
            crate::ui::cover_download_worker::setup_for_test(),
        ),
        ArtistPortraitRuntime::for_test(false, |_| None),
    );
    let tile = LazyReleaseCover::new("", "Lorna Shore", 160);
    let card = crate::ui::stats::stats_band_card::StatsBandCard::new();

    tiles.show(&tile);
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || tile.shows_image(),
    ));
    apply_credit_with_menu(card.widget(), None);

    for surface in [tile.widget(), card.widget()] {
        assert!(surface.tooltip_text().is_none());
        assert!(!has_secondary_click(surface));
        assert!(menu_controllers(surface).is_empty());
    }
    assert!(tiles.credit_links().url_for("Lorna Shore").is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn art_5_the_stats_card_offers_the_image_source_by_keyboard_and_drops_it_with_the_credit() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let card = crate::ui::stats::stats_band_card::StatsBandCard::new();
    let window = gtk4::Window::new();
    window.set_default_size(400, 300);
    window.set_child(Some(card.widget()));
    window.present();

    apply_credit_with_menu(card.widget(), Some(&commons_credit()));

    assert_eq!(card.widget().tooltip_text().as_deref(), Some(EXPECTED_TEXT));
    assert!(has_secondary_click(card.widget()));
    assert_eq!(
        menu_label(&open_source_menu()),
        Some("Open image source".into())
    );

    // Wiring twice must not stack a second set of controllers.
    apply_credit_with_menu(card.widget(), Some(&commons_credit()));
    assert_eq!(menu_controllers(card.widget()).len(), 2);

    let keys = menu_key(card.widget()).expect("the card listens for the context shortcut");
    let plain = keys.emit_by_name::<bool>(
        "key-pressed",
        &[&gtk4::gdk::Key::a, &0u32, &gtk4::gdk::ModifierType::empty()],
    );
    assert!(!plain);
    assert!(popover_in(card.widget()).is_none());
    let consumed = keys.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk4::gdk::Key::F10,
            &0u32,
            &gtk4::gdk::ModifierType::SHIFT_MASK,
        ],
    );
    assert!(consumed);
    assert!(popover_in(card.widget()).is_some());

    apply_credit_with_menu(card.widget(), None);
    assert!(card.widget().tooltip_text().is_none());
    assert!(menu_controllers(card.widget()).is_empty());
    assert!(!has_secondary_click(card.widget()));
}
