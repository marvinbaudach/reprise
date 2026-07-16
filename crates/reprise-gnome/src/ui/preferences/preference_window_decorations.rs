use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, WindowDecorationMode};

use super::window_decoration_strings as strings;
use super::PreferencesContext;

pub(in crate::ui) fn mode_from_index(index: u32) -> WindowDecorationMode {
    match index {
        1 => WindowDecorationMode::System,
        _ => WindowDecorationMode::Client,
    }
}

pub(in crate::ui) fn mode_index(mode: WindowDecorationMode) -> u32 {
    match mode {
        WindowDecorationMode::Client => 0,
        WindowDecorationMode::System => 1,
    }
}

pub(in crate::ui) fn row(context: &Rc<PreferencesContext>) -> adw::ComboRow {
    let model = gtk4::StringList::new(&[
        &strings::text(strings::DECORATION_CLIENT),
        &strings::text(strings::DECORATION_SYSTEM),
    ]);
    let selected = {
        let conn = context.conn.borrow();
        mode_index(settings::get_window_decoration_mode(&conn))
    };
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::WINDOW_DECORATIONS))
        .subtitle(strings::text(strings::WINDOW_DECORATIONS_SUBTITLE))
        .model(&model)
        .selected(selected)
        .build();
    let weak = Rc::downgrade(context);
    row.connect_selected_notify(move |row| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        let mode = mode_from_index(row.selected());
        let saved = {
            let conn = context.conn.borrow();
            settings::set_window_decoration_mode(&conn, mode)
        };
        if let Err(error) = saved {
            tracing::warn!(%error, "could not save window decoration mode");
            row.set_selected(mode_index(context.decorations.mode()));
            return;
        }
        context.decorations.apply(mode);
    });
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoration_mode_combo_indices_are_stable() {
        assert_eq!(mode_from_index(0), WindowDecorationMode::Client);
        assert_eq!(mode_from_index(1), WindowDecorationMode::System);
        assert_eq!(mode_from_index(99), WindowDecorationMode::Client);
        assert_eq!(mode_index(WindowDecorationMode::Client), 0);
        assert_eq!(mode_index(WindowDecorationMode::System), 1);
    }
}
