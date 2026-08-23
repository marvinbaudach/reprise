use super::*;

#[test]
fn nr_7_new_releases_plugin_uses_privacy_copy_and_live_toggle_id() {
    let descriptor = &reprise_core::modules::NEW_RELEASES_MODULE;

    assert_eq!(plugin_title(descriptor), "New Releases");
    assert!(plugin_description(descriptor).contains("contacts MusicBrainz"));
    assert!(plugin_applies_live(descriptor));
}

#[test]
fn concerts_plugin_uses_event_provider_privacy_copy_and_live_toggle_id() {
    let descriptor = &reprise_core::modules::CONCERTS_MODULE;

    assert_eq!(plugin_title(descriptor), "Concerts");
    assert!(plugin_description(descriptor).contains("contacts"));
    assert!(plugin_applies_live(descriptor));
}

#[test]
fn source_plugins_expose_the_service_specific_privacy_copy() {
    assert_eq!(
        plugin_description(&reprise_core::modules::YOUTUBE_MODULE),
        "Channels as audio episodes · channel feeds, audio via yt-dlp"
    );
    assert_eq!(
        plugin_description(&reprise_core::modules::PODCASTS_MODULE),
        "Shows as audio episodes · RSS feeds, search via Apple Podcasts"
    );
    assert_eq!(
        plugin_description(&reprise_core::modules::RADIO_MODULE),
        "Stations and live streams · radio-browser.info directory"
    );
}

#[test]
fn set_10_plugins_is_the_single_settings_home_in_the_design_order() {
    assert_eq!(plugin_ids_for_group(PluginGroup::Local), &["song_visuals"]);
    assert_eq!(
        plugin_ids_for_group(PluginGroup::Online),
        &[
            "artwork",
            "online_lyrics",
            "concerts",
            "new_releases",
            "youtube",
            "podcasts",
            "radio",
        ]
    );
    assert_eq!(
        plugin_ids_for_group(PluginGroup::Connected),
        &["scrobbling"]
    );
    assert_eq!(
        LOCAL_PLUGIN_IDS
            .iter()
            .chain(ONLINE_PLUGIN_IDS)
            .copied()
            .filter(|id| plugin_uses_expander(id))
            .collect::<Vec<_>>(),
        ["concerts", "new_releases", "youtube", "podcasts", "radio",]
    );
}

#[test]
fn artwork_plugin_uses_the_combined_privacy_copy() {
    assert_eq!(
        plugin_title(&reprise_core::modules::ARTWORK_MODULE),
        "Artwork"
    );
    assert_eq!(
        plugin_description(&reprise_core::modules::ARTWORK_MODULE),
        "Album covers, artist portraits and source artwork · contacts MusicBrainz, coverartarchive.org, Deezer, YouTube, Apple Podcasts and image hosts"
    );
    assert!(!plugin_uses_expander("artwork"));
}

#[test]
fn network_plugin_deep_link_highlight_is_transient() {
    assert_eq!(
        highlight_duration(),
        std::time::Duration::from_millis(u64::from(crate::ui::motion::AMBIENT_MS))
    );
    let css = css();
    assert!(css.contains(".reprise-plugin-target"));
    assert!(css.contains(&format!(
        "{}ms {}",
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING
    )));
}

#[test]
fn all_network_plugin_rows_expose_privacy_copy() {
    let descriptors = plugin_ids_for_group(PluginGroup::Online)
        .iter()
        .map(|id| descriptor(id))
        .collect::<Vec<_>>();
    assert_eq!(descriptors.len(), ONLINE_PLUGIN_IDS.len());
    for descriptor in descriptors {
        assert!(plugin_applies_live(descriptor));
        let description = plugin_description(descriptor);
        let names_contact = match descriptor.id {
            "youtube" => description.contains("yt-dlp"),
            "podcasts" => description.contains("Apple Podcasts"),
            "radio" => description.contains("radio-browser.info"),
            "new_releases" => description.contains("MusicBrainz"),
            "concerts" => description.contains("event providers"),
            "artwork" => {
                description.contains("MusicBrainz")
                    && description.contains("coverartarchive.org")
                    && description.contains("Deezer")
                    && description.contains("YouTube")
                    && description.contains("Apple Podcasts")
            }
            "online_lyrics" => description.contains("LRCLIB"),
            id => panic!("online capability {id} has no privacy-copy assertion"),
        };
        assert!(
            names_contact,
            "{} does not name the contacted service: {description}",
            descriptor.id
        );
    }
}

#[test]
fn set_10_plugins_are_grouped_by_user_intent_with_one_scrobbling_entry() {
    let all_ids = LOCAL_PLUGIN_IDS
        .iter()
        .chain(ONLINE_PLUGIN_IDS)
        .chain(CONNECTED_PLUGIN_IDS)
        .copied()
        .collect::<Vec<_>>();
    let unique_ids = all_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        all_ids.len(),
        unique_ids.len(),
        "every capability must appear exactly once"
    );
    assert_eq!(all_ids.iter().filter(|id| **id == "scrobbling").count(), 1);
}

#[test]
fn set_11_online_master_off_preserves_each_module_configuration() {
    let db = crate::test_db::open().unwrap();
    let configured = [
        (&reprise_core::modules::YOUTUBE_MODULE, true),
        (&reprise_core::modules::PODCASTS_MODULE, false),
        (&reprise_core::modules::RADIO_MODULE, true),
        (&reprise_core::modules::NEW_RELEASES_MODULE, false),
        (&reprise_core::modules::CONCERTS_MODULE, true),
        (&reprise_core::modules::ARTWORK_MODULE, true),
        (&reprise_core::modules::ONLINE_LYRICS_MODULE, false),
    ];
    for (descriptor, enabled) in configured {
        reprise_core::modules::set_enabled(&db, descriptor, enabled).unwrap();
    }

    reprise_core::online_sources::set_enabled(&db, false).unwrap();
    reprise_core::online_sources::set_enabled(&db, true).unwrap();

    for (descriptor, enabled) in configured {
        assert_eq!(
            reprise_core::modules::is_enabled(&db, descriptor).unwrap(),
            enabled,
            "{} changed during the global-gate round trip",
            descriptor.id
        );
    }
}

#[test]
fn set_11_collapsed_online_content_reveals_all_sources_read_only() {
    let collapsed = collapsed_group_state(
        CollapsiblePluginGroup::OnlineContent,
        false,
        false,
        ONLINE_PLUGIN_IDS.len(),
    );
    assert!(!collapsed.rows_visible);
    assert!(!collapsed.rows_sensitive);
    assert!(collapsed.disclosure_visible);
    assert_eq!(collapsed.disclosure_label, "Show the 7 sources");

    let revealed = collapsed_group_state(
        CollapsiblePluginGroup::OnlineContent,
        false,
        true,
        ONLINE_PLUGIN_IDS.len(),
    );
    assert!(revealed.rows_visible);
    assert!(!revealed.rows_sensitive);
    assert!(!revealed.disclosure_visible);

    let enabled = collapsed_group_state(
        CollapsiblePluginGroup::OnlineContent,
        true,
        false,
        ONLINE_PLUGIN_IDS.len(),
    );
    assert!(enabled.rows_visible);
    assert!(enabled.rows_sensitive);
    assert!(!enabled.disclosure_visible);

    let connected = collapsed_group_state(
        CollapsiblePluginGroup::ConnectedServices,
        false,
        false,
        CONNECTED_PLUGIN_IDS.len(),
    );
    assert!(!connected.rows_visible);
    assert!(!connected.rows_sensitive);
    assert!(connected.disclosure_visible);
    assert_eq!(
        connected.disclosure_label,
        "Scrobbling · needs online sources"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn plugins_alignment_placeholder_is_presentation_only() {
    gtk4::init().unwrap();
    let placeholder = switch_alignment_placeholder();

    assert_eq!(
        placeholder.accessible_role(),
        gtk4::AccessibleRole::Presentation
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_11_online_content_starts_with_a_persistent_master_row() {
    gtk4::init().unwrap();
    let master = online_master_row(false);
    let group = online_group_with_master(&master);
    let disclosure = adw::ActionRow::new();
    let plugin = aligned_switch_row("Artwork", "Artwork services", true);
    group.add(&disclosure);
    group.add(&plugin);
    apply_collapsed_group(
        &[plugin.clone().upcast()],
        &disclosure,
        &collapsed_group_state(CollapsiblePluginGroup::OnlineContent, false, false, 1),
    );

    // The heading stands exactly once: on the master row, not above it.
    assert!(group.title().is_empty());
    assert_eq!(master.title(), "Online content");
    assert!(master.has_css_class(chrome::MASTER_ROW_CLASS));
    assert_eq!(
        master.subtitle().as_deref(),
        Some(strings::text(strings::ONLINE_CONTENT_MASTER_DESCRIPTION).as_str())
    );
    assert!(master.is_visible());
    assert!(master.is_sensitive());
    assert!(!master.is_active());
    master.set_active(true);
    assert!(master.is_active());
    assert!(!plugin.is_visible());
    assert!(!plugin.is_sensitive());
    assert!(master
        .ancestor(adw::PreferencesGroup::static_type())
        .is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_14a_plugin_switches_share_the_same_right_edge_across_row_types() {
    gtk4::init().unwrap();
    let group = adw::PreferencesGroup::new();
    let switch_row = aligned_switch_row("Artwork", "Artwork services", false);
    let expander = adw::ExpanderRow::builder()
        .title("Podcasts")
        .show_enable_switch(true)
        .build();
    group.add(&switch_row);
    group.add(&expander);
    let window = gtk4::Window::builder()
        .default_width(640)
        .child(&group)
        .build();
    window.present();
    assert!(crate::ui::test_settle::settle_until_mapped(&window));

    let switch = switch_row
        .first_child()
        .and_then(|root| find_descendant::<gtk4::Switch>(&root))
        .expect("switch row must render its switch");
    let expander_switch = expander
        .first_child()
        .and_then(|root| find_descendant::<gtk4::Switch>(&root))
        .expect("expander row must render its enable switch");
    let switch_bounds = switch.compute_bounds(&group).unwrap();
    let expander_bounds = expander_switch.compute_bounds(&group).unwrap();
    let switch_edge = switch_bounds.x() + switch_bounds.width();
    let expander_edge = expander_bounds.x() + expander_bounds.width();

    assert_eq!(switch_edge, expander_edge);
    window.close();
}

fn find_descendant<T: IsA<gtk4::Widget> + gtk4::glib::types::StaticType>(
    root: &gtk4::Widget,
) -> Option<T> {
    if let Ok(found) = root.clone().downcast::<T>() {
        return Some(found);
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_descendant::<T>(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
fn doc_7c_library_doctor_has_no_preferences_surface() {
    assert!(!LOCAL_PLUGIN_IDS.contains(&"library_doctor"));
    assert!(!plugin_uses_expander("library_doctor"));
}
