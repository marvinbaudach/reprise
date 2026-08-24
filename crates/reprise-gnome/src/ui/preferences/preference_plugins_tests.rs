use super::*;
use crate::ui::preferences::preference_online_master::{self as master_chrome, OnlineMaster};

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
fn set_11a_online_master_off_preserves_each_module_configuration() {
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
fn set_11a_the_children_stay_in_place_when_the_gate_goes_off() {
    let names = ["Concerts", "New Releases", "YouTube"]
        .map(str::to_owned)
        .to_vec();

    let on = online_section_state(true, 7, 7, &names);
    assert_eq!(on.badge, "7 of 7 plugins on");
    assert_eq!(on.card_opacity, 1.0);
    assert!(on.card_interactive);
    assert_eq!(on.hint, None);

    let off = online_section_state(false, 7, 7, &names);
    assert_eq!(off.badge, "all 7 plugins off");
    // Dimmed, not greyed out: the rows must stay readable.
    assert_eq!(off.card_opacity, 0.42);
    assert!(off.card_opacity > 0.0);
    assert!(!off.card_interactive);
    assert_eq!(
        off.hint.as_deref(),
        Some(
            "7 plugins paused \u{b7} no requests \u{b7} Concerts, New Releases and YouTube hidden from the sidebar"
        )
    );
}

#[test]
fn set_11a_the_badge_counts_the_children_that_are_actually_on() {
    let names = sidebar_plugin_names();

    assert_eq!(
        online_section_state(true, 3, 7, &names).badge,
        "3 of 7 plugins on"
    );
    assert_eq!(
        online_section_state(false, 3, 7, &names).badge,
        "all 7 plugins off"
    );
}

#[test]
fn set_11a_the_paused_hint_names_the_sidebar_entries_that_really_go() {
    // The draft names three because its mock lists five plugins. The real list
    // is longer, so the names are derived rather than written out.
    assert_eq!(
        sidebar_plugin_names(),
        vec!["Concerts", "New Releases", "YouTube", "Podcasts", "Radio"]
    );
    for id in SIDEBAR_PLUGIN_IDS {
        assert!(
            ONLINE_PLUGIN_IDS.contains(id),
            "{id} is named as a sidebar entry but is not an online plugin"
        );
    }
}

#[test]
fn set_11a_connected_services_still_collapse_behind_the_gate() {
    let collapsed = connected_group_state(false, false);
    assert!(!collapsed.rows_visible);
    assert!(!collapsed.rows_sensitive);
    assert!(collapsed.disclosure_visible);
    assert_eq!(
        collapsed.disclosure_label,
        "Scrobbling \u{b7} needs online sources"
    );

    let revealed = connected_group_state(false, true);
    assert!(revealed.rows_visible);
    assert!(!revealed.rows_sensitive);
    assert!(!revealed.disclosure_visible);

    let enabled = connected_group_state(true, false);
    assert!(enabled.rows_visible);
    assert!(enabled.rows_sensitive);
    assert!(!enabled.disclosure_visible);
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
fn set_11a_the_master_stands_above_its_children_not_among_them() {
    gtk4::init().unwrap();
    let master = OnlineMaster::new(false);
    master.set_badge(false, 0, 7);

    // It is not a row: it never enters the card its children sit in.
    assert!(master
        .widget()
        .clone()
        .downcast::<adw::PreferencesRow>()
        .is_err());
    assert!(master.widget().has_css_class(master_chrome::MASTER_CLASS));
    assert!(!master.is_active());
    master.set_active_silently(true);
    assert!(master.is_active());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_14b_plugin_switches_share_the_same_right_edge_across_row_types() {
    gtk4::init().unwrap();
    let group = adw::PreferencesGroup::new();
    let switch_row = aligned_switch_row("Artwork", "Artwork services", false);
    let expander = adw::ExpanderRow::builder().title("Podcasts").build();
    let expander_toggle = gtk4::Switch::builder().valign(gtk4::Align::Center).build();
    expander.add_suffix(&expander_toggle);
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
    let expander_switch = expander_toggle.clone();
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

/// `SET-11a`: a switch never moves an expansion state, and the counterprobe
/// shows the mechanism that used to — `enable-expansion` forces `expanded` to
/// its own value, which is why switching the gate on unfolded every plugin's
/// settings at once.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_11a_switching_a_plugin_on_never_opens_its_settings() {
    gtk4::init().unwrap();

    // Counterprobe: the retired wiring. Not a stand-in — the very libadwaita
    // call the page used to make.
    let retired = adw::ExpanderRow::builder()
        .title("Concerts")
        .show_enable_switch(true)
        .enable_expansion(false)
        .build();
    retired.add_row(&adw::ActionRow::builder().title("Location").build());
    assert!(!retired.is_expanded());
    retired.set_enable_expansion(true);
    assert!(
        retired.is_expanded(),
        "the counterprobe must reproduce the unfolding, or it proves nothing"
    );

    // The wiring in production: the switch is a suffix, the expansion is the
    // chevron's and the title area's alone.
    let row = adw::ExpanderRow::builder().title("Concerts").build();
    let toggle = gtk4::Switch::builder().valign(gtk4::Align::Center).build();
    row.add_suffix(&toggle);
    row.add_row(&adw::ActionRow::builder().title("Location").build());
    assert!(!row.is_expanded());

    toggle.set_active(true);
    assert!(
        !row.is_expanded(),
        "switching a plugin on must not open its settings"
    );
    toggle.set_active(false);
    assert!(!row.is_expanded());

    // And the other axis still works, and is not disturbed by the switch.
    row.set_expanded(true);
    toggle.set_active(true);
    toggle.set_active(false);
    assert!(
        row.is_expanded(),
        "a switch must not close a row the reader opened either"
    );
}

/// `SET-11a`: the bracket really materialises. Nesting a group inside another
/// group's box is not libadwaita's documented path, so "the card is indented
/// behind a rail" has to be measured on screen, not assumed from the code that
/// asks for it — a silently dropped child would leave the page looking flat and
/// every unit test still green.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_11a_the_children_card_is_indented_behind_an_allocated_rail() {
    gtk4::init().unwrap();
    crate::ui::style::install();

    let page = adw::PreferencesPage::new();
    page.add_css_class(chrome::PLUGINS_PAGE_CLASS);
    let master = OnlineMaster::new(true);
    let master_group = adw::PreferencesGroup::new();
    master_group.add(master.widget());

    let children_group = adw::PreferencesGroup::new();
    let bracket = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let rail = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rail.add_css_class(master_chrome::RAIL_CLASS);
    rail.set_valign(gtk4::Align::Fill);
    rail.set_vexpand(true);
    let card = adw::PreferencesGroup::new();
    card.add_css_class(master_chrome::CHILDREN_CLASS);
    card.set_hexpand(true);
    bracket.append(&rail);
    bracket.append(&card);
    children_group.add(&bracket);
    for title in ["Artwork", "Online Lyrics", "Concerts"] {
        card.add(&aligned_switch_row(title, "Subtitle", true));
    }
    page.add(&master_group);
    page.add(&children_group);

    let window = gtk4::Window::builder()
        .default_width(760)
        .default_height(680)
        .child(&page)
        .build();
    window.present();
    assert!(crate::ui::test_settle::settle_until_mapped(&window));

    assert!(
        master.widget().is_ancestor(&page),
        "the master must reach the page through its group's box"
    );
    assert!(
        card.is_ancestor(&page),
        "the children card must reach the page through the bracket"
    );

    let rail_bounds = rail
        .compute_bounds(&page)
        .expect("the rail must be allocated on the page");
    let card_bounds = card
        .compute_bounds(&page)
        .expect("the card must be allocated on the page");
    assert!(
        rail_bounds.width() >= 2.0,
        "the rail must claim its 2px, got {}",
        rail_bounds.width()
    );
    assert!(
        rail_bounds.height() > 0.0,
        "the rail must run the height of the card it marks"
    );
    assert_eq!(
        card_bounds.x() - (rail_bounds.x() + rail_bounds.width()),
        18.0,
        "the card sits 18px right of the rail"
    );

    // And the master is genuinely above the card, not beside it.
    let master_bounds = master
        .widget()
        .compute_bounds(&page)
        .expect("the master must be allocated on the page");
    assert!(
        master_bounds.y() + master_bounds.height() <= card_bounds.y(),
        "the master must stand above its children, not among them"
    );

    window.close();
}
