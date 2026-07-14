//! Visibility binding for settings that only apply while a service is on.

use gtk4::prelude::*;
use libadwaita as adw;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn dependent_service_row_follows_its_toggle_live() {
        gtk4::init().unwrap();
        let service = adw::SwitchRow::builder().active(false).build();
        let account = adw::ActionRow::new();

        bind_visibility(&service, &account);
        assert!(!account.is_visible());

        service.set_active(true);
        assert!(account.is_visible());

        service.set_active(false);
        assert!(!account.is_visible());

        let account_weak = account.downgrade();
        drop(account);
        assert!(account_weak.upgrade().is_none());
    }
}
