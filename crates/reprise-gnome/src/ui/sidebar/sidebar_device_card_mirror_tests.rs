use super::tests::view;
use super::*;
use reprise_core::device_sync::device_view::DeviceContentsState;
use reprise_core::device_sync::{DeviceStorageAccess, MusicDiff, MusicReading};

fn select_playlist(device: &mut DeviceView) {
    device.page.blockers.clear();
    device.page.playlists = vec![reprise_core::device_sync::SyncPlaylistRow {
        source: reprise_core::device_sync::SelectionSource::Playlist(1),
        name: Some("Road".into()),
        smart: false,
        selected: true,
        available: true,
        entry_count: 1,
        unique_track_count: 1,
        unavailable_count: 0,
        target_bytes: 1,
        last_synced_at: None,
    }];
}

fn no_cancel() -> CancelCallback {
    Rc::new(|_| {})
}

fn diff(
    files_to_copy: usize,
    bytes_to_copy: u64,
    files_to_remove: usize,
    bytes_freed: u64,
) -> MusicDiff {
    MusicDiff {
        files_to_copy,
        bytes_to_copy,
        files_to_remove,
        bytes_freed,
        playlists_rewritten: 0,
    }
}

#[test]
fn card_detail_mode_only_distinguishes_delta_and_progress() {
    let mut pending = view(PlannedSyncPhase::Idle);
    select_playlist(&mut pending);
    pending.page.changes.additions = 1;
    assert_eq!(detail_mode(&pending), DetailMode::Delta);

    pending.sync_phase = PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 0,
        bytes_total: 1,
    };
    assert_eq!(detail_mode(&pending), DetailMode::Progress);
    assert_eq!(
        detail_mode(&view(PlannedSyncPhase::ComputingDelta)),
        DetailMode::Delta
    );
}

#[test]
fn mtp_63_sidebar_keeps_free_space_visible_during_sync() {
    let mut copying = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 1,
        total: 2,
        current_track: "Track A".into(),
        bytes_done: 1,
        bytes_total: 2,
    });
    copying.name = "Phone A".into();
    copying.bytes_per_second = 2 * 1_024 * 1_024;

    copying.storage.free_bytes = Some(8 * 1_024 * 1_024);
    assert_eq!(card_title(&copying), "Syncing Phone A");
    assert_eq!(
        card_subtitle(&copying),
        "8.0 MiB free · ↑ Track A · 2.0 MiB/s"
    );
    copying.sync_phase = PlannedSyncPhase::ComputingDelta;
    assert_eq!(card_subtitle(&copying), "8.0 MiB free · Checking changes…");
    copying.sync_phase = PlannedSyncPhase::Finishing;
    assert_eq!(card_subtitle(&copying), "8.0 MiB free · Finishing…");
}

#[test]
fn mtp_63_computing_delta_without_storage_names_the_activity_without_a_placeholder() {
    let device = view(PlannedSyncPhase::ComputingDelta);

    let subtitle = card_subtitle(&device);

    assert_eq!(subtitle, "Checking changes…");
    assert!(!subtitle.contains(device_sync_strings::SPACE_UNKNOWN));
    assert!(!subtitle.starts_with('·'));
}

#[test]
fn syncing_without_storage_names_the_activity_without_a_placeholder() {
    let mut device = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track A".into(),
        bytes_done: 0,
        bytes_total: 1,
    });
    device.bytes_per_second = 2 * 1_024 * 1_024;

    let subtitle = card_subtitle(&device);

    assert_eq!(subtitle, "↑ Track A · 2.0 MiB/s");
    assert!(!subtitle.contains(device_sync_strings::SPACE_UNKNOWN));
    assert!(!subtitle.starts_with('·'));
}

#[test]
fn finishing_without_storage_names_the_activity_without_a_placeholder() {
    let device = view(PlannedSyncPhase::Finishing);

    let subtitle = card_subtitle(&device);

    assert_eq!(subtitle, "Finishing…");
    assert!(!subtitle.contains(device_sync_strings::SPACE_UNKNOWN));
    assert!(!subtitle.starts_with('·'));
}

#[test]
fn attention_without_storage_names_the_problem_without_a_placeholder() {
    let mut device = view(PlannedSyncPhase::Idle);
    device
        .page
        .warnings
        .push(reprise_core::device_sync::SyncPageWarning::UnsafeManagedItem);

    let subtitle = card_subtitle(&device);

    assert_eq!(subtitle, "Needs attention");
    assert!(!subtitle.contains(device_sync_strings::SPACE_UNKNOWN));
    assert!(!subtitle.starts_with('·'));
}

#[test]
fn mtp_29_idle_card_reads_the_aggregate_balance_not_a_blended_change_count() {
    let mut device = view(PlannedSyncPhase::Idle);
    select_playlist(&mut device);
    device.contents_state = DeviceContentsState::Verified;
    device.target_reading = MusicReading::Diff(diff(1, 1_024 * 1_024, 1, 0));

    assert_eq!(card_title(&device), "Pixel 8");
    assert_eq!(card_subtitle(&device), "1 to copy · 1.0 MiB · 1 to remove");
}

#[test]
fn mtp_48_inert_device_card_names_the_active_device_and_offers_no_sync_copy() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.session_state = reprise_core::device_sync::DeviceSessionState::Inert {
        active_device_name: "Pixel 7a (Anna)".into(),
    };

    assert_eq!(
        card_subtitle(&device),
        "Plugged in · disconnect Pixel 7a (Anna) to use it"
    );
    assert!(!device.page.controls.can_start);
}

#[test]
fn mtp_50_remembered_card_is_dimmed_has_no_diff_and_exposes_local_memory_actions() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.connected = false;
    device.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;
    device.last_sync = Some(chrono::Utc::now() - chrono::Duration::days(3));
    device.target_reading = MusicReading::Diff(diff(14, 2_600_000_000, 3, 148 * 1_024 * 1_024));

    assert_eq!(card_subtitle(&device), "Not connected · synced 3 days ago");
    assert!(idle_tooltip(&device).is_none());
    assert!(css().contains(
        ".device-card-remembered .device-card-title { color: alpha(@window_fg_color, 0.62); }"
    ));
    assert!(!css().contains("opacity: 0.58"));
    let menu_source = include_str!("../device_sync/device_sync_card_menu.rs");
    assert!(menu_source.contains("BUTTON_SECONDARY"));
    assert!(menu_source.contains("FORGET_DEVICE"));
    assert!(menu_source.contains("device_sync_rename::prompt"));
    assert!(menu_source.contains("forget_remembered_device"));
}

#[test]
fn css_covers_the_sync_card_vocabulary() {
    let css = css();
    for marker in [
        ".device-card {",
        ".device-card-active:hover",
        ".device-card:focus-visible",
        ".device-card-icon",
        ".device-card-glyph",
        ".device-card-detail",
        ".device-card-percent",
        ".device-card-progress trough",
        ".device-card-progress progress",
    ] {
        assert!(css.contains(marker), "missing rule: {marker}");
    }
    assert!(
        !css.contains("#1CA98F"),
        "the accent must come from the shared style source, not a literal"
    );
}

#[test]
fn mtp_63_every_emphasis_step_has_its_own_ground_and_edge() {
    let css = css();
    assert!(
        !css.contains('#'),
        "device-card structural CSS must use theme roles, never color literals"
    );

    let rule = |selector: &str| -> String {
        css.split_once(selector)
            .and_then(|(_, rest)| rest.split_once('}'))
            .map_or_else(
                || panic!("missing CSS rule: {selector}"),
                |(body, _)| body.to_owned(),
            )
    };
    let property = |body: &str, name: &str| -> String {
        body.split(';')
            .find_map(|declaration| {
                let (property, value) = declaration.split_once(':')?;
                (property.trim() == name).then(|| value.trim().to_owned())
            })
            .unwrap_or_else(|| panic!("missing {name} in {body}"))
    };
    let parse = |hex: &str| -> [u8; 3] {
        let hex = hex.trim().strip_prefix('#').expect("color starts with #");
        [0, 2, 4].map(|offset| {
            u8::from_str_radix(&hex[offset..offset + 2], 16).expect("hexadecimal color channel")
        })
    };
    let composite = |foreground: [u8; 3], background: [u8; 3], alpha: f64| {
        [0, 1, 2].map(|index| {
            (f64::from(foreground[index]) * alpha + f64::from(background[index]) * (1.0 - alpha))
                .round() as u8
        })
    };
    let contrast = |first: [u8; 3], second: [u8; 3]| {
        let luminance = |color: [u8; 3]| {
            let linear = |channel: u8| {
                let channel = f64::from(channel) / 255.0;
                if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2])
        };
        let first = luminance(first);
        let second = luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    };
    let alpha = |value: &str| {
        value
            .trim_end_matches(')')
            .rsplit_once(',')
            .unwrap_or_else(|| panic!("alpha color expected: {value}"))
            .1
            .trim()
            .parse::<f64>()
            .expect("alpha is a decimal fraction")
    };

    let remembered = rule(".device-card-remembered {");
    let connected = rule(".device-card-connected {");
    let active = rule(".device-card-active {");
    assert_eq!(property(&remembered, "background-color"), "transparent");

    for theme in crate::ui::style::theme::Theme::all() {
        for (is_dark, palette) in [(true, theme.palette()), (false, theme.light_palette())] {
            let sidebar = parse(palette.sidebar_bg);
            let foreground = parse(palette.fg);
            let accent = parse(crate::ui::style::accent::APP_ACCENT);
            let theme_css = crate::ui::style::theme::theme_css(
                theme,
                is_dark,
                crate::ui::style::accent::AccentSource::App,
            );
            let accent_edge = theme_css
                .lines()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("@define-color reprise_accent_text_color ")
                        .map(|value| parse(value.trim_end_matches(';')))
                })
                .expect("theme defines the contrast-checked accent role");

            let remembered_edge = composite(
                foreground,
                sidebar,
                alpha(&property(&remembered, "border-color")),
            );
            let connected_ground = composite(
                foreground,
                sidebar,
                alpha(&property(&connected, "background-color")),
            );
            let connected_edge = composite(
                foreground,
                connected_ground,
                alpha(&property(&connected, "border-color")),
            );
            let active_ground = composite(
                accent,
                sidebar,
                alpha(&property(&active, "background-color")),
            );
            let active_edge = match property(&active, "border-color").as_str() {
                "@reprise_accent_text_color" => accent_edge,
                value if value.starts_with("alpha(@accent_color,") => {
                    composite(accent, active_ground, alpha(value))
                }
                value => panic!("unsupported active edge: {value}"),
            };

            for (step, edge, ground) in [
                ("remembered", remembered_edge, sidebar),
                ("connected", connected_edge, connected_ground),
                ("active", active_edge, active_ground),
            ] {
                let ratio = contrast(edge, ground);
                assert!(
                    ratio >= 3.0,
                    "{theme:?} {} {step} edge reaches only {ratio:.2}:1",
                    if is_dark { "dark" } else { "light" }
                );
            }
        }
    }
}

#[test]
fn device_card_secondary_copy_and_disabled_heading_keep_their_authored_contrast() {
    let css = css();

    assert!(css.contains(
        ".device-card-detail { font-size: 11.5px; color: @reprise_secondary_fg_color; }"
    ));
    assert!(css.contains(".device-section-heading:disabled { background: none; filter: none; }"));
}

#[test]
fn accent_text_surfaces_stay_inside_the_contrast_checked_tint_ceiling() {
    let css = css();
    let background_alpha = |selector: &str| {
        let body = css
            .split_once(selector)
            .and_then(|(_, rest)| rest.split_once('}'))
            .map_or_else(|| panic!("missing rule {selector}"), |(body, _)| body);
        body.split(';')
            .find_map(|declaration| {
                let (property, value) = declaration.split_once(':')?;
                if property.trim() != "background-color" {
                    return None;
                }
                value
                    .trim()
                    .strip_prefix("alpha(@accent_color,")?
                    .trim_end_matches(')')
                    .trim()
                    .parse::<f64>()
                    .ok()
            })
            .unwrap_or_else(|| panic!("missing accent background in {selector}"))
    };
    let active_ground = background_alpha(".device-card-active {");
    let active_hover_ground = background_alpha(".device-card-active:hover {");
    let checked_ceiling: f64 = crate::ui::style::tokens::CHIP_BG_HOVER_ALPHA
        .parse()
        .expect("shared tint ceiling is numeric");

    for (selector, parent_tint) in [
        (
            ".device-card-active .device-card-icon {",
            active_hover_ground,
        ),
        (".device-card-cancel {", active_ground),
        (".device-card-cancel:hover {", active_ground),
    ] {
        let overlay = background_alpha(selector);
        let effective = parent_tint + overlay * (1.0 - parent_tint);
        assert!(
            effective <= checked_ceiling,
            "{selector} produces an effective {effective:.3} tint above the checked {checked_ceiling:.3} ceiling"
        );
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_63_the_cancel_button_exists_only_while_this_device_syncs() {
    gtk4::init().unwrap();
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let cancelled = Rc::new(RefCell::new(Vec::new()));
    let cancelled_for_callback = cancelled.clone();
    let on_cancel: CancelCallback = Rc::new(move |id| cancelled_for_callback.borrow_mut().push(id));
    let card = DeviceCard::new(&view(PlannedSyncPhase::Idle), &on_open, &on_cancel);

    let active = view(PlannedSyncPhase::Finishing);
    card.update(&active);
    assert!(card.cancel_button.is_visible());
    assert_eq!(
        card.cancel_button.icon_name().as_deref(),
        Some(crate::ui::scan_card_css::SIDEBAR_CANCEL_ICON)
    );
    assert_eq!(card.suffix_stack.margin_end(), ACTIVE_SUFFIX_RESERVATION);
    card.cancel_button.emit_clicked();
    assert_eq!(cancelled.borrow().as_slice(), ["pixel"]);

    card.update(&view(PlannedSyncPhase::Idle));
    assert!(!card.cancel_button.is_visible());
    assert_eq!(card.suffix_stack.margin_end(), 0);

    let mut remembered = view(PlannedSyncPhase::Idle);
    remembered.connected = false;
    remembered.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;
    card.update(&remembered);
    assert!(!card.cancel_button.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn css_parses_in_gtk_without_dropping_declarations() {
    if gtk4::init().is_err() {
        return;
    }
    let combined = format!(
        "{}\n{}",
        crate::ui::style::theme::theme_css(
            crate::ui::style::theme::Theme::DEFAULT,
            true,
            crate::ui::style::accent::AccentSource::App,
        ),
        css()
    );
    let errors = crate::ui::style::css_parse_errors(&combined);
    assert!(
        errors.is_empty(),
        "GTK reported CSS parsing errors: {errors:?}"
    );
}

#[test]
#[ignore = "visual fixture; run via the isolated Xvfb screenshot command"]
fn device_card_contrast_ladder_visual_fixture() {
    gtk4::init().unwrap();
    let is_dark = std::env::var("REPRISE_SMOKE_DARK").as_deref() == Ok("1");
    libadwaita::StyleManager::default().set_color_scheme(if is_dark {
        libadwaita::ColorScheme::ForceDark
    } else {
        libadwaita::ColorScheme::ForceLight
    });
    crate::ui::style::install_css_string_for_test(&format!(
        "{}\n{}\n.device-card-fixture {{ background-color: @sidebar_bg_color; color: @window_fg_color; }}",
        crate::ui::style::theme::theme_css(
            crate::ui::style::theme::Theme::DEFAULT,
            is_dark,
            crate::ui::style::accent::AccentSource::App,
        ),
        css()
    ));

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    column.add_css_class("device-card-fixture");
    column.set_margin_top(18);
    column.set_margin_bottom(18);
    column.set_margin_start(18);
    column.set_margin_end(18);
    let heading = gtk4::Label::new(Some("DEVICES"));
    heading.add_css_class("caption");
    heading.add_css_class("device-section-heading");
    heading.set_xalign(0.0);
    column.append(&heading);

    let mut active = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 214,
        total: 1047,
        current_track: "Immortal — Lorna Shore".into(),
        bytes_done: 214,
        bytes_total: 1047,
    });
    active.name = "Pixel 8 · syncing".into();
    let mut connected = view(PlannedSyncPhase::Idle);
    connected.id = "connected".into();
    connected.name = "Walkman · connected".into();
    connected.memory_status = Some("Up to date · synced 12 min ago".into());
    let mut remembered = view(PlannedSyncPhase::Idle);
    remembered.id = "remembered".into();
    remembered.name = "Old phone · remembered".into();
    remembered.connected = false;
    remembered.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;

    let on_open: OpenCallback = Rc::new(|_, _| {});
    let cards = [active, connected, remembered]
        .iter()
        .map(|device| {
            let card = DeviceCard::new(device, &on_open, &no_cancel());
            card.update(device);
            column.append(card.root());
            card
        })
        .collect::<Vec<_>>();
    let window = gtk4::Window::builder()
        .title("Reprise device card contrast ladder")
        .default_width(360)
        .default_height(390)
        .child(&column)
        .build();
    window.present();
    let hold_ms = std::env::var("REPRISE_SMOKE_HOLD_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(250);
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(hold_ms),
    ));
    window.close();
    drop(cards);
}

#[test]
fn mtp_29_deletions_only_idle_card_reads_frees_not_zero_bytes() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::Verified;
    device.target_reading = MusicReading::Diff(diff(0, 0, 3, 148 * 1_024 * 1_024));

    let subtitle = card_subtitle(&device);

    assert_eq!(subtitle, "3 to remove · frees 148.0 MiB");
    assert!(!subtitle.contains("0 B"));
}

#[test]
fn mtp_29_up_to_date_idle_card_names_when_it_last_synced() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::Verified;
    device.last_sync = Some(chrono::Utc::now() - chrono::Duration::minutes(12));

    assert_eq!(card_subtitle(&device), "Up to date · synced 12 min ago");
}

#[test]
fn mtp_29_never_verified_idle_card_prompts_a_scan_instead_of_the_balance() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::NeverVerified;
    device.target_reading = MusicReading::Diff(diff(5, 1, 0, 0));

    assert_eq!(card_subtitle(&device), "Tap to scan device contents");
}

#[test]
fn warnings_keep_an_idle_card_reading_needs_attention() {
    let mut device = view(PlannedSyncPhase::Idle);
    select_playlist(&mut device);
    device
        .page
        .warnings
        .push(reprise_core::device_sync::SyncPageWarning::UnsafeManagedItem);

    assert_eq!(detail_mode(&device), DetailMode::Delta);
    assert_eq!(card_subtitle(&device), "Needs attention");
}

#[test]
fn mtp_29_a_lone_no_playlists_selected_blocker_does_not_read_as_needs_attention() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.storage.access = DeviceStorageAccess::Writable;
    device.storage.free_bytes = Some(2 * 1_024 * 1_024);
    device.contents_state = DeviceContentsState::Verified;
    device
        .page
        .blockers
        .push(reprise_core::device_sync::MirrorBlocker::NoPlaylistsSelected);

    assert_eq!(
        card_subtitle(&device),
        "Up to date",
        "an unselected mirror is not an error — it reads through the ordinary balance states"
    );
}

#[test]
fn mtp_64_sidebar_device_card_delegates_to_the_main_window_page() {
    let source = include_str!("sidebar.rs");
    assert!(source.contains("on_open: Rc<dyn Fn(String, String)>"));
    assert!(!source.contains("device_sync_dialog::present"));
}

#[test]
fn byte_progress_fraction_is_bounded_and_handles_an_unknown_total() {
    assert_eq!(sync_fraction(50, 100), 0.5);
    assert_eq!(sync_fraction(150, 100), 1.0);
    assert_eq!(sync_fraction(50, 0), 0.0);
}

#[test]
fn card_activity_distinguishes_transcoding_and_copying_with_artist() {
    let track = "Immortal — Lorna Shore";

    assert_eq!(
        device_sync_strings::sync_activity(
            sidebar_device_card_text::step_glyph(&SyncStep::Transcoding),
            track,
        ),
        "⟳ transcoding · Immortal — Lorna Shore"
    );
    assert_eq!(
        device_sync_strings::sync_activity(
            sidebar_device_card_text::step_glyph(&SyncStep::Copying),
            track,
        ),
        "↑ Immortal — Lorna Shore"
    );
}

#[test]
fn syncing_title_is_explicit() {
    assert_eq!(
        card_title(&view(PlannedSyncPhase::Finishing)),
        "Syncing Pixel 8"
    );
}

#[test]
fn mtp_64_sidebar_device_card_has_no_direct_sync_action() {
    let direct_sync_action = ["app", "sync-device"].join(".");

    assert!(!include_str!("sidebar_device_card.rs").contains(&direct_sync_action));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_64_cancel_is_an_overlay_sibling_and_both_buttons_are_context_targets() {
    gtk4::init().unwrap();
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&view(PlannedSyncPhase::Finishing), &on_open, &no_cancel());

    assert_eq!(card.surface.parent().as_ref(), Some(card.root.upcast_ref()));
    assert_eq!(
        card.cancel_button.parent().as_ref(),
        Some(card.root.upcast_ref())
    );
    let surface: gtk4::Widget = card.surface.clone().upcast();
    let mut ancestor = card.cancel_button.parent();
    while let Some(widget) = ancestor {
        assert_ne!(
            widget, surface,
            "Cancel must never descend from the card surface"
        );
        ancestor = widget.parent();
    }

    let targets = card.context_menu_targets();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0], &card.surface);
    assert_eq!(targets[1], &card.cancel_button);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn constructed_card_already_projects_its_emphasis_and_cancel_geometry() {
    gtk4::init().unwrap();
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&view(PlannedSyncPhase::Finishing), &on_open, &no_cancel());

    assert!(card.surface.has_css_class("device-card-active"));
    assert!(card.cancel_button.is_visible());
    assert_eq!(card.cancel_button.width_request(), CANCEL_BUTTON_SIZE);
    assert_eq!(card.cancel_button.height_request(), CANCEL_BUTTON_SIZE);
    assert_eq!(card.cancel_button.margin_end(), CANCEL_BUTTON_MARGIN_END);
    assert_eq!(card.suffix_stack.margin_end(), ACTIVE_SUFFIX_RESERVATION);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn cancel_focus_returns_to_the_card_surface_when_cancel_disappears() {
    gtk4::init().unwrap();
    let active = view(PlannedSyncPhase::Finishing);
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&active, &on_open, &no_cancel());
    let window = gtk4::Window::builder().child(card.root()).build();
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(20),
    ));

    assert!(card.cancel_button.grab_focus());
    while gtk4::glib::MainContext::default().pending() {
        gtk4::glib::MainContext::default().iteration(false);
    }
    assert!(card.cancel_button.has_focus());
    card.update(&view(PlannedSyncPhase::Idle));

    assert!(card.surface.has_focus());
    assert!(!card.cancel_button.is_visible());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn device_card_open_is_a_native_keyboard_action() {
    gtk4::init().unwrap();
    let opened = Rc::new(RefCell::new(None));
    let opened_for_callback = opened.clone();
    let on_open: OpenCallback = Rc::new(move |id, name| {
        opened_for_callback.borrow_mut().replace((id, name));
    });
    let card = DeviceCard::new(&view(PlannedSyncPhase::Idle), &on_open, &no_cancel());
    assert!(card.surface.is_focusable());
    card.surface.emit_clicked();
    assert_eq!(
        opened.borrow().as_ref(),
        Some(&("pixel".to_owned(), "Pixel 8".to_owned()))
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_7_disabled_animations_apply_progress_and_state_changes_immediately() {
    if gtk4::init().is_err() {
        return;
    }
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    let device = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 50,
        bytes_total: 100,
    });
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&device, &on_open, &no_cancel());

    card.update(&device);

    assert_eq!(card.progress.fraction(), 0.5);
    assert_eq!(
        card.detail_stack.transition_duration(),
        crate::ui::motion::STANDARD_MS
    );
    assert_eq!(
        card.detail_stack.visible_child_name().as_deref(),
        Some("progress")
    );
    assert_eq!(
        card.indicator.visible_child_name().as_deref(),
        Some("syncing")
    );
    assert!(card.spinner.is_spinning());
    assert!(card.progress_revealer.reveals_child());
    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn enabled_animations_interpolate_progress_to_the_latest_fraction() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    let idle = view(PlannedSyncPhase::Idle);
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&idle, &on_open, &no_cancel());
    assert!(card.root.settings().is_gtk_enable_animations());
    let window = gtk4::Window::new();
    window.set_child(Some(&card.root));
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(20),
    ));
    let syncing = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 50,
        bytes_total: 100,
    });

    card.update(&syncing);

    assert!(card.progress.fraction() < 0.5);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (card.progress.fraction() - 0.5).abs() >= 1e-6 && std::time::Instant::now() < deadline {
        gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
            std::time::Duration::from_millis(20),
        ));
    }
    assert!((card.progress.fraction() - 0.5).abs() < 1e-6);
    window.close();
    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_2_device_background_surfaces_only_crossfade_in_place() {
    gtk4::init().unwrap();
    let device = view(PlannedSyncPhase::Idle);
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let card = DeviceCard::new(&device, &on_open, &no_cancel());

    assert_eq!(
        card.indicator.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
    assert_eq!(
        card.detail_stack.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
    assert_eq!(
        card.suffix_stack.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
    assert_eq!(
        card.progress_revealer.transition_type(),
        gtk4::RevealerTransitionType::Crossfade
    );
}
