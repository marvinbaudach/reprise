use super::*;
use crate::ui::preference_plugins::plugin_applies_live;

#[test]
fn set_10_optional_capability_deep_links_target_plugin_rows() {
    assert_eq!(
        plugin_targets_for_deep_link(PluginDeepLink::OnlineSources),
        &["youtube", "podcasts", "radio"]
    );
    assert_eq!(
        plugin_targets_for_deep_link(PluginDeepLink::Artwork),
        &["artwork"]
    );
}

#[test]
fn app_location_deep_link_owns_a_main_preferences_page() {
    assert_eq!(SettingsDeepLink::Location.page_name(), "location");
}

#[test]
fn gapless_control_is_available_only_without_crossfade_overlap() {
    assert!(gapless_control_state(0).sensitive);
    assert_eq!(gapless_control_state(0).subtitle, strings::GAPLESS_SUBTITLE);
    assert!(!gapless_control_state(1).sensitive);
    assert_eq!(
        gapless_control_state(10).subtitle,
        strings::GAPLESS_CROSSFADE_ACTIVE_SUBTITLE
    );
}

#[test]
fn only_runtime_safe_plugins_apply_without_restart() {
    for descriptor in reprise_core::modules::ALL_MODULES {
        assert!(plugin_applies_live(descriptor), "{}", descriptor.id);
    }
    assert!(!plugin_applies_live(&reprise_core::modules::MPRIS_MODULE));
}

#[test]
fn equalizer_presets_are_bounded_and_flat_is_zero() {
    assert_eq!(equalizer_preset(0), Some(EqualizerPreset::Flat));
    for index in 0..EqualizerPreset::ALL.len() as u32 {
        assert!(equalizer_preset(index)
            .unwrap()
            .ten_band_levels()
            .into_iter()
            .all(|gain| (-12.0..=12.0).contains(&gain)));
    }
}

fn equalizer_controls_for_test(conn: Rc<Db>) -> EqualizerControls {
    let bands = settings::get_equalizer_bands(&conn);
    let on_enabled: Rc<dyn Fn(bool)> = Rc::new(|_| {});
    let preset_conn = conn.clone();
    let on_preset: Rc<dyn Fn([f64; 10]) -> bool> =
        Rc::new(move |bands| settings::set_equalizer_bands(&preset_conn, bands).is_ok());
    let band_conn = conn;
    let on_band: Rc<dyn Fn(usize, f64)> = Rc::new(move |index, value| {
        let mut bands = settings::get_equalizer_bands(&band_conn);
        bands[index] = value;
        settings::set_equalizer_bands(&band_conn, bands).unwrap();
    });
    build_equalizer_controls(bands, true, on_enabled, &on_preset, on_band)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_preset_row_offers_every_shared_preset() {
    gtk4::init().unwrap();
    let controls = equalizer_controls_for_test(Rc::new(Db::open_in_memory().unwrap()));
    let model = controls.preset_button.menu_model().expect("preset model");

    assert_eq!(model.n_items(), EqualizerPreset::ALL.len() as i32);
    for (index, preset) in EqualizerPreset::ALL.into_iter().enumerate() {
        let label = model
            .item_attribute_value(
                index as i32,
                gtk4::gio::MENU_ATTRIBUTE_LABEL,
                Some(glib::VariantTy::STRING),
            )
            .expect("preset label")
            .get::<String>()
            .unwrap();
        assert_eq!(label, strings::text(preset_label(preset)));
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn choosing_a_new_preset_stores_its_bands() {
    gtk4::init().unwrap();
    let conn = Rc::new(Db::open_in_memory().unwrap());
    let controls = equalizer_controls_for_test(conn.clone());
    let vocal = EqualizerPreset::ALL
        .iter()
        .position(|preset| *preset == EqualizerPreset::Vocal)
        .unwrap();

    controls
        .preset_row
        .activate_action(&format!("equalizer.select-{vocal}"), None)
        .unwrap();

    assert_eq!(
        settings::get_equalizer_bands(&conn),
        EqualizerPreset::Vocal.ten_band_levels()
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn moving_a_band_labels_the_row_custom() {
    gtk4::init().unwrap();
    let conn = Rc::new(Db::open_in_memory().unwrap());
    let controls = equalizer_controls_for_test(conn.clone());
    assert_eq!(
        controls.preset_button.label().as_deref(),
        Some(strings::text(strings::PRESET_FLAT).as_str())
    );

    controls.scales[0].set_value(1.0);

    assert_eq!(settings::get_equalizer_bands(&conn)[0], 1.0);
    assert_eq!(
        controls.preset_button.label().as_deref(),
        Some(strings::text(strings::PRESET_CUSTOM).as_str())
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_17_the_bands_start_collapsed_behind_the_profile() {
    gtk4::init().unwrap();
    let controls = equalizer_controls_for_test(Rc::new(Db::open_in_memory().unwrap()));

    assert!(!controls.root.is_expanded());
    assert_eq!(
        controls.preset_row.next_sibling(),
        Some(controls.root.clone().upcast())
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn disabling_the_equalizer_dims_the_collapsed_bands() {
    gtk4::init().unwrap();
    let controls = equalizer_controls_for_test(Rc::new(Db::open_in_memory().unwrap()));

    controls.enabled.set_active(false);

    assert!(!controls.root.is_sensitive());
}
