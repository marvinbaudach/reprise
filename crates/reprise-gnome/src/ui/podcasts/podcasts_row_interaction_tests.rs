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
    install_row_interaction(&root, 7);

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

/// `SRC-14`: what a press on a row means. Kept a pure decision so the mapping
/// is pinned without synthesising GTK events — the gesture handler does
/// nothing but call this and dispatch.
#[test]
fn src_14_a_plain_click_selects_and_a_double_click_plays() {
    use gtk4::gdk::ModifierType;

    assert_eq!(
        pointer_intent(1, ModifierType::empty()),
        RowIntent::Select(SelectMode::Only),
        "a single click selects instead of playing"
    );
    assert_eq!(pointer_intent(2, ModifierType::empty()), RowIntent::Play);
    assert_eq!(
        pointer_intent(3, ModifierType::empty()),
        RowIntent::Play,
        "a third press stays on the play the second one started"
    );
}

/// `SRC-14`: the modifiers choose the selection mode, exactly as in the track
/// list.
#[test]
fn src_14_modifiers_choose_the_selection_mode() {
    use gtk4::gdk::ModifierType;

    assert_eq!(
        pointer_intent(1, ModifierType::CONTROL_MASK),
        RowIntent::Select(SelectMode::Toggle)
    );
    assert_eq!(
        pointer_intent(1, ModifierType::SHIFT_MASK),
        RowIntent::Select(SelectMode::Range)
    );
    assert_eq!(
        pointer_intent(2, ModifierType::CONTROL_MASK),
        RowIntent::Play,
        "a double click plays whatever is held down"
    );
}

/// `SRC-14` / `ACC-8`: Space is the keyboard partner for Ctrl-click and
/// Shift+Space for Shift-click, which is what lets a keyboard user build a
/// multi-selection at all. Enter keeps playing.
#[test]
fn src_14_space_selects_and_enter_plays() {
    use gtk4::gdk::{Key, ModifierType};

    assert_eq!(
        key_intent(Key::space, ModifierType::empty()),
        Some(RowIntent::Select(SelectMode::Toggle))
    );
    assert_eq!(
        key_intent(Key::space, ModifierType::SHIFT_MASK),
        Some(RowIntent::Select(SelectMode::Range))
    );
    assert_eq!(
        key_intent(Key::Return, ModifierType::empty()),
        Some(RowIntent::Play)
    );
    assert_eq!(
        key_intent(Key::KP_Enter, ModifierType::empty()),
        Some(RowIntent::Play)
    );
    assert_eq!(
        key_intent(Key::a, ModifierType::empty()),
        None,
        "everything else keeps propagating"
    );
}

/// `POD-20`: the marker stays put under the pointer. Swapping it for a glyph
/// made the loaded row change its content on hover.
#[test]
fn pod_20_no_hover_swaps_the_marker_for_a_glyph() {
    let source = include_str!("podcasts_row_interaction.rs");

    assert!(
        !source.contains("install_playback_hover"),
        "the hover glyph swap must be gone, not merely unused"
    );
    assert!(
        !source.contains("media-playback-pause-symbolic"),
        "no pause glyph is built for grouped episode rows"
    );
}

/// `POD-20`: a plain episode Box opts into the app-wide hover feedback.
#[test]
fn pod_20_the_episode_row_marks_itself_on_hover_like_a_music_row() {
    let source = include_str!("podcasts_groups.rs");

    assert!(
        source.contains("reprise-hover"),
        "the episode row must carry the app-wide hover tint"
    );
}
