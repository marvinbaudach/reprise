use std::path::{Path, PathBuf};
use std::time::Duration;

use gtk4::gio;
use gtk4::prelude::*;

use super::*;
use crate::ui::test_accessible_label::{accessible_label_mismatch, has_labelled_by_relation};
use crate::ui::test_settle::{settle_for, settle_until_mapped};
use crate::ui::track_list::track_menu::{
    build_track_menu, MenuContext, MenuInputs, PlaylistEntry, SelectionSummary,
};

const SETTLE: Duration = Duration::from_millis(300);

fn model_buttons(root: &gtk4::Widget, found: &mut Vec<gtk4::Widget>) {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if widget.type_().name() == MODEL_BUTTON_TYPE {
            found.push(widget.clone());
        }
        model_buttons(&widget, found);
        child = widget.next_sibling();
    }
}

/// Pops `popover` up on a realized window and returns the window with the
/// popover's model buttons, submenu pages included.
fn popped_up(popover: &gtk4::PopoverMenu) -> (gtk4::Window, Vec<gtk4::Widget>) {
    let anchor = gtk4::Button::with_label("anchor");
    let window = gtk4::Window::builder().child(&anchor).build();
    window.present();
    popover.set_parent(&anchor);
    popover.popup();
    assert!(settle_until_mapped(popover), "the popover maps");
    settle_for(SETTLE);
    let mut buttons = Vec::new();
    model_buttons(popover.upcast_ref(), &mut buttons);
    (window, buttons)
}

fn assert_named_by_text(buttons: &[gtk4::Widget]) {
    for button in buttons {
        let text = visible_text(button).expect("every item in these menus draws text");
        assert_eq!(
            accessible_label_mismatch(button.upcast_ref::<gtk4::Accessible>(), &text),
            None,
            "the item drawn as {text:?} must carry that text as its accessible label"
        );
        assert!(
            !has_labelled_by_relation(button.upcast_ref::<gtk4::Accessible>()),
            "the item drawn as {text:?} must not be named through GTK's labelled-by relation, \
             which resolves to an empty name"
        );
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_every_model_button_of_a_popover_menu_is_named_by_its_text() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let submenu = gio::Menu::new();
    submenu.append(Some("Inner item"), Some("win.inner"));
    let model = gio::Menu::new();
    model.append(Some("Plain item"), Some("win.plain"));
    model.append_submenu(Some("Submenu"), &submenu);
    let popover = popover_menu_from_model(&model);

    let (window, buttons) = popped_up(&popover);

    assert!(
        buttons.len() >= 3,
        "the plain item, the submenu entry and the submenu's item, found {}",
        buttons.len()
    );
    assert_named_by_text(&buttons);
    popover.popdown();
    popover.unparent();
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_a_replaced_model_gets_named_buttons() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let first = gio::Menu::new();
    first.append(Some("First item"), Some("win.first"));
    let popover = popover_menu_from_model(&first);
    let second = gio::Menu::new();
    second.append(Some("Second item"), Some("win.second"));
    popover.set_menu_model(Some(&second));

    let (window, buttons) = popped_up(&popover);

    assert_eq!(buttons.len(), 1);
    assert_named_by_text(&buttons);
    popover.popdown();
    popover.unparent();
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_a_menu_buttons_popover_is_named_by_its_helper_call() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let model = gio::Menu::new();
    model.append(Some("Menu button item"), Some("win.item"));
    let button = gtk4::MenuButton::builder().menu_model(&model).build();
    name_menu_button_items(&button);
    let window = gtk4::Window::builder().child(&button).build();
    window.present();
    button.popup();
    settle_for(SETTLE);

    let popover = button
        .popover()
        .expect("a menu button with a model has one");
    let mut buttons = Vec::new();
    model_buttons(popover.upcast_ref(), &mut buttons);

    assert_eq!(buttons.len(), 1);
    assert_named_by_text(&buttons);
    button.popdown();
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_the_track_context_menu_items_are_named_by_their_text() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let selection = SelectionSummary {
        count: 1,
        any_missing: false,
        all_missing: false,
        same_album: true,
        same_artist: true,
        same_folder: true,
    };
    let playlists = [PlaylistEntry {
        id: 7,
        name: "Evening".into(),
        is_current: false,
    }];
    let model = build_track_menu(&MenuInputs {
        context: MenuContext::LibraryTracks,
        selection: &selection,
        playlists: &playlists,
        is_missing_view: false,
    });
    let popover = popover_menu_from_model(&model);

    let (window, buttons) = popped_up(&popover);

    let texts: Vec<String> = buttons
        .iter()
        .filter_map(visible_text)
        .map(|text| text.to_string())
        .collect();
    for expected in ["Play next", "Edit tags…", "Move to Trash…", "Evening"] {
        assert!(
            texts.iter().any(|text| text == expected),
            "{expected:?} missing from the menu's items {texts:?}"
        );
    }
    assert_named_by_text(&buttons);
    popover.popdown();
    popover.unparent();
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_the_spoken_name_drops_the_mnemonic_underscore() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let holder = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    holder.append(&gtk4::Label::with_mnemonic("_Open"));

    assert_eq!(visible_text(holder.upcast_ref()).as_deref(), Some("Open"));
}

fn ui_sources(directory: &Path, found: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            ui_sources(&path, found);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let is_production_source =
            name.ends_with(".rs") && !name.ends_with("_tests.rs") && !name.starts_with("menu_a11y");
        if is_production_source {
            if let Ok(source) = std::fs::read_to_string(&path) {
                found.push((path, source));
            }
        }
    }
}

fn production_sources() -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    ui_sources(&root, &mut found);
    assert!(found.len() > 100, "the scan reads the ui source tree");
    found
}

fn code_lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
}

#[test]
fn gp_10_no_popover_menu_is_built_without_the_naming_helper() {
    const CONSTRUCTORS: [&str; 3] = [
        "PopoverMenu::from_model",
        "PopoverMenu::builder",
        "PopoverMenu::new_from_model",
    ];
    let bypasses: Vec<String> = production_sources()
        .into_iter()
        .filter(|(_, source)| {
            code_lines(source).any(|line| CONSTRUCTORS.iter().any(|name| line.contains(name)))
        })
        .map(|(path, _)| path.display().to_string())
        .collect();

    assert!(
        bypasses.is_empty(),
        "build popover menus with menu_a11y::popover_menu_from_model so their items have \
         accessible names (GTK fb6f2118); bypassed in {bypasses:?}"
    );
}

#[test]
fn gp_10_every_menu_button_with_a_model_names_its_items() {
    let bypasses: Vec<String> = production_sources()
        .into_iter()
        .filter(|(_, source)| {
            code_lines(source).any(|line| line.contains(".menu_model(&"))
                && !source.contains("menu_a11y::name_menu_button_items")
        })
        .map(|(path, _)| path.display().to_string())
        .collect();

    assert!(
        bypasses.is_empty(),
        "call menu_a11y::name_menu_button_items for a MenuButton built with a menu model, so \
         its items have accessible names (GTK fb6f2118); missing in {bypasses:?}"
    );
}
