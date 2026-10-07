//! TAG-1 display regressions for the focus handover after a Tag Editor save:
//! the dialog closing must not scroll the table to wherever a recycled row
//! widget now lives. Shares the fixture and samplers of its parent module.

use gtk4::prelude::*;

use super::{
    assert_no_visible_jump, record_viewport, save_refresh, scrolled_library, settle_after_reload,
    SETTLE, VISIBLE_JUMP_PX,
};

/// The Tag Editor restores keyboard focus through
/// `TransientFocusGuard::capture`, which remembers *the widget* that had
/// focus when the dialog opened. Opened from the track table, that widget is
/// a `GtkColumnView` row — and those are recycled: after the save's
/// `items_changed(0, old, new)` the very same widget is bound to a different
/// row. Restoring focus onto it therefore scrolls wherever it now lives.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_restoring_dialog_focus_after_a_save_keeps_the_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    let before = fixture.adjustment.value();
    assert!(
        before > VISIBLE_JUMP_PX,
        "precondition: the list must be scrolled well away from the top, got {before}"
    );

    // Opening the editor captures whatever the table had focused — the row
    // the user clicked. Without that, this test would capture the window
    // itself and prove nothing.
    let focused = gtk4::prelude::GtkWindowExt::focus(&fixture.window)
        .expect("precondition: something in the window must hold focus");
    assert!(
        focused.is_ancestor(&fixture.track_list.shared.column_view),
        "precondition: focus must sit on a row inside the table, not on {}",
        focused.type_()
    );
    let guard = crate::ui::transient_focus::TransientFocusGuard::capture(&fixture.window);
    let samples = record_viewport(&fixture);

    let receipt = save_refresh(&fixture);
    settle_after_reload(&fixture, &receipt);
    let restored = fixture.adjustment.value();
    assert!(
        (restored - before).abs() < VISIBLE_JUMP_PX,
        "precondition: the save refresh itself must have put the viewport back, \
         before={before}, restored={restored}"
    );

    // The dialog finished closing: focus goes back to the captured widget.
    guard.restore();
    crate::ui::test_settle::settle_for(SETTLE);

    assert_no_visible_jump(&samples, before, "the dialog's focus restore");
    assert!(
        (fixture.adjustment.value() - before).abs() < VISIBLE_JUMP_PX,
        "restoring the dialog's focus moved the viewport: before={before}, after={}",
        fixture.adjustment.value()
    );
    fixture.window.close();
}

/// The reported sequence: the dialog closes, GTK hands keyboard focus back to
/// the table, and the table scrolls to whichever row it now considers
/// focused. The save's `items_changed(0, old, new)` has meanwhile reset that
/// focus row to the top, because the scroll restore deliberately scrolls
/// without `ListScrollFlags::FOCUS`.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_focus_returning_to_the_table_after_a_save_keeps_the_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    let before = fixture.adjustment.value();
    assert!(
        before > VISIBLE_JUMP_PX,
        "precondition: the list must be scrolled well away from the top, got {before}"
    );

    // The dialog is open: it, not the table, owns keyboard focus.
    fixture.elsewhere.grab_focus();
    let receipt = save_refresh(&fixture);
    settle_after_reload(&fixture, &receipt);
    let restored = fixture.adjustment.value();
    assert!(
        (restored - before).abs() < VISIBLE_JUMP_PX,
        "precondition: the save refresh itself must have put the viewport back, \
         before={before}, restored={restored}"
    );

    let samples = record_viewport(&fixture);
    // The dialog closes and focus returns to the library table.
    fixture.track_list.shared.column_view.grab_focus();
    crate::ui::test_settle::settle_for(SETTLE);

    assert_no_visible_jump(
        &samples,
        restored,
        "the focus handover after the dialog closed",
    );
    assert!(
        (fixture.adjustment.value() - restored).abs() < VISIBLE_JUMP_PX,
        "focus returning to the table moved the viewport: restored={restored}, after={}",
        fixture.adjustment.value()
    );
    fixture.window.close();
}
