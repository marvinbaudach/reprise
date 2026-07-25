//! Shared Scrobbling entry and provider detail page.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::scrobble_runtime::ScrobbleRuntime;
use crate::ui::strings;

use super::PreferencesContext;

pub(in crate::ui) struct ScrobblingSurface {
    pub(in crate::ui) entry: adw::ActionRow,
    pub(in crate::ui) page: adw::NavigationPage,
}

pub(in crate::ui) fn scrobbling_summary(listenbrainz: bool, lastfm: bool) -> String {
    let message = match (listenbrainz, lastfm) {
        (false, false) => strings::SCROBBLING_CONNECT_SERVICES,
        (true, false) => strings::SCROBBLING_LISTENBRAINZ_ENABLED,
        (false, true) => strings::SCROBBLING_LASTFM_ENABLED,
        (true, true) => strings::SCROBBLING_BOTH_ENABLED,
    };
    strings::text(message)
}

pub(in crate::ui) fn build_surface(
    listenbrainz: &adw::ExpanderRow,
    lastfm: &adw::ExpanderRow,
    summary: &str,
) -> ScrobblingSurface {
    let entry = adw::ActionRow::builder()
        .title(strings::text(strings::SCROBBLING))
        .subtitle(summary)
        .activatable(true)
        .build();
    entry.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));

    let group = adw::PreferencesGroup::new();
    group.add(listenbrainz);
    group.add(lastfm);
    let detail = adw::PreferencesPage::new();
    detail.add(&group);
    let header = adw::HeaderBar::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&detail));
    let page = adw::NavigationPage::new(&toolbar, &strings::text(strings::SCROBBLING));

    ScrobblingSurface { entry, page }
}

pub(in crate::ui) fn present_scrobbling(
    parent: &impl IsA<gtk4::Widget>,
    page: &adw::NavigationPage,
) {
    let Some(navigation) = parent
        .ancestor(adw::NavigationView::static_type())
        .and_downcast::<adw::NavigationView>()
    else {
        tracing::warn!("scrobbling entry activated outside the preferences navigation");
        return;
    };
    navigation.push(page);
}

pub(in crate::ui) fn build(context: &Rc<PreferencesContext>) -> adw::ActionRow {
    let listenbrainz = context.build_listenbrainz_row();
    let lastfm = context.build_lastfm_row();
    let summary = scrobbling_summary(context.listenbrainz.is_active(), context.lastfm.is_active());
    let surface = build_surface(&listenbrainz, &lastfm, &summary);
    let page = surface.page.clone();
    surface
        .entry
        .connect_activated(move |row| present_scrobbling(row, &page));
    subscribe_summary(&context.listenbrainz, context, &surface.entry);
    subscribe_summary(&context.lastfm, context, &surface.entry);
    surface.entry
}

fn subscribe_summary(
    runtime: &Rc<ScrobbleRuntime>,
    context: &Rc<PreferencesContext>,
    entry: &adw::ActionRow,
) {
    let context = Rc::downgrade(context);
    let entry = entry.downgrade();
    runtime.subscribe(Rc::new(move |_| {
        let (Some(context), Some(entry)) = (context.upgrade(), entry.upgrade()) else {
            return;
        };
        let summary =
            scrobbling_summary(context.listenbrainz.is_active(), context.lastfm.is_active());
        entry.set_subtitle(&summary);
    }));
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    use super::*;

    #[test]
    fn scrobbling_entry_summarizes_any_enabled_provider_without_a_master_state() {
        assert_eq!(
            scrobbling_summary(false, false),
            "Connect ListenBrainz, Last.fm, or both"
        );
        assert_eq!(scrobbling_summary(true, false), "ListenBrainz enabled");
        assert_eq!(scrobbling_summary(false, true), "Last.fm enabled");
        assert_eq!(
            scrobbling_summary(true, true),
            "ListenBrainz and Last.fm enabled"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_6b_scrobbling_detail_keeps_providers_independent() {
        gtk4::init().unwrap();
        let listenbrainz = adw::ExpanderRow::builder()
            .title("ListenBrainz")
            .show_enable_switch(true)
            .enable_expansion(true)
            .build();
        let lastfm = adw::ExpanderRow::builder()
            .title("Last.fm")
            .show_enable_switch(true)
            .enable_expansion(false)
            .build();

        let surface = build_surface(&listenbrainz, &lastfm, "ListenBrainz enabled");

        assert_eq!(surface.entry.type_(), adw::ActionRow::static_type());
        assert_eq!(surface.entry.title(), "Scrobbling");
        assert_eq!(surface.page.title(), "Scrobbling");
        assert!(listenbrainz.is_ancestor(&surface.page));
        assert!(lastfm.is_ancestor(&surface.page));
        assert!(listenbrainz.enables_expansion());
        assert!(!lastfm.enables_expansion());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_6a_scrobbling_detail_stays_inside_preferences_navigation() {
        gtk4::init().unwrap();
        let listenbrainz = adw::ExpanderRow::builder().title("ListenBrainz").build();
        let lastfm = adw::ExpanderRow::builder().title("Last.fm").build();
        let surface = build_surface(&listenbrainz, &lastfm, "Not connected");
        let group = adw::PreferencesGroup::new();
        group.add(&surface.entry);
        let root_content = adw::PreferencesPage::new();
        root_content.add(&group);
        let root = adw::NavigationPage::new(&root_content, "Plugins");
        let navigation = adw::NavigationView::new();
        navigation.add(&root);

        present_scrobbling(&surface.entry, &surface.page);

        assert_eq!(navigation.visible_page().as_ref(), Some(&surface.page));
        assert!(navigation.pop());
        assert_eq!(navigation.visible_page().as_ref(), Some(&root));
    }
}
