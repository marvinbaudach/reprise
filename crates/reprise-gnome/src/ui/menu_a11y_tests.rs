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
    popped_up_with(popover, &[])
}

/// [`popped_up`] with action groups installed on the window first, so the
/// popover resolves its items' actions (and their state) when it maps.
fn popped_up_with(
    popover: &gtk4::PopoverMenu,
    groups: &[(&str, &gio::SimpleActionGroup)],
) -> (gtk4::Window, Vec<gtk4::Widget>) {
    let anchor = gtk4::Button::with_label("anchor");
    let window = gtk4::Window::builder().child(&anchor).build();
    for (name, group) in groups {
        window.insert_action_group(name, Some(*group));
    }
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

fn buttons_of(popover: &gtk4::PopoverMenu) -> Vec<gtk4::Widget> {
    let mut buttons = Vec::new();
    model_buttons(popover.upcast_ref(), &mut buttons);
    buttons
}

/// A boolean toggle ("check" item) and a string-targeted radio pair.
fn stateful_actions() -> (
    gio::SimpleActionGroup,
    gio::SimpleAction,
    gio::SimpleAction,
    gio::Menu,
) {
    let toggle = gio::SimpleAction::new_stateful("toggle", None, &false.to_variant());
    let mode = gio::SimpleAction::new_stateful(
        "mode",
        Some(gtk4::glib::VariantTy::STRING),
        &"a".to_variant(),
    );
    let group = gio::SimpleActionGroup::new();
    group.add_action(&toggle);
    group.add_action(&mode);
    let model = gio::Menu::new();
    model.append(Some("Always on top"), Some("act.toggle"));
    for (label, target) in [("Mode A", "a"), ("Mode B", "b")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("act.mode"), Some(&target.to_variant()));
        model.append_item(&item);
    }
    (group, toggle, mode, model)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_a_toggle_or_radio_flipping_while_open_keeps_its_name() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (group, toggle, mode, model) = stateful_actions();
    let popover = popover_menu_from_model(&model);
    let (window, buttons) = popped_up_with(&popover, &[("act", &group)]);
    assert_eq!(buttons.len(), 3);
    assert_named_by_text(&buttons);

    toggle.set_state(&true.to_variant());
    mode.set_state(&"b".to_variant());
    settle_for(SETTLE);

    assert_named_by_text(&buttons_of(&popover));
    toggle.set_state(&false.to_variant());
    mode.set_state(&"a".to_variant());
    settle_for(SETTLE);
    assert_named_by_text(&buttons_of(&popover));
    popover.popdown();
    popover.unparent();
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_items_added_to_the_live_model_while_open_are_named() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let section = gio::Menu::new();
    let model = gio::Menu::new();
    model.append(Some("Fixed item"), Some("win.fixed"));
    model.append_section(None, &section);
    let popover = popover_menu_from_model(&model);
    let (window, buttons) = popped_up(&popover);
    assert_eq!(buttons.len(), 1);

    model.append(Some("Added to the menu"), Some("win.added"));
    section.append(Some("Added to the section"), Some("win.section"));
    settle_for(SETTLE);
    section.remove_all();
    section.append(Some("Replaced in the section"), Some("win.replaced"));
    settle_for(SETTLE);

    let buttons = buttons_of(&popover);
    let texts: Vec<String> = buttons
        .iter()
        .filter_map(visible_text)
        .map(|text| text.to_string())
        .collect();
    for expected in ["Added to the menu", "Replaced in the section"] {
        assert!(
            texts.iter().any(|text| text == expected),
            "{expected:?} missing from {texts:?}"
        );
    }
    assert_named_by_text(&buttons);
    popover.popdown();
    popover.unparent();
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_repeated_flips_remaps_and_calls_connect_one_handler_per_object() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (group, toggle, _mode, model) = stateful_actions();
    let submenu = gio::Menu::new();
    submenu.append(Some("Inner item"), Some("win.inner"));
    model.append_submenu(Some("Submenu"), &submenu);
    let popover = popover_menu_from_model(&model);
    let (window, buttons) = popped_up_with(&popover, &[("act", &group)]);
    let baseline = connection_count();
    assert!(
        baseline >= buttons.len(),
        "every model button, model and the popover is connected: {baseline} connections \
         for {} buttons",
        buttons.len()
    );

    const APPENDED: usize = 4;
    for flip in 0..APPENDED {
        toggle.set_state(&(flip % 2 == 0).to_variant());
        popover.popdown();
        settle_for(SETTLE);
        popover.popup();
        settle_for(SETTLE);
        name_model_buttons_on_map(&popover);
        model.append(Some(&format!("Extra {flip}")), Some("win.extra"));
        settle_for(SETTLE);
    }

    assert_eq!(
        connection_count(),
        baseline + APPENDED,
        "only the appended items may add a connection, one `notify::active` each"
    );
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
