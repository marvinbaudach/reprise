//! Library Doctor plugin controls shared with the main-window result page.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;

use super::{strings, PreferencesContext};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_ENABLE: &str = "enable";

type RunCallback = Rc<dyn Fn(u32)>;
type RevertCallback = Rc<dyn Fn()>;

#[derive(Default)]
pub(in crate::ui) struct DoctorPreferenceControls {
    expander_rows: RefCell<Vec<glib::WeakRef<adw::ExpanderRow>>>,
    module_controls: RefCell<Vec<glib::WeakRef<gtk4::Widget>>>,
    revert_controls: RefCell<Vec<glib::WeakRef<gtk4::Widget>>>,
    run: RefCell<Option<RunCallback>>,
    revert: RefCell<Option<RevertCallback>>,
}

impl DoctorPreferenceControls {
    pub(in crate::ui) fn clear_surfaces(&self) {
        self.expander_rows.borrow_mut().clear();
        self.module_controls.borrow_mut().clear();
        self.revert_controls.borrow_mut().clear();
    }

    pub(in crate::ui) fn set_callbacks(
        &self,
        run: impl Fn(u32) + 'static,
        revert: impl Fn() + 'static,
    ) {
        self.run.borrow_mut().replace(Rc::new(run));
        self.revert.borrow_mut().replace(Rc::new(revert));
    }

    fn register_expander(&self, row: &adw::ExpanderRow) {
        self.expander_rows.borrow_mut().push(row.downgrade());
    }

    fn register_module_control(&self, control: &impl IsA<gtk4::Widget>) {
        self.module_controls
            .borrow_mut()
            .push(control.upcast_ref::<gtk4::Widget>().downgrade());
    }

    fn register_revert_control(&self, control: &impl IsA<gtk4::Widget>) {
        self.revert_controls
            .borrow_mut()
            .push(control.upcast_ref::<gtk4::Widget>().downgrade());
    }

    pub(in crate::ui) fn set_job_running(&self, running: bool) {
        let state = control_state(running);
        retain_apply(&self.expander_rows, |row| row.set_subtitle(&state.subtitle));
        retain_apply(&self.module_controls, |control| {
            control.set_sensitive(state.remote_sensitive);
        });
        retain_apply(&self.revert_controls, |control| {
            control.set_sensitive(state.revert_sensitive);
        });
    }
}

fn retain_apply<T: glib::object::IsA<glib::Object>>(
    controls: &RefCell<Vec<glib::WeakRef<T>>>,
    apply: impl Fn(&T),
) {
    controls
        .borrow_mut()
        .retain(|target| match target.upgrade() {
            Some(control) => {
                apply(&control);
                true
            }
            None => false,
        });
}

pub(super) struct DoctorPluginControlState {
    pub(super) remote_sensitive: bool,
    pub(super) revert_sensitive: bool,
    pub(super) subtitle: String,
}

pub(super) fn control_state(job_running: bool) -> DoctorPluginControlState {
    let description = crate::ui::preference_plugins::plugin_description(
        &reprise_core::modules::LIBRARY_DOCTOR_MODULE,
    );
    DoctorPluginControlState {
        remote_sensitive: !job_running,
        revert_sensitive: !job_running,
        subtitle: if job_running {
            format!(
                "{} · {}",
                description,
                strings::text(strings::DOCTOR_CONTROLS_LOCKED)
            )
        } else {
            description
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteToggleAction {
    Disable,
    Enable,
    Confirm,
}

pub(super) const fn remote_toggle_action(
    requested_active: bool,
    preference: reprise_core::library_doctor::RemoteSuggestionPreference,
) -> RemoteToggleAction {
    if !requested_active {
        RemoteToggleAction::Disable
    } else if preference.consent_required {
        RemoteToggleAction::Confirm
    } else {
        RemoteToggleAction::Enable
    }
}

pub(super) fn remote_suggestions_row(context: &Rc<PreferencesContext>) -> adw::SwitchRow {
    let parent = context.preferences_parent();
    remote_suggestions_row_for(&context.conn, &parent, Rc::new(|_| {}))
}

pub(in crate::ui) fn remote_suggestions_row_for(
    conn: &Rc<Db>,
    parent: &impl IsA<gtk4::Widget>,
    on_changed: Rc<dyn Fn(bool)>,
) -> adw::SwitchRow {
    let preference = reprise_core::library_doctor::remote_suggestion_preference(conn).unwrap_or(
        reprise_core::library_doctor::RemoteSuggestionPreference {
            enabled: false,
            consent_required: true,
        },
    );
    let row = adw::SwitchRow::builder()
        .title(strings::text(strings::LIBRARY_DOCTOR_REMOTE))
        .subtitle(strings::text(strings::LIBRARY_DOCTOR_REMOTE_DESCRIPTION))
        .use_markup(false)
        .active(preference.enabled)
        .build();
    let syncing = Rc::new(Cell::new(false));
    let conn = conn.clone();
    let parent = parent.upcast_ref::<gtk4::Widget>().downgrade();
    let syncing_notify = syncing.clone();
    row.connect_active_notify(move |row| {
        if syncing_notify.get() {
            return;
        }
        let preference = reprise_core::library_doctor::remote_suggestion_preference(&conn)
            .unwrap_or(reprise_core::library_doctor::RemoteSuggestionPreference {
                enabled: false,
                consent_required: true,
            });
        match remote_toggle_action(row.is_active(), preference) {
            RemoteToggleAction::Disable => {
                let result = {
                    let conn = &conn;
                    reprise_core::library_doctor::disable_remote_suggestions(conn)
                };
                if let Err(error) = result {
                    tracing::warn!(%error, "could not disable Library Doctor remote suggestions");
                    set_active_without_notify(row, &syncing_notify, true);
                } else {
                    on_changed(false);
                }
            }
            RemoteToggleAction::Enable => {
                let result = {
                    let conn = &conn;
                    reprise_core::library_doctor::accept_remote_suggestions(conn)
                };
                if let Err(error) = result {
                    tracing::warn!(%error, "could not enable Library Doctor remote suggestions");
                    set_active_without_notify(row, &syncing_notify, false);
                } else {
                    on_changed(true);
                }
            }
            RemoteToggleAction::Confirm => {
                set_active_without_notify(row, &syncing_notify, false);
                let Some(parent) = parent.upgrade() else {
                    return;
                };
                present_remote_confirmation(&conn, &parent, row, &syncing_notify, &on_changed);
            }
        }
    });
    row
}

fn set_active_without_notify(row: &adw::SwitchRow, syncing: &Cell<bool>, active: bool) {
    syncing.set(true);
    row.set_active(active);
    syncing.set(false);
}

fn present_remote_confirmation(
    conn: &Rc<Db>,
    parent: &gtk4::Widget,
    row: &adw::SwitchRow,
    syncing: &Rc<Cell<bool>>,
    on_changed: &Rc<dyn Fn(bool)>,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(strings::text(strings::LIBRARY_DOCTOR_REMOTE_HEADING))
        .body(strings::text(strings::LIBRARY_DOCTOR_REMOTE_BODY))
        .default_response(RESPONSE_CANCEL)
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
    dialog.add_response(
        RESPONSE_ENABLE,
        &strings::text(strings::LIBRARY_DOCTOR_REMOTE_ENABLE),
    );
    dialog.set_response_appearance(RESPONSE_ENABLE, adw::ResponseAppearance::Suggested);
    let conn = conn.clone();
    let row = row.clone();
    let syncing = syncing.clone();
    let on_changed = on_changed.clone();
    dialog.choose(Some(parent), gio::Cancellable::NONE, move |response| {
        if response != RESPONSE_ENABLE {
            return;
        }
        let result = {
            let conn = &conn;
            reprise_core::library_doctor::accept_remote_suggestions(conn)
        };
        match result {
            Ok(()) => {
                set_active_without_notify(&row, &syncing, true);
                on_changed(true);
            }
            Err(error) => {
                tracing::warn!(%error, "could not record Library Doctor remote consent");
            }
        }
    });
}

pub(in crate::ui) fn plugin_row(context: &Rc<PreferencesContext>) -> adw::ExpanderRow {
    let state = control_state(context.library_doctor_job_running.get());
    let row = adw::ExpanderRow::builder()
        .title(strings::text(strings::LIBRARY_DOCTOR))
        .subtitle(&state.subtitle)
        .enable_expansion(true)
        .build();

    let scope = adw::ComboRow::builder()
        .title(strings::text(strings::DOCTOR_SCOPE))
        .model(&gtk4::StringList::new(&[
            &strings::text(strings::DOCTOR_SCOPE_WHOLE_LIBRARY),
            &strings::text(strings::DOCTOR_SCOPE_CURRENT_VIEW),
            &strings::text(strings::DOCTOR_SCOPE_SELECTION),
        ]))
        .sensitive(state.remote_sensitive)
        .build();
    row.add_row(&scope);

    let remote = remote_suggestions_row(context);
    remote.set_sensitive(state.remote_sensitive);
    row.add_row(&remote);

    let local_hint = adw::ActionRow::builder()
        .subtitle(strings::text(strings::DOCTOR_LOCAL_ALWAYS_INCLUDED))
        .build();
    local_hint.add_css_class("property");
    row.add_row(&local_hint);

    let run = gtk4::Button::builder()
        .label(strings::text(strings::DOCTOR_RUN_SCAN))
        .css_classes(["suggested-action"])
        .sensitive(state.remote_sensitive)
        .valign(gtk4::Align::Center)
        .build();
    let run_row = adw::ActionRow::builder()
        .title(strings::text(strings::DOCTOR_RUN_SCAN))
        .activatable_widget(&run)
        .build();
    run_row.add_suffix(&run);
    row.add_row(&run_row);

    let cleanup_available = {
        let conn = &context.conn;
        reprise_core::library_doctor::LibraryDoctor::new(conn)
            .last_cleanup()
            .ok()
            .flatten()
            .is_some()
    };
    let revert = gtk4::Button::builder()
        .label(strings::text(strings::DOCTOR_REVERT_LAST_CLEANUP))
        .sensitive(!context.library_doctor_job_running.get())
        .valign(gtk4::Align::Center)
        .build();
    let revert_row = adw::ActionRow::builder()
        .title(strings::text(strings::DOCTOR_REVERT_LAST_CLEANUP))
        .activatable_widget(&revert)
        .visible(cleanup_available)
        .build();
    revert_row.add_suffix(&revert);
    row.add_row(&revert_row);

    let target = glib::WeakRef::new();
    target.set(Some(row.upcast_ref::<gtk4::Widget>()));
    context
        .plugin_rows
        .borrow_mut()
        .insert("library_doctor", target);
    context.doctor_controls.register_expander(&row);
    for control in [
        scope.clone().upcast::<gtk4::Widget>(),
        remote.clone().upcast(),
        run.clone().upcast(),
    ] {
        context.doctor_controls.register_module_control(&control);
    }
    context.doctor_controls.register_revert_control(&revert);

    {
        let weak = Rc::downgrade(context);
        run.connect_clicked(move |_| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let callback = context.doctor_controls.run.borrow().clone();
            let selected = scope.selected();
            context.close_for_main_navigation();
            if let Some(callback) = callback {
                callback(selected);
            }
        });
    }
    {
        let weak = Rc::downgrade(context);
        revert.connect_clicked(move |_| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let callback = context.doctor_controls.revert.borrow().clone();
            context.close_for_main_navigation();
            if let Some(callback) = callback {
                callback();
            }
        });
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_7a_first_remote_enable_requires_confirmation_and_cancel_stays_off() {
        let consent_required = reprise_core::library_doctor::RemoteSuggestionPreference {
            enabled: false,
            consent_required: true,
        };
        let consented = reprise_core::library_doctor::RemoteSuggestionPreference {
            enabled: false,
            consent_required: false,
        };

        assert_eq!(
            remote_toggle_action(true, consent_required),
            RemoteToggleAction::Confirm
        );
        assert_eq!(
            remote_toggle_action(true, consented),
            RemoteToggleAction::Enable
        );
        assert_eq!(
            remote_toggle_action(false, consented),
            RemoteToggleAction::Disable
        );
    }
}
