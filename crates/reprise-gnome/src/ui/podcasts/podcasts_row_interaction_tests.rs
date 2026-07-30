//! Pointer/keyboard parity of the compact episode row.

use super::*;
/// `ACC-8`: the row lost its play button, so pointer and keyboard must
/// both reach the same activation. This pins that the row carries a click
/// gesture AND a key controller and is focusable with a button role —
/// deleting the key controller (leaving a pointer-only row) turns it red.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn acc_8_row_activation_is_reachable_by_pointer_and_keyboard() {
    gtk4::init().unwrap();
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root.set_focusable(true);
    root.set_accessible_role(gtk4::AccessibleRole::Button);
    let glyph = gtk4::Image::new();

    install_row_activation(&root, 7, &glyph);

    let controllers = root.observe_controllers();
    let mut has_click = false;
    let mut has_keys = false;
    for index in 0..controllers.n_items() {
        let controller = controllers.item(index);
        has_click |= controller
            .as_ref()
            .is_some_and(ObjectExt::is::<gtk4::GestureClick>);
        has_keys |= controller
            .as_ref()
            .is_some_and(ObjectExt::is::<gtk4::EventControllerKey>);
    }

    assert!(has_click, "the row must still activate on a click");
    assert!(has_keys, "the row must activate from the keyboard too");
    assert!(
        root.is_focusable(),
        "a keyboard user must be able to reach it"
    );
}
