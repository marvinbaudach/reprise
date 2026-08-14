//! App-wide location preferences shared by Concerts, Radio, and Podcasts.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::view_source::ViewSource;

use super::PreferencesContext;
use crate::ui::location_broadcast::LocationBroadcast;
use crate::ui::{one_shot_task, strings};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_SET: &str = "set";

type OnOpen = Rc<dyn Fn(ViewSource)>;
pub(in crate::ui) const LOCATION_TARGET_CLASS: &str = "reprise-location-target";

#[derive(Clone, Debug, PartialEq)]
enum LocationDecision {
    Store {
        latitude: f64,
        longitude: f64,
        name: String,
        country_code: Option<String>,
    },
    Error(String),
}

fn geocode_decision(
    result: Result<Option<reprise_core::concerts::GeocodedLocation>, String>,
) -> LocationDecision {
    match result {
        Ok(Some(location)) => LocationDecision::Store {
            latitude: location.lat,
            longitude: location.lon,
            name: location.display_name,
            country_code: location.country_code,
        },
        Ok(None) | Err(_) => LocationDecision::Error(strings::text(strings::LOCATION_NOT_FOUND)),
    }
}

fn portal_decision(
    result: &Result<reprise_platform_linux::location::PortalLocation, String>,
) -> LocationDecision {
    match result {
        Ok(location) => LocationDecision::Store {
            latitude: location.latitude,
            longitude: location.longitude,
            name: strings::text(strings::LOCATION_CURRENT_LOCATION),
            // The portal returns coordinates only. O-4 forbids a separate
            // reverse-geocoding request, so this stays honestly countryless.
            country_code: None,
        },
        Err(_) => LocationDecision::Error(strings::text(strings::LOCATION_NOT_FOUND)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CurrentLocationButtonState {
    sensitive: bool,
    show_spinner: bool,
}

fn current_location_button_state(pending: bool) -> CurrentLocationButtonState {
    CurrentLocationButtonState {
        sensitive: !pending,
        show_spinner: pending,
    }
}

fn set_current_location_pending(button: &gtk4::Button, pending: bool) {
    let state = current_location_button_state(pending);
    button.set_sensitive(state.sensitive);
    if state.show_spinner {
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let spinner = gtk4::Spinner::new();
        spinner.start();
        content.append(&spinner);
        content.append(&gtk4::Label::new(Some(&strings::text(
            strings::LOCATION_USE_CURRENT_LOCATION,
        ))));
        button.set_child(Some(&content));
    } else {
        button.set_label(&strings::text(strings::LOCATION_USE_CURRENT_LOCATION));
    }
}

struct LocationPageSurface {
    page: adw::PreferencesPage,
    city: adw::ActionRow,
    #[cfg(test)]
    radius: adw::ComboRow,
    #[cfg(test)]
    used_by: [adw::ActionRow; 3],
}

fn build_surface(
    conn: &Rc<Db>,
    broadcast: &Rc<LocationBroadcast>,
    on_open: &OnOpen,
) -> LocationPageSurface {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::PREFERENCES_LOCATION))
        .icon_name("find-location-symbolic")
        .build();
    let location_group = adw::PreferencesGroup::builder()
        .description(strings::text(strings::LOCATION_INTRO))
        .build();
    let city = city_row(conn, broadcast);
    let radius = radius_row(conn, broadcast);
    location_group.add(&city);
    location_group.add(&radius);
    page.add(&location_group);

    let used_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::LOCATION_USED_BY))
        .build();
    let badge = gtk4::Label::new(Some("3"));
    badge.add_css_class("accent");
    badge.add_css_class("pill");
    used_group.set_header_suffix(Some(&badge));
    let location = reprise_core::location::app_location(conn).ok().flatten();
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".to_owned());
    let podcast_country = crate::ui::podcasts::dialog_country(location.as_ref(), &locale);
    let used_by = [
        used_by_row(
            strings::text(strings::CONCERTS),
            strings::text(strings::LOCATION_CONCERTS_DESCRIPTION),
            ViewSource::Concerts,
            on_open,
        ),
        used_by_row(
            strings::text(strings::LOCATION_RADIO_NEAR_YOU),
            strings::text(strings::LOCATION_RADIO_DESCRIPTION),
            ViewSource::Radio,
            on_open,
        ),
        used_by_row(
            strings::location_podcasts_popular_in(&podcast_country),
            strings::text(strings::LOCATION_PODCASTS_DESCRIPTION),
            ViewSource::Podcasts,
            on_open,
        ),
    ];
    for row in &used_by {
        used_group.add(row);
    }
    page.add(&used_group);

    let footnote_group = adw::PreferencesGroup::new();
    let footnote = gtk4::Label::new(Some(&strings::text(strings::LOCATION_FOOTNOTE)));
    footnote.add_css_class("caption");
    footnote.add_css_class("dim-label");
    footnote.set_wrap(true);
    footnote.set_xalign(0.0);
    footnote.set_margin_start(12);
    footnote.set_margin_end(12);
    footnote_group.add(&footnote);
    page.add(&footnote_group);

    LocationPageSurface {
        page,
        city,
        #[cfg(test)]
        radius,
        #[cfg(test)]
        used_by,
    }
}

fn city_row(conn: &Rc<Db>, broadcast: &Rc<LocationBroadcast>) -> adw::ActionRow {
    let stored = reprise_core::location::app_location(conn).ok().flatten();
    let city = adw::ActionRow::builder()
        .title(strings::text(strings::LOCATION_CITY))
        .subtitle(
            stored
                .as_ref()
                .map_or_else(strings::location_not_set, |location| location.name.clone()),
        )
        .build();
    // a11y-semantics: role=row name=city state=location-summary action=focus/edit
    city.set_focusable(true);
    let edit = gtk4::Button::from_icon_name("document-edit-symbolic");
    edit.add_css_class("flat");
    edit.set_valign(gtk4::Align::Center);
    let edit_label = strings::text(strings::LOCATION_EDIT_CITY);
    edit.set_tooltip_text(Some(&edit_label));
    edit.update_property(&[gtk4::accessible::Property::Label(&edit_label)]);
    let current = gtk4::Button::builder()
        .label(strings::text(strings::LOCATION_USE_CURRENT_LOCATION))
        .valign(gtk4::Align::Center)
        .build();
    let clear = gtk4::Button::builder()
        .label(strings::text(strings::LOCATION_CLEAR_LOCATION))
        .valign(gtk4::Align::Center)
        .build();
    city.add_suffix(&edit);
    city.add_suffix(&current);
    city.add_suffix(&clear);

    {
        let conn = conn.clone();
        let broadcast = broadcast.clone();
        let city = city.clone();
        edit.connect_clicked(move |button| {
            let initial = reprise_core::location::app_location(&conn)
                .ok()
                .flatten()
                .map_or_else(String::new, |location| location.name);
            let conn = conn.clone();
            let broadcast = broadcast.clone();
            let city = city.clone();
            present_city_editor(button.upcast_ref(), &initial, move |query| {
                let receiver = one_shot_task::spawn("reprise-geocode", move || {
                    geocode_decision(
                        reprise_core::concerts::geocode(&query).map_err(|error| error.to_string()),
                    )
                });
                receive_location(
                    receiver,
                    conn.clone(),
                    broadcast.clone(),
                    city.clone(),
                    None,
                );
            });
        });
    }
    {
        let conn = conn.clone();
        let broadcast = broadcast.clone();
        let city = city.clone();
        let pending = Rc::new(Cell::new(false));
        current.connect_clicked(move |button| {
            if pending.replace(true) {
                return;
            }
            set_current_location_pending(button, true);
            let receiver = one_shot_task::spawn("reprise-location", || {
                portal_decision(&reprise_platform_linux::location::current_location(
                    reprise_platform_linux::location::DEFAULT_TIMEOUT,
                ))
            });
            let button = button.clone();
            let pending = pending.clone();
            receive_location(
                receiver,
                conn.clone(),
                broadcast.clone(),
                city.clone(),
                Some(Box::new(move || {
                    pending.set(false);
                    set_current_location_pending(&button, false);
                })),
            );
        });
    }
    {
        let conn = conn.clone();
        let broadcast = broadcast.clone();
        let city = city.clone();
        clear.connect_clicked(move |_| match reprise_core::location::clear(&conn) {
            Ok(()) => {
                city.set_subtitle(&strings::location_not_set());
                broadcast.notify();
            }
            Err(error) => tracing::warn!(%error, "could not clear app location"),
        });
    }
    city
}

fn present_city_editor(parent: &gtk4::Widget, initial: &str, on_submit: impl Fn(String) + 'static) {
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(parent);
    let entry = gtk4::Entry::builder()
        .text(initial)
        .placeholder_text(strings::text(strings::LOCATION_CITY))
        .activates_default(true)
        .build();
    let dialog = adw::AlertDialog::builder()
        .heading(strings::text(strings::LOCATION_EDIT_CITY))
        .default_response(RESPONSE_SET)
        .close_response(RESPONSE_CANCEL)
        .extra_child(&entry)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
    dialog.add_response(RESPONSE_SET, &strings::text(strings::LOCATION_SET_CITY));
    dialog.set_response_appearance(RESPONSE_SET, adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled(RESPONSE_SET, !initial.trim().is_empty());
    entry.connect_changed({
        let dialog = dialog.clone();
        move |entry| dialog.set_response_enabled(RESPONSE_SET, !entry.text().trim().is_empty())
    });
    dialog.choose(Some(parent), gio::Cancellable::NONE, move |response| {
        focus_guard.restore();
        if response.as_str() == RESPONSE_SET {
            on_submit(entry.text().trim().to_owned());
        }
    });
}

fn receive_location(
    receiver: std::io::Result<async_channel::Receiver<LocationDecision>>,
    conn: Rc<Db>,
    broadcast: Rc<LocationBroadcast>,
    city: adw::ActionRow,
    on_complete: Option<Box<dyn FnOnce()>>,
) {
    let Ok(receiver) = receiver else {
        apply_location(
            &conn,
            &broadcast,
            &city,
            LocationDecision::Error(strings::text(strings::LOCATION_NOT_FOUND)),
        );
        if let Some(on_complete) = on_complete {
            on_complete();
        }
        return;
    };
    glib::spawn_future_local(async move {
        let decision = receiver.recv().await.unwrap_or_else(|_| {
            LocationDecision::Error(strings::text(strings::LOCATION_NOT_FOUND))
        });
        apply_location(&conn, &broadcast, &city, decision);
        if let Some(on_complete) = on_complete {
            on_complete();
        }
    });
}

fn apply_location(
    conn: &Db,
    broadcast: &LocationBroadcast,
    city: &adw::ActionRow,
    decision: LocationDecision,
) {
    match decision {
        LocationDecision::Store {
            latitude,
            longitude,
            name,
            country_code,
        } => match reprise_core::location::store(
            conn,
            latitude,
            longitude,
            &name,
            country_code.as_deref(),
        ) {
            Ok(()) => {
                city.set_subtitle(&name);
                broadcast.notify();
            }
            Err(error) => tracing::warn!(%error, "could not save app location"),
        },
        LocationDecision::Error(error) => city.set_subtitle(&error),
    }
}

fn radius_row(conn: &Rc<Db>, broadcast: &Rc<LocationBroadcast>) -> adw::ComboRow {
    let radii = reprise_core::location::RADIUS_PRESETS_KM;
    let labels = radii
        .iter()
        .copied()
        .map(strings::location_radius_km)
        .collect::<Vec<_>>();
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk4::StringList::new(&label_refs);
    let stored = reprise_core::location::default_radius_km(conn)
        .unwrap_or(reprise_core::location::DEFAULT_RADIUS_KM) as u32;
    let selected = radii
        .iter()
        .position(|radius| *radius == stored)
        .unwrap_or(radii.len() - 1) as u32;
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::LOCATION_DEFAULT_RADIUS))
        .model(&model)
        .selected(selected)
        .build();
    let conn = conn.clone();
    let broadcast = broadcast.clone();
    row.connect_selected_notify(move |row| {
        let Some(radius) = radii.get(row.selected() as usize).copied() else {
            return;
        };
        match reprise_core::location::set_default_radius_km(&conn, f64::from(radius)) {
            Ok(()) => broadcast.notify(),
            Err(error) => tracing::warn!(%error, "could not save default location radius"),
        }
    });
    row
}

fn used_by_row(
    title: String,
    subtitle: String,
    source: ViewSource,
    on_open: &OnOpen,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .activatable(true)
        .build();
    row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    let on_open = on_open.clone();
    row.connect_activated(move |_| on_open(source.clone()));
    row
}

fn focus_city_row(city: &adw::ActionRow, highlight: bool) {
    if highlight {
        city.add_css_class(LOCATION_TARGET_CLASS);
        let target = city.downgrade();
        glib::timeout_add_local_once(super::preference_plugins::highlight_duration(), move || {
            if let Some(city) = target.upgrade() {
                city.remove_css_class(LOCATION_TARGET_CLASS);
            }
        });
    }
    let target = city.downgrade();
    glib::idle_add_local_once(move || {
        if let Some(city) = target.upgrade() {
            city.grab_focus();
        }
    });
}

impl PreferencesContext {
    pub(in crate::ui) fn location_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let context = Rc::downgrade(self);
        let on_open: OnOpen = Rc::new(move |source| {
            let Some(context) = context.upgrade() else {
                return;
            };
            if let Some(dialog) = context.preferences_dialog() {
                dialog.force_close();
            }
            context
                .sidebar
                .refresh_and_select(source, "Location used-by row");
        });
        let surface = build_surface(&self.conn, &self.location_broadcast, &on_open);
        self.location_city_row.borrow().set(Some(&surface.city));
        surface.page
    }

    pub(in crate::ui) fn focus_location_city(&self) {
        let city = self.location_city_row.borrow().upgrade();
        let Some(city) = city else {
            return;
        };
        let highlight = reprise_core::location::app_location(&self.conn)
            .ok()
            .flatten()
            .is_none();
        focus_city_row(&city, highlight);
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::PreferencesRowExt;

    use super::*;

    #[test]
    fn location_apply_decisions_store_success_and_keep_errors_visible() {
        assert_eq!(
            geocode_decision(Ok(Some(reprise_core::concerts::GeocodedLocation {
                lat: 48.137,
                lon: 11.575,
                display_name: "Munich, Bavaria".into(),
                country_code: Some("DE".into()),
            }))),
            LocationDecision::Store {
                latitude: 48.137,
                longitude: 11.575,
                name: "Munich, Bavaria".into(),
                country_code: Some("DE".into()),
            }
        );
        assert!(matches!(
            geocode_decision(Ok(None)),
            LocationDecision::Error(_)
        ));
        assert_eq!(
            portal_decision(&Ok(reprise_platform_linux::location::PortalLocation {
                latitude: 47.376,
                longitude: 8.541,
                accuracy_m: Some(1_000.0),
            })),
            LocationDecision::Store {
                latitude: 47.376,
                longitude: 8.541,
                name: strings::text(strings::LOCATION_CURRENT_LOCATION),
                country_code: None,
            }
        );
    }

    #[test]
    fn current_location_button_is_disabled_with_pending_feedback() {
        assert_eq!(
            current_location_button_state(false),
            CurrentLocationButtonState {
                sensitive: true,
                show_spinner: false,
            }
        );
        assert_eq!(
            current_location_button_state(true),
            CurrentLocationButtonState {
                sensitive: false,
                show_spinner: true,
            }
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_15_location_page_owns_city_radius_and_all_three_named_readers() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let broadcast = Rc::new(LocationBroadcast::default());
        let on_open: OnOpen = Rc::new(|_| {});
        let surface = build_surface(&conn, &broadcast, &on_open);

        assert_eq!(surface.city.title(), "City");
        assert_eq!(surface.city.subtitle().as_deref(), Some("Not set"));
        assert_eq!(surface.radius.title(), "Default radius");
        assert_eq!(surface.radius.selected(), 3);
        assert_eq!(
            surface
                .used_by
                .iter()
                .map(PreferencesRowExt::title)
                .collect::<Vec<_>>(),
            ["Concerts", "Radio · Near you", "Podcasts · Popular in US",]
        );

        let mut index = Vec::new();
        super::super::preferences_search_index::collect_rows(
            surface.page.upcast_ref(),
            super::super::preferences_window::PageId::Location,
            &mut index,
        );
        let radius_hits = index
            .iter()
            .filter(|row| row.document.title == "Default radius" && row.matches("radius"))
            .count();
        assert_eq!(
            radius_hits, 1,
            "the real Location page must index its radius row"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn missing_location_target_focuses_city_and_removes_its_brief_highlight() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let broadcast = Rc::new(LocationBroadcast::default());
        let on_open: OnOpen = Rc::new(|_| {});
        let surface = build_surface(&conn, &broadcast, &on_open);
        let window = gtk4::Window::builder().child(&surface.page).build();
        window.present();

        focus_city_row(&surface.city, true);
        assert!(surface.city.has_css_class(LOCATION_TARGET_CLASS));
        let main_loop = glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        glib::timeout_add_local_once(
            super::super::preference_plugins::highlight_duration()
                + std::time::Duration::from_millis(30),
            move || quit.quit(),
        );
        main_loop.run();

        assert!(surface.city.has_focus());
        assert!(!surface.city.has_css_class(LOCATION_TARGET_CLASS));
        window.close();
    }
}
