use super::*;

pub(super) fn wire_deep_link(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let RuntimeWiring {
        preferences,
        radio_view,
        window,
        minimal_view,
        ..
    } = *w;
    // `RAD-5`: "Near you" without a stored location hands off to the
    // location setting in Preferences, the same deep-link shape
    // `present_rhythmbox_import` above already uses.
    let deep_link_preferences = Rc::downgrade(preferences);
    radio_view.on_materialized(move |radio| {
        radio.set_on_location_settings(move || {
            if let Some(preferences) = deep_link_preferences.upgrade() {
                preferences.present_location_settings();
            }
        });
    });
    scratch.active_content_focus.focus_later_if_unset(window);
    minimal_view.apply_initial();
    super::startup_report::mark("minimal_view::apply_initial");
    super::window_smoke::arm_quit(window);
    super::startup_quiet::arm(window);
}
