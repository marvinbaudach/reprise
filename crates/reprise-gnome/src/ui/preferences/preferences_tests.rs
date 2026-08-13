use super::*;
use crate::ui::preference_plugins::plugin_applies_live;

#[test]
fn set_10_retired_settings_deep_links_target_the_plugins_rows() {
    assert_eq!(
        plugin_targets_for_deep_link(SettingsDeepLink::OnlineSources),
        &["youtube", "podcasts", "radio"]
    );
    assert_eq!(
        plugin_targets_for_deep_link(SettingsDeepLink::ConcertLocation),
        &["concerts"]
    );
    assert_eq!(
        plugin_targets_for_deep_link(SettingsDeepLink::Artwork),
        &["artwork"]
    );
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
    assert_eq!(equalizer_preset(0), [0.0; 10]);
    for index in 0..4 {
        assert!(equalizer_preset(index)
            .into_iter()
            .all(|gain| (-12.0..=12.0).contains(&gain)));
    }
}
