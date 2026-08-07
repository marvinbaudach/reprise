//! Library preferences backed by the existing safe picker, scanner, and import paths.

use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings;

use super::strings;
use super::PreferencesContext;

#[derive(Debug, PartialEq, Eq)]
struct RescanRowState {
    subtitle: String,
    sensitive: bool,
    opacity_percent: u8,
}

fn rescan_row_state(detail: Option<&str>, scanning: bool) -> RescanRowState {
    RescanRowState {
        subtitle: detail.map_or_else(|| strings::text(strings::LIBRARY_UP_TO_DATE), str::to_owned),
        sensitive: !scanning,
        opacity_percent: if scanning { 45 } else { 100 },
    }
}

fn apply_rescan_row_state(row: &adw::ActionRow, state: &RescanRowState) {
    row.set_subtitle(&state.subtitle);
    row.set_sensitive(state.sensitive);
    row.set_opacity(f64::from(state.opacity_percent) / 100.0);
}

fn build_rescan_row(callback: Rc<dyn Fn()>) -> adw::ActionRow {
    let idle_subtitle = strings::text(strings::LIBRARY_UP_TO_DATE);
    let row = adw::ActionRow::builder()
        .title(strings::text(strings::CONTEXT_MENU_RESCAN_LIBRARY))
        .subtitle(&idle_subtitle)
        .subtitle_lines(1)
        .title_lines(1)
        .activatable(true)
        .build();
    // Ellipsizing alone still lets Pango advertise the full natural width.
    // Cap the internal subtitle label's character request so long scan detail
    // cannot widen the row or the dialog before it is ellipsized.
    if let Some(subtitle) = descendant_label_with_text(row.upcast_ref(), &idle_subtitle) {
        subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        subtitle.set_max_width_chars(1);
    }
    row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    row.connect_activated(move |_| callback());
    row
}

fn descendant_label_with_text(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
    if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
        if label.label() == text {
            return Some(label);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(label) = descendant_label_with_text(&current, text) {
            return Some(label);
        }
        child = current.next_sibling();
    }
    None
}

fn library_root_text(context: &PreferencesContext) -> String {
    let root = {
        let conn = &context.conn;
        settings::get_library_root(conn)
    };
    root.ok()
        .flatten()
        .unwrap_or_else(|| strings::text(strings::NO_LIBRARY_FOLDER))
}

impl PreferencesContext {
    pub(in crate::ui) fn refresh_library_folder_rows(&self) {
        let subtitle = library_root_text(self);
        let rows = std::mem::take(&mut *self.library_folder_rows.borrow_mut());
        let mut live_rows = Vec::with_capacity(rows.len());
        for weak in rows {
            if let Some(row) = weak.upgrade() {
                row.set_subtitle(&subtitle);
                live_rows.push(weak);
            }
        }
        *self.library_folder_rows.borrow_mut() = live_rows;
    }

    pub(in crate::ui) fn library_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_LIBRARY))
            .icon_name("folder-music-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        let folder = adw::ActionRow::builder()
            .title(strings::text(strings::LIBRARY_FOLDER))
            .subtitle(library_root_text(self))
            .build();
        self.library_folder_rows
            .borrow_mut()
            .push(folder.downgrade());
        let choose = gtk4::Button::with_label(&strings::text(strings::CHOOSE_FOLDER));
        choose.set_valign(gtk4::Align::Center);
        let scan_button = self.scan_button.clone();
        choose.connect_clicked(move |_| scan_button.emit_clicked());
        folder.add_suffix(&choose);
        group.add(&folder);

        let excluded_count = {
            let conn = &self.conn;
            reprise_core::library::exclusions::count(conn).unwrap_or_else(|error| {
                tracing::warn!(%error, "could not count library exclusions");
                0
            }) as usize
        };
        let excluded = adw::ActionRow::builder()
            .title(strings::text(strings::EXCLUDED_FILES))
            .subtitle(strings::excluded_files_subtitle(excluded_count))
            .build();
        let restore = gtk4::Button::with_label(&strings::text(strings::RESTORE_EXCLUDED_FILES));
        restore.set_valign(gtk4::Align::Center);
        restore.set_sensitive(excluded_count > 0);
        let weak = Rc::downgrade(self);
        let excluded_for_restore = excluded.clone();
        restore.connect_clicked(move |button| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let result = {
                let conn = &context.conn;
                reprise_core::library::exclusions::clear(conn)
            };
            match result {
                Ok(_) => {
                    button.set_sensitive(false);
                    excluded_for_restore.set_subtitle(&strings::excluded_files_subtitle(0));
                    context.track_list.rescan_library();
                }
                Err(error) => {
                    tracing::warn!(%error, "could not restore library exclusions");
                    if let Some(player) = &context.player {
                        player.show_toast(&strings::text(strings::RESTORE_EXCLUDED_FILES_FAILED));
                    }
                }
            }
        });
        excluded.add_suffix(&restore);
        group.add(&excluded);

        let weak = Rc::downgrade(self);
        let rescan = build_rescan_row(Rc::new(move || {
            if let Some(context) = weak.upgrade() {
                context.track_list.rescan_library();
            }
        }));
        apply_rescan_row_state(
            &rescan,
            &rescan_row_state(
                self.scan_controls.current_presentation_detail().as_deref(),
                !self.scan_button.is_sensitive(),
            ),
        );
        let rescan_for_activity = rescan.downgrade();
        self.scan_button.connect_sensitive_notify(move |button| {
            let Some(rescan) = rescan_for_activity.upgrade() else {
                return;
            };
            rescan.set_sensitive(button.is_sensitive());
            rescan.set_opacity(if button.is_sensitive() { 1.0 } else { 0.45 });
        });
        let rescan_for_detail = rescan.downgrade();
        let subscription = self.scan_controls.subscribe_presentation(move |detail| {
            let Some(rescan) = rescan_for_detail.upgrade() else {
                return;
            };
            let subtitle = detail.unwrap_or_else(|| strings::text(strings::LIBRARY_UP_TO_DATE));
            rescan.set_subtitle(&subtitle);
        });
        rescan.connect_destroy(move |_| {
            let _keep_subscription_alive_until_destroy = &subscription;
        });
        group.add(&rescan);

        page.add(&group);
        page
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gtk4::prelude::*;

    use super::*;

    #[test]
    fn rescan_row_has_idle_and_running_one_line_presentations() {
        assert_eq!(
            rescan_row_state(None, false),
            RescanRowState {
                subtitle: "Library up to date".to_owned(),
                sensitive: true,
                opacity_percent: 100,
            }
        );
        assert_eq!(
            rescan_row_state(
                Some("748 of 1,909 checked · 6 cached · 113 unavailable"),
                true,
            ),
            RescanRowState {
                subtitle: "748 of 1,909 checked · 6 cached · 113 unavailable".to_owned(),
                sensitive: false,
                opacity_percent: 45,
            }
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_9_rescan_subtitle_keeps_one_line_and_a_stable_row_height() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let row = build_rescan_row(Rc::new(|| {}));
        let group = adw::PreferencesGroup::new();
        group.add(&row);
        let window = gtk4::Window::builder()
            .default_width(420)
            .default_height(160)
            .child(&group)
            .build();
        window.present();
        settle_layout();
        let idle_height = row.height();
        let idle_widths = row.measure(gtk4::Orientation::Horizontal, -1);

        apply_rescan_row_state(
            &row,
            &rescan_row_state(
                Some("748 of 1,909 checked · 6 cached · 113 unavailable"),
                true,
            ),
        );
        settle_layout();

        assert_eq!(row.subtitle_lines(), 1);
        assert_eq!(row.width_request(), -1);
        assert_eq!(
            row.measure(gtk4::Orientation::Horizontal, -1),
            idle_widths,
            "the running detail must not add a horizontal size request"
        );
        assert_eq!(row.height(), idle_height);
        assert!(!row.is_sensitive());
        assert!((row.opacity() - 0.45).abs() < 0.01);
        window.close();
    }

    fn settle_layout() {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            quit.quit();
        });
        main_loop.run();
    }
}
