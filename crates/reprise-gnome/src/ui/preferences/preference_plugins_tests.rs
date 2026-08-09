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
            "youtube",
            "podcasts",
            "radio",
            "new_releases",
            "concerts",
            "cover_download",
            "artist_portraits",
            "online_lyrics",
            "source_images",
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
        ["youtube", "podcasts", "radio", "new_releases", "concerts",]
    );
}

#[test]
fn set_12_provision_badges_use_the_static_group_majority_rule() {
    use reprise_core::modules::ProvisionKind;

    assert!(provision_badges_for("youtube", ONLINE_PLUGIN_IDS).is_empty());
    assert_eq!(
        provision_badges_for("cover_download", ONLINE_PLUGIN_IDS)
            .iter()
            .map(|provision| provision.kind)
            .collect::<Vec<_>>(),
        [ProvisionKind::Extends]
    );
    for id in LOCAL_PLUGIN_IDS {
        assert!(
            !provision_badges_for(id, LOCAL_PLUGIN_IDS).is_empty(),
            "a tied Local row must retain its badge: {id}"
        );
    }
    assert_eq!(
        provision_badge_tone(ProvisionKind::PanelTab),
        ProvisionBadgeTone::Accent
    );
    assert_eq!(
        provision_badge_tone(ProvisionKind::SidebarSection),
        ProvisionBadgeTone::Accent
    );
    assert_eq!(
        provision_badge_tone(ProvisionKind::ContextItem),
        ProvisionBadgeTone::Neutral
    );
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
            "cover_download" => {
                description.contains("MusicBrainz") && description.contains("coverartarchive.org")
            }
            "artist_portraits" => description.contains("Deezer"),
            "online_lyrics" => description.contains("LRCLIB"),
            "source_images" => {
                description.contains("YouTube")
                    && description.contains("Apple Podcasts")
                    && description.contains("radio-browser.info")
            }
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
        (&reprise_core::modules::COVER_DOWNLOAD_MODULE, false),
        (&reprise_core::modules::ARTIST_PORTRAITS_MODULE, true),
        (&reprise_core::modules::ONLINE_LYRICS_MODULE, false),
        (&reprise_core::modules::SOURCE_IMAGES_MODULE, true),
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
    assert_eq!(collapsed.disclosure_label, "Show the 9 sources");

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
fn set_11_online_content_header_owns_the_master_switch() {
    gtk4::init().unwrap();
    let master = gtk4::Switch::new();
    let group = build_online_group_header(&master);

    assert_eq!(group.title(), "Online content");
    assert_eq!(
        group.description().as_deref(),
        Some(
            "Use online sources — off makes this a local player: nothing below runs, no requests, sidebar entries hidden."
        )
    );
    assert_eq!(
        group.header_suffix().as_ref(),
        Some(master.upcast_ref::<gtk4::Widget>())
    );
}

#[test]
fn doc_7c_library_doctor_has_no_preferences_surface() {
    assert!(!LOCAL_PLUGIN_IDS.contains(&"library_doctor"));
    assert!(!plugin_uses_expander("library_doctor"));
}
