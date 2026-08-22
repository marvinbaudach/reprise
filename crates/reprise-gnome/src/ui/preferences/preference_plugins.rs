use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::modules::ModuleDescriptor;

use super::preference_plugin_chrome as chrome;
use super::{strings, PreferencesContext};

pub(in crate::ui) const TARGET_CLASS: &str = "reprise-plugin-target";
pub(in crate::ui) const ONLINE_LYRICS_TARGETS: &[&str] = &["online_lyrics"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PluginGroup {
    Local,
    Online,
    Connected,
}

const LOCAL_PLUGIN_IDS: &[&str] = &["song_visuals"];
const ONLINE_PLUGIN_IDS: &[&str] = &[
    "artwork",
    "online_lyrics",
    "concerts",
    "new_releases",
    "youtube",
    "podcasts",
    "radio",
];
const CONNECTED_PLUGIN_IDS: &[&str] = &["scrobbling"];

fn plugin_ids_for_group(group: PluginGroup) -> &'static [&'static str] {
    match group {
        PluginGroup::Local => LOCAL_PLUGIN_IDS,
        PluginGroup::Online => ONLINE_PLUGIN_IDS,
        PluginGroup::Connected => CONNECTED_PLUGIN_IDS,
    }
}

fn plugin_uses_expander(id: &str) -> bool {
    matches!(
        id,
        "youtube" | "podcasts" | "radio" | "new_releases" | "concerts"
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CollapsedGroupState {
    rows_visible: bool,
    rows_sensitive: bool,
    disclosure_visible: bool,
    disclosure_label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollapsiblePluginGroup {
    OnlineContent,
    ConnectedServices,
}

fn collapsed_group_state(
    group: CollapsiblePluginGroup,
    master_enabled: bool,
    revealed_while_disabled: bool,
    row_count: usize,
) -> CollapsedGroupState {
    CollapsedGroupState {
        rows_visible: master_enabled || revealed_while_disabled,
        rows_sensitive: master_enabled,
        disclosure_visible: !master_enabled && !revealed_while_disabled,
        disclosure_label: match group {
            CollapsiblePluginGroup::OnlineContent => {
                strings::online_content_show_sources(row_count)
            }
            CollapsiblePluginGroup::ConnectedServices => {
                strings::text(strings::SCROBBLING_NEEDS_ONLINE_SOURCES)
            }
        },
    }
}

fn apply_collapsed_group(
    rows: &[gtk4::Widget],
    disclosure: &adw::ActionRow,
    state: &CollapsedGroupState,
) {
    disclosure.set_title(&state.disclosure_label);
    disclosure.set_visible(state.disclosure_visible);
    for row in rows {
        row.set_visible(state.rows_visible);
        row.set_sensitive(state.rows_sensitive);
    }
}

fn descriptor(id: &str) -> &'static ModuleDescriptor {
    reprise_core::modules::ALL_MODULES
        .iter()
        .copied()
        .find(|descriptor| descriptor.id == id)
        .unwrap_or_else(|| panic!("unknown Plugins capability: {id}"))
}

pub(in crate::ui) fn highlight_duration() -> std::time::Duration {
    std::time::Duration::from_millis(u64::from(crate::ui::motion::AMBIENT_MS))
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{TARGET_CLASS}, .{} {{ \
           background-color: alpha(@accent_bg_color, 0.22); \
           box-shadow: inset 3px 0 @accent_color; \
           transition: background-color {}ms {}, box-shadow {}ms {}; }} \
         .{} {{ background-color: alpha(@card_bg_color, 0.55); }} {}",
        super::preference_location::LOCATION_TARGET_CLASS,
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING,
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING,
        super::preference_concerts::LOCATION_REFERENCE_CLASS,
        chrome::css(),
    )
}

pub(in crate::ui) fn plugin_applies_live(descriptor: &ModuleDescriptor) -> bool {
    descriptor.applies_live
}

pub(in crate::ui) fn plugin_title(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::LISTENBRAINZ,
        "lastfm" => strings::LASTFM,
        "new_releases" => strings::NEW_RELEASES,
        "concerts" => strings::CONCERTS,
        "youtube" => strings::YOUTUBE,
        "podcasts" => strings::PODCASTS,
        "radio" => strings::RADIO,
        "library_doctor" => strings::LIBRARY_DOCTOR,
        "artwork" => strings::ARTWORK,
        "online_lyrics" => strings::ONLINE_LYRICS,
        "song_visuals" => strings::SONG_VISUALS,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

pub(in crate::ui) fn plugin_description(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::PLUGIN_LISTENBRAINZ_DESCRIPTION,
        "lastfm" => strings::PLUGIN_LASTFM_DESCRIPTION,
        "new_releases" => strings::NEW_RELEASES_DESCRIPTION,
        "concerts" => strings::CONCERTS_DESCRIPTION,
        "youtube" => strings::ONLINE_SOURCES_YOUTUBE_SUBTITLE,
        "podcasts" => strings::ONLINE_SOURCES_PODCASTS_SUBTITLE,
        "radio" => strings::ONLINE_SOURCES_RADIO_SUBTITLE,
        "artwork" => strings::ARTWORK_DESCRIPTION,
        "online_lyrics" => strings::ONLINE_LYRICS_DESCRIPTION,
        "song_visuals" => strings::SONG_VISUALS_DESCRIPTION,
        _ => return descriptor.description.to_string(),
    };
    strings::text(message)
}

fn register_plugin_row(
    context: &PreferencesContext,
    descriptor: &'static ModuleDescriptor,
    row: &impl IsA<gtk4::Widget>,
) {
    let target = glib::WeakRef::new();
    target.set(Some(row.upcast_ref::<gtk4::Widget>()));
    context
        .plugin_rows
        .borrow_mut()
        .insert(descriptor.id, target);
}

fn wire_switch(
    context: &Rc<PreferencesContext>,
    descriptor: &'static ModuleDescriptor,
    row: &adw::SwitchRow,
) {
    let syncing = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(context);
    let syncing_notify = syncing.clone();
    row.connect_active_notify(move |row| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        if syncing_notify.get() {
            return;
        }
        let active = row.is_active();
        if let Err(error) = context.set_module_enabled(descriptor, active, "plugin toggled") {
            tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
            syncing_notify.set(true);
            row.set_active(!active);
            syncing_notify.set(false);
        }
    });
}

fn wire_expander(
    context: &Rc<PreferencesContext>,
    descriptor: &'static ModuleDescriptor,
    row: &adw::ExpanderRow,
    set_children_sensitive: Rc<dyn Fn(bool)>,
) {
    let syncing = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(context);
    let syncing_notify = syncing.clone();
    row.connect_enable_expansion_notify(move |row| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        if syncing_notify.get() {
            return;
        }
        let active = row.enables_expansion();
        if let Err(error) = context.set_module_enabled(descriptor, active, "plugin toggled") {
            tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
            syncing_notify.set(true);
            row.set_enable_expansion(!active);
            syncing_notify.set(false);
            return;
        }
        set_children_sensitive(active);
    });

    if descriptor.id == "new_releases" {
        let alive = row.downgrade();
        let target = alive.clone();
        let syncing = syncing.clone();
        context.artist_news.subscribe_enabled(
            move || alive.upgrade().is_some(),
            move |enabled| {
                let Some(row) = target.upgrade() else {
                    return;
                };
                syncing.set(true);
                row.set_enable_expansion(enabled);
                syncing.set(false);
            },
        );
    }
    if descriptor.id == "concerts" {
        let alive = row.downgrade();
        let target = alive.clone();
        let syncing = syncing.clone();
        context.concerts.subscribe_enabled(
            move || alive.upgrade().is_some(),
            move |enabled| {
                let Some(row) = target.upgrade() else {
                    return;
                };
                syncing.set(true);
                row.set_enable_expansion(enabled);
                syncing.set(false);
            },
        );
    }
}

fn switch_alignment_placeholder() -> gtk4::Image {
    gtk4::Image::builder()
        .icon_name("pan-down-symbolic")
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .opacity(0.0)
        .can_target(false)
        .can_focus(false)
        .build()
}

fn aligned_switch_row(title: &str, subtitle: &str, active: bool) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .subtitle(subtitle)
        .use_markup(false)
        .active(active)
        .build();
    chrome::reserve_gutter(&row);
    row.add_suffix(&switch_alignment_placeholder());
    row
}

fn online_master_row(active: bool) -> adw::SwitchRow {
    let row = aligned_switch_row(
        &strings::text(strings::PLUGIN_GROUP_ONLINE_CONTENT),
        &strings::text(strings::ONLINE_CONTENT_MASTER_DESCRIPTION),
        active,
    );
    // The master carries the heading now, so it may neither be truncated nor
    // repeated by a group title above it: its description runs over the full
    // width instead.
    row.add_css_class(chrome::MASTER_ROW_CLASS);
    row.set_title_lines(0);
    row.set_subtitle_lines(0);
    row
}

fn online_group_with_master(master: &adw::SwitchRow) -> adw::PreferencesGroup {
    // No group title: `SET-11` makes the master the group's header, and the
    // heading must not stand twice.
    let group = adw::PreferencesGroup::new();
    group.add(master);
    group
}

fn simple_plugin_row(
    context: &Rc<PreferencesContext>,
    descriptor: &'static ModuleDescriptor,
) -> adw::SwitchRow {
    let description = plugin_description(descriptor);
    let subtitle = if plugin_applies_live(descriptor) {
        description
    } else {
        format!(
            "{} · {}",
            description,
            strings::text(strings::RESTART_REQUIRED)
        )
    };
    let active = reprise_core::modules::is_enabled(&context.conn, descriptor)
        .unwrap_or(descriptor.default_enabled);
    let row = aligned_switch_row(&plugin_title(descriptor), &subtitle, active);
    register_plugin_row(context, descriptor, &row);
    wire_switch(context, descriptor, &row);
    row
}

fn settings_plugin_row(
    context: &Rc<PreferencesContext>,
    descriptor: &'static ModuleDescriptor,
) -> adw::ExpanderRow {
    let active = reprise_core::modules::is_enabled(&context.conn, descriptor)
        .unwrap_or(descriptor.default_enabled);
    let row = adw::ExpanderRow::builder()
        .title(plugin_title(descriptor))
        .subtitle(plugin_description(descriptor))
        .show_enable_switch(true)
        .enable_expansion(active)
        .build();
    chrome::attach_chevron(&row);
    let set_children_sensitive: Rc<dyn Fn(bool)> = match descriptor.id {
        "youtube" => {
            let rows = super::preference_youtube::build(&context.conn, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "podcasts" => {
            let rows = super::preference_podcasts::build(&context.conn, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "radio" => {
            let rows = super::preference_radio::build(&context.conn, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "new_releases" => {
            let rows =
                super::preference_new_releases::build(&context.conn, &context.artist_news, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "concerts" => {
            let weak = Rc::downgrade(context);
            let on_location: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(context) = weak.upgrade() {
                    context.present_location_settings();
                }
            });
            let rows = super::preference_concerts::build(
                &context.conn,
                &context.concerts,
                &context.location_broadcast,
                &on_location,
                active,
            );
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        id => panic!("capability {id} has no Plugins child-row builder"),
    };
    register_plugin_row(context, descriptor, &row);
    wire_expander(context, descriptor, &row, set_children_sensitive);
    row
}

impl PreferencesContext {
    pub(in crate::ui) fn plugins_page(self: &Rc<Self>) -> adw::PreferencesPage {
        // TODO(package-B): remove these superseded catalog entries from the
        // shared string module it owns.
        let _legacy_group_catalog_entries = (
            strings::LOCAL_FEATURES,
            strings::ONLINE_CONTENT,
            strings::CONNECTED_SERVICES,
        );
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLUGINS))
            .icon_name("application-x-addon-symbolic")
            .build();
        page.add_css_class(chrome::FLAT_ROWS_CLASS);
        let local_group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::PLUGIN_GROUP_LOCAL))
            .build();
        let global_enabled = reprise_core::online_sources::is_enabled(&self.conn).unwrap_or(true);
        let online_master = online_master_row(global_enabled);
        let online_group = online_group_with_master(&online_master);
        let connected_group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::PLUGIN_GROUP_CONNECTED_SERVICES))
            .build();
        for id in plugin_ids_for_group(PluginGroup::Local) {
            match *id {
                id if plugin_uses_expander(id) => {
                    let row = settings_plugin_row(self, descriptor(id));
                    local_group.add(&row);
                }
                id => {
                    let row = simple_plugin_row(self, descriptor(id));
                    local_group.add(&row);
                }
            }
        }
        let online_disclosure = adw::ActionRow::builder().activatable(true).build();
        chrome::reserve_gutter(&online_disclosure);
        online_disclosure.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        online_group.add(&online_disclosure);
        let mut online_rows = Vec::with_capacity(ONLINE_PLUGIN_IDS.len());
        for id in plugin_ids_for_group(PluginGroup::Online) {
            let descriptor = descriptor(id);
            let row = if plugin_uses_expander(id) {
                let row = settings_plugin_row(self, descriptor);
                row.upcast::<gtk4::Widget>()
            } else {
                let row = simple_plugin_row(self, descriptor);
                row.upcast()
            };
            online_group.add(&row);
            online_rows.push(row);
        }
        let connected_disclosure = adw::ActionRow::builder()
            .title(strings::text(strings::SCROBBLING_NEEDS_ONLINE_SOURCES))
            .activatable(true)
            .build();
        chrome::reserve_gutter(&connected_disclosure);
        connected_disclosure.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        connected_group.add(&connected_disclosure);
        let connected_rows = plugin_ids_for_group(PluginGroup::Connected)
            .iter()
            .map(|id| match *id {
                "scrobbling" => {
                    let row = super::preference_scrobbling::build(self);
                    chrome::reserve_gutter(&row);
                    row.upcast::<gtk4::Widget>()
                }
                id => panic!("connected capability {id} has no Plugins row builder"),
            })
            .collect::<Vec<_>>();
        for row in &connected_rows {
            connected_group.add(row);
        }

        let online_rows = Rc::new(online_rows);
        let connected_rows = Rc::new(connected_rows);
        let reveal_online = Rc::new(Cell::new(
            !global_enabled
                && self
                    .pending_plugin_targets
                    .borrow()
                    .iter()
                    .any(|target| ONLINE_PLUGIN_IDS.contains(target)),
        ));
        let reveal_connected = Rc::new(Cell::new(false));
        apply_collapsed_group(
            &online_rows,
            &online_disclosure,
            &collapsed_group_state(
                CollapsiblePluginGroup::OnlineContent,
                global_enabled,
                reveal_online.get(),
                online_rows.len(),
            ),
        );
        apply_collapsed_group(
            &connected_rows,
            &connected_disclosure,
            &collapsed_group_state(
                CollapsiblePluginGroup::ConnectedServices,
                global_enabled,
                false,
                connected_rows.len(),
            ),
        );

        {
            let rows = online_rows.clone();
            let disclosure = online_disclosure.clone();
            let revealed = reveal_online.clone();
            online_disclosure.connect_activated(move |_| {
                revealed.set(true);
                apply_collapsed_group(
                    &rows,
                    &disclosure,
                    &collapsed_group_state(
                        CollapsiblePluginGroup::OnlineContent,
                        false,
                        true,
                        rows.len(),
                    ),
                );
            });
        }
        {
            let rows = connected_rows.clone();
            let disclosure = connected_disclosure.clone();
            let revealed = reveal_connected.clone();
            connected_disclosure.connect_activated(move |_| {
                revealed.set(true);
                apply_collapsed_group(
                    &rows,
                    &disclosure,
                    &collapsed_group_state(
                        CollapsiblePluginGroup::ConnectedServices,
                        false,
                        true,
                        rows.len(),
                    ),
                );
            });
        }
        {
            let context = self.clone();
            let rows = online_rows.clone();
            let disclosure = online_disclosure.clone();
            let connected_rows = connected_rows.clone();
            let connected_disclosure = connected_disclosure.clone();
            let reveal_online = reveal_online.clone();
            let reveal_connected = reveal_connected.clone();
            let syncing = Rc::new(Cell::new(false));
            let syncing_notify = syncing.clone();
            online_master.connect_active_notify(move |master| {
                if syncing_notify.get() {
                    return;
                }
                let enabled = master.is_active();
                if let Err(error) = context.set_online_sources_enabled(enabled) {
                    tracing::warn!(%error, "could not save the online-sources gate");
                    syncing_notify.set(true);
                    master.set_active(!enabled);
                    syncing_notify.set(false);
                    return;
                }
                reveal_online.set(false);
                reveal_connected.set(false);
                apply_collapsed_group(
                    &rows,
                    &disclosure,
                    &collapsed_group_state(
                        CollapsiblePluginGroup::OnlineContent,
                        enabled,
                        false,
                        rows.len(),
                    ),
                );
                apply_collapsed_group(
                    &connected_rows,
                    &connected_disclosure,
                    &collapsed_group_state(
                        CollapsiblePluginGroup::ConnectedServices,
                        enabled,
                        false,
                        connected_rows.len(),
                    ),
                );
            });
        }
        page.add(&local_group);
        page.add(&online_group);
        page.add(&connected_group);
        page
    }

    pub(in crate::ui) fn highlight_pending_plugin_rows(&self) {
        let targets = std::mem::take(&mut *self.pending_plugin_targets.borrow_mut());
        let rows = targets
            .iter()
            .filter_map(|target| {
                self.plugin_rows
                    .borrow()
                    .get(target)
                    .and_then(glib::WeakRef::upgrade)
            })
            .collect::<Vec<_>>();
        if let Some(first) = rows.first() {
            first.grab_focus();
        }
        for row in rows {
            if let Ok(expander) = row.clone().downcast::<adw::ExpanderRow>() {
                expander.set_expanded(true);
            }
            row.add_css_class(TARGET_CLASS);
            let row = row.downgrade();
            glib::timeout_add_local_once(highlight_duration(), move || {
                if let Some(row) = row.upgrade() {
                    row.remove_css_class(TARGET_CLASS);
                }
            });
        }
    }
}

#[cfg(test)]
#[path = "preference_plugins_tests.rs"]
mod tests;
