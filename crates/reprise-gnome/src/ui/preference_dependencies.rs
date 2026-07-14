//! Visibility binding for settings that only apply while a service is on.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

pub(super) fn bind_visibility(service: &adw::SwitchRow, dependent: &impl IsA<gtk4::Widget>) {
    let dependent = dependent.as_ref();
    dependent.set_visible(service.is_active());
    let dependent = dependent.downgrade();
    service.connect_active_notify(move |service| {
        if let Some(dependent) = dependent.upgrade() {
            dependent.set_visible(service.is_active());
        }
    });
}

pub(super) fn add_configure_button(service: &adw::SwitchRow, label: &str) -> gtk4::Button {
    let button = gtk4::Button::builder()
        .label(label)
        .valign(gtk4::Align::Center)
        .build();
    service.add_suffix(&button);
    bind_visibility(service, &button);
    button
}

pub(super) fn service_subtitle(description: &str, enabled: bool, status: &str) -> String {
    if enabled {
        format!("{description} · {status}")
    } else {
        description.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn configure_button_lives_in_service_row_and_follows_its_toggle() {
        gtk4::init().unwrap();
        let service = adw::SwitchRow::builder().active(false).build();
        let configure = add_configure_button(&service, "Configure…");

        assert_eq!(configure.label().as_deref(), Some("Configure…"));
        assert!(configure.parent().is_some());
        assert!(!configure.is_visible());

        service.set_active(true);
        assert!(configure.is_visible());

        service.set_active(false);
        assert!(!configure.is_visible());

        let configure_weak = configure.downgrade();
        service.remove(&configure);
        drop(configure);
        assert!(configure_weak.upgrade().is_none());
    }

    #[test]
    fn service_status_is_only_shown_while_enabled() {
        assert_eq!(
            service_subtitle("Scrobble listens", false, "Connected as Ada"),
            "Scrobble listens"
        );
        assert_eq!(
            service_subtitle("Scrobble listens", true, "Connected as Ada"),
            "Scrobble listens · Connected as Ada"
        );
    }
}
