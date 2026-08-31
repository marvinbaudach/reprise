use super::tests::view;
use super::*;
use reprise_core::device_sync::{
    AnalysisSidecarWrite, DesiredManagedFile, DeviceFileRecord, DevicePlaylistRecord,
    DeviceSyncMachine, Effect, Event, ManagedRemoval, MirrorPlan, PlaylistWrite, SelectionSource,
    SyncTrack, TransferAction,
};
use std::path::PathBuf;

fn no_cancel() -> CancelCallback {
    Rc::new(|_| {})
}

#[test]
fn css_covers_the_sync_card_vocabulary() {
    let css = css();
    for marker in [
        ".device-card {",
        ".device-card-active:hover",
        ".device-card-current.device-card-remembered",
        ".device-card-current.device-card-connected",
        ".device-card-current.device-card-active",
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
        unit_bytes_done: 214,
        unit_bytes_total: 1047,
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
fn mtp_64_sidebar_device_card_delegates_to_the_main_window_page() {
    let source = include_str!("sidebar_playlist_notifications.rs");
    assert!(source.contains("on_open: Rc<dyn Fn(String, String)>"));
    assert!(!source.contains("device_sync_dialog::present"));
}

#[test]
fn byte_progress_fraction_is_bounded_and_handles_an_unknown_total() {
    assert_eq!(sync_fraction(0, 1, 50, 100), 0.5);
    assert_eq!(sync_fraction(1, 1, 50, 100), 1.0);
    assert_eq!(sync_fraction(0, 0, 50, 100), 0.0);
}

#[test]
fn displayed_fraction_is_monotonic_across_every_kind_of_planned_work() {
    let mut plan = MirrorPlan::default();
    plan.copy.push(desired_track());
    plan.analysis_writes.push(AnalysisSidecarWrite {
        track_id: 1,
        device_path: "Reprise/1.reprise-analysis".into(),
        size_bytes: 10,
        existing_size_bytes: None,
    });
    plan.playlist_writes.push(playlist_write(7));
    plan.playlist_removals.push(playlist_record(8));
    plan.remove
        .push(ManagedRemoval::Inventory(existing_track()));
    plan.transfer_bytes = 110;

    let mut machine = DeviceSyncMachine::new("serial-1".into(), plan);
    let mut effects = machine.dispatch(Event::Start);
    let mut phases = vec![machine.phase().clone()];
    while let Some(effect) = effects.pop() {
        let Some(event) = successful_event(&mut machine, &mut phases, effect) else {
            break;
        };
        effects = machine.dispatch(event);
        phases.push(machine.phase().clone());
    }

    let syncing = phases
        .iter()
        .filter_map(|phase| match phase {
            PlannedSyncPhase::Syncing {
                done,
                total,
                unit_bytes_done,
                unit_bytes_total,
                ..
            } => Some((
                *done,
                *total,
                sync_fraction(*done, *total, *unit_bytes_done, *unit_bytes_total),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let displayed_fractions = phases
        .iter()
        .filter_map(|phase| match phase {
            PlannedSyncPhase::Syncing {
                done,
                total,
                unit_bytes_done,
                unit_bytes_total,
                ..
            } => Some(sync_fraction(
                *done,
                *total,
                *unit_bytes_done,
                *unit_bytes_total,
            )),
            PlannedSyncPhase::Finishing => Some(1.0),
            PlannedSyncPhase::Idle | PlannedSyncPhase::ComputingDelta => None,
        })
        .collect::<Vec<_>>();

    assert!(
        displayed_fractions
            .windows(2)
            .all(|pair| pair[0] <= pair[1]),
        "displayed fractions moved backwards: {displayed_fractions:?}"
    );
    assert_eq!(
        displayed_fractions
            .iter()
            .filter(|fraction| **fraction == 1.0)
            .count(),
        1,
        "the displayed fraction must reach exactly 1.0 exactly once: {displayed_fractions:?}"
    );
    assert!(
        syncing.iter().all(|(_, total, _)| *total == 5),
        "the run-wide total changed: {syncing:?}"
    );
    assert!(
        syncing.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "done moved backwards: {syncing:?}"
    );
}

fn successful_event(
    machine: &mut DeviceSyncMachine,
    phases: &mut Vec<PlannedSyncPhase>,
    effect: Effect,
) -> Option<Event> {
    Some(match effect {
        Effect::CleanPartials => Event::PartialsCleaned(Ok(())),
        Effect::CopyTrack { bytes, .. } => {
            machine.dispatch(Event::CopyProgress { copied: bytes / 2 });
            phases.push(machine.phase().clone());
            Event::TrackCopied(Ok(bytes))
        }
        Effect::RecordFile { .. } => Event::FileRecorded(Ok(())),
        Effect::WriteAnalysis { index } => {
            machine.dispatch(Event::CopyProgress {
                copied: machine.plan().analysis_writes[index].size_bytes / 2,
            });
            phases.push(machine.phase().clone());
            Event::AnalysisWritten(Ok(machine.plan().analysis_writes[index].size_bytes))
        }
        Effect::WritePlaylist { .. } => Event::PlaylistWritten(Ok(())),
        Effect::RecordPlaylist { .. } => Event::PlaylistRecorded(Ok(())),
        Effect::RemovePlaylist { .. } => Event::PlaylistRemoved(Ok(())),
        Effect::ForgetPlaylist { .. } => Event::PlaylistForgotten(Ok(())),
        Effect::RemoveTrack { .. } => Event::TrackRemoved(Ok(())),
        Effect::ForgetFile { .. } => Event::FileForgotten(Ok(())),
        Effect::Finished(_) => return None,
        unexpected => panic!("unexpected effect: {unexpected:?}"),
    })
}

fn desired_track() -> DesiredManagedFile {
    DesiredManagedFile {
        track: SyncTrack {
            id: 1,
            source_path: PathBuf::from("/music/1.flac"),
            original_name: "1.flac".into(),
            title: "Track 1".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            track_number: Some(1),
            duration_ms: 180_000,
            bitrate_kbps: Some(1000),
            size_bytes: 100,
            source_mtime: 0,
        },
        device_path: "Reprise/1.opus".into(),
        target_bytes: 100,
        profile_fingerprint: "fingerprint".into(),
        action: TransferAction::CopyOriginal,
    }
}

fn playlist_write(id: i64) -> PlaylistWrite {
    PlaylistWrite {
        source: SelectionSource::Playlist(id),
        source_name: format!("Playlist {id}"),
        device_path: format!("Reprise/Playlist {id}.m3u8"),
        entries: Vec::new(),
        contents: "#EXTM3U\n".into(),
    }
}

fn playlist_record(id: i64) -> DevicePlaylistRecord {
    DevicePlaylistRecord {
        device_serial: "serial-1".into(),
        source: SelectionSource::Playlist(id),
        source_name: format!("Playlist {id}"),
        device_path: format!("Reprise/Playlist {id}.m3u8"),
        last_synced_at: None,
    }
}

fn existing_track() -> DeviceFileRecord {
    DeviceFileRecord {
        device_serial: "serial-1".into(),
        track_id: 9,
        source_path: "/music/9.flac".into(),
        source_size: 10,
        source_mtime: 0,
        device_path: "Reprise/9.opus".into(),
        device_size: 10,
        profile_fingerprint: "old".into(),
        pinned: false,
    }
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
        unit_bytes_done: 50,
        unit_bytes_total: 100,
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
        unit_bytes_done: 50,
        unit_bytes_total: 100,
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
