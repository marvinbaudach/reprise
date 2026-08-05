//! The seek bar's context menu: one entry, and the way back to the
//! colour-scale legend once it has stopped appearing on its own.
//!
//! Split out of `player_bar.rs` to keep that file under the project's
//! 800-line cap.

use gtk4::prelude::*;

use crate::ui::strings;
use crate::ui::waveform_seek::WaveformSeek;

/// The action group the seek bar's context menu lives in, and the one entry in
/// it. One item, because there is one thing about this bar a menu can usefully
/// say — and it exists so a hint that has stopped appearing can still be
/// reached by whoever missed it.
const SEEK_MENU_GROUP: &str = "seek";
const ACTION_EXPLAIN_COLOR_SCALE: &str = "explain-color-scale";

/// Installs the seek bar's right-click menu. The action is disabled in the
/// single-colour bar: explaining a scale that is not on screen would be worse
/// than not offering it.
pub(super) fn install(
    waveform: &WaveformSeek,
    legend: &super::seek_legend::SeekLegend,
) -> gtk4::gio::SimpleAction {
    let explain = gtk4::gio::SimpleAction::new(ACTION_EXPLAIN_COLOR_SCALE, None);
    explain.connect_activate({
        let legend = legend.clone();
        move |_, _| legend.show()
    });
    let actions = gtk4::gio::SimpleActionGroup::new();
    gtk4::prelude::ActionMapExt::add_action(&actions, &explain);
    let area = waveform.widget();
    area.insert_action_group(SEEK_MENU_GROUP, Some(&actions));

    let model = gtk4::gio::Menu::new();
    model.append(
        Some(&strings::text(strings::EXPLAIN_COLOR_SCALE)),
        Some(&format!("{SEEK_MENU_GROUP}.{ACTION_EXPLAIN_COLOR_SCALE}")),
    );
    let popover = gtk4::PopoverMenu::from_model(Some(&model));
    popover.set_parent(area);
    popover.set_has_arrow(false);
    popover.set_halign(gtk4::Align::Start);
    // A popover parented to a widget outlives it unless it is taken off
    // explicitly, and GTK warns loudly when the widget is finalized first.
    area.connect_destroy({
        let popover = popover.clone();
        move |_| popover.unparent()
    });

    // input-parity: ACC-8 keyboard=menu-key
    let secondary = gtk4::GestureClick::new();
    secondary.set_button(gtk4::gdk::BUTTON_SECONDARY);
    secondary.connect_pressed({
        let popover = popover.clone();
        move |gesture, _, x, y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        }
    });
    area.add_controller(secondary);
    explain
}
