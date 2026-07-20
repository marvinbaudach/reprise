//! The single application-wide CSS provider plus the live theme palette.
//!
//! Feature modules own their CSS class names and contribute one plain
//! `css()` string section; this module composes those sections, installs
//! them exactly once per process, and keeps the tunable design values in
//! [`tokens`]. Widget constructors must never install providers themselves —
//! per-widget installation used to add one display-global provider on every
//! rebuild.
//!
//! The palette lives in its own [`theme`] provider (separate from the
//! structural CSS above) so [`set_theme`] can recolor the whole app by
//! reloading just that one provider — the mechanism behind the live theme
//! picker.

pub(super) mod buttons;
pub(super) mod cover_accent;
pub(super) mod interactions;
pub(super) mod menus;
pub(super) mod reduced_motion;
mod text_levels;
pub(super) mod theme;
pub(super) mod tokens;

use std::cell::{Cell, RefCell};

use libadwaita as adw;

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// The dedicated palette provider, kept so [`set_theme`] can reload it.
    static THEME_PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
    /// The currently active theme, kept so appearance changes can reload the
    /// palette CSS with the correct theme without needing the database.
    static CURRENT_THEME: Cell<theme::Theme> = const { Cell::new(theme::Theme::DEFAULT) };
}

/// One entry per feature that ships app-authored (theme-independent) CSS.
/// Palette colors are NOT here — they live in the separate theme provider.
fn app_css() -> String {
    [
        buttons::css(),
        interactions::css(),
        text_levels::css(),
        super::link_activation::css(),
        menus::css(),
        super::browse_bar::css(),
        super::column_header_dnd::css(),
        super::column_layout_editor::css(),
        super::eq_bars::css(),
        super::playing_marker::css(),
        super::sidebar_device_card::css(),
        super::device_view::css(),
        super::list_density::css(),
        super::library_chrome::css(),
        super::library_view_css::css(),
        super::album_card_css::css(),
        super::album_glow::css(),
        super::artist_view_css::css(),
        super::now_playing::css(),
        super::lyrics_view::css(),
        super::player_bar_layout::css(),
        super::preference_choice_cards::css(),
        super::preference_playback::css(),
        super::preference_plugins::css(),
        super::preferences_window::css(),
        super::rating::css(),
        super::track_content::css(),
        super::track_list_header_style::css(),
        super::track_list_row_interaction::css(),
        super::stats_css::css(),
        super::toasts::css(),
        super::tag_editor_style::css(),
        info_panel_clip_css(),
        super::issues::css(),
        super::compact_player_layouts::mini_css(),
        super::scan_card_css::css(),
    ]
    .join("\n")
}

#[cfg(test)]
pub(in crate::ui) fn css_parse_errors(css: &str) -> Vec<String> {
    use std::rc::Rc;

    let errors = Rc::new(RefCell::new(Vec::new()));
    let provider = gtk4::CssProvider::new();
    {
        let errors = errors.clone();
        provider.connect_parsing_error(move |_, section, error| {
            errors.borrow_mut().push(format!("{section:?}: {error}"));
        });
    }
    provider.load_from_string(css);
    drop(provider);
    Rc::try_unwrap(errors)
        .expect("CSS parser callback releases its error collector")
        .into_inner()
}

/// The OverlaySplitView positions children with GPU transforms without
/// clipping the content pane. Clip the internal wrapper widgets so resized
/// columns cannot paint behind the info-panel sidebar.
fn info_panel_clip_css() -> String {
    concat!(
        "overlay-split-view > widget { overflow: hidden; } ",
        "overlay-split-view > widget > * { overflow: hidden; } ",
    )
    .into()
}

/// Installs the structural app CSS and the default theme palette on the
/// default display, once per process, and applies the default color scheme. Safe to call from every window/test
/// entry point; calls before GTK has a default display (e.g. plain unit
/// tests) are ignored without arming the once-guard, so a later call after
/// `gtk4::init` still installs.
pub(super) fn install() {
    INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
        let Some(display) = gtk4::gdk::Display::default() else {
            return;
        };
        installed.set(true);

        let provider = gtk4::CssProvider::new();
        provider.load_from_string(&app_css());
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let is_dark = adw::StyleManager::default().is_dark();
        let theme_provider = gtk4::CssProvider::new();
        theme_provider.load_from_string(&theme::theme_css(theme::Theme::DEFAULT, is_dark));
        gtk4::style_context_add_provider_for_display(
            &display,
            &theme_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        THEME_PROVIDER.with(|slot| *slot.borrow_mut() = Some(theme_provider));

        cover_accent::install(&display);
        reduced_motion::install(&display);

        adw::StyleManager::default().connect_dark_notify(|_| {
            reload_theme_for_appearance();
        });
    });
}

/// Recolors the whole app to `theme` by reloading the palette provider. A
/// no-op before [`install`] has run (no display yet).
pub(in crate::ui) fn set_theme(theme: theme::Theme) {
    CURRENT_THEME.with(|slot| slot.set(theme));
    let is_dark = adw::StyleManager::default().is_dark();
    THEME_PROVIDER.with(|slot| {
        if let Some(provider) = slot.borrow().as_ref() {
            provider.load_from_string(&theme::theme_css(theme, is_dark));
        }
    });
}

/// Sets the libadwaita color scheme preference and reloads the current theme
/// palette so the dark/light variant matches the new scheme.
pub(in crate::ui) fn set_color_scheme(scheme: &str) {
    let adw_scheme = match scheme {
        "dark" => adw::ColorScheme::ForceDark,
        "light" => adw::ColorScheme::ForceLight,
        _ => adw::ColorScheme::Default,
    };
    adw::StyleManager::default().set_color_scheme(adw_scheme);
}

/// Reloads the current theme's palette CSS to match the current system
/// appearance (dark or light). Called from the `dark_notify` signal handler.
fn reload_theme_for_appearance() {
    let theme = CURRENT_THEME.with(std::cell::Cell::get);
    let is_dark = adw::StyleManager::default().is_dark();
    THEME_PROVIDER.with(|slot| {
        if let Some(provider) = slot.borrow().as_ref() {
            provider.load_from_string(&theme::theme_css(theme, is_dark));
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn style_2_side_surfaces_follow_the_theme() {
        let css = super::app_css();

        assert!(css.contains(
            ".reprise-library-split .reprise-library-sidebar { background-color: @sidebar_bg_color;"
        ));
        assert!(css.contains(".reprise-now-playing-stage { background-color: @sidebar_bg_color;"));
        assert!(css.contains("border-right: 1px solid rgba(255, 255, 255, 0.06)"));
        assert!(css.contains("border-left: 1px solid rgba(255, 255, 255, 0.06)"));

        for theme in super::theme::Theme::all() {
            for (is_dark, palette) in [(true, theme.palette()), (false, theme.light_palette())] {
                assert!(
                    super::theme::theme_css(theme, is_dark).contains(&format!(
                        "@define-color sidebar_bg_color {};",
                        palette.sidebar_bg
                    )),
                    "{theme:?} did not project its sidebar surface into both consumers"
                );
            }
        }
    }

    #[test]
    fn app_css_contains_every_feature_section() {
        let css = super::app_css();

        for marker in [
            ".reprise-equalizer scale > trough > highlight",
            "popover.menu > contents",
            ".toast button.text-button",
            ".player-bar-play",
            ".reprise-surface",
            ".reprise-hover:hover",
            ".reprise-btn-icon:hover",
            ".reprise-btn-toggle:checked",
            ".reprise-text-primary",
            ".reprise-text-secondary",
            ".reprise-text-hint",
            ".reprise-filter-chip",
            ".reprise-column-drop-before",
            ".reprise-column-row:hover",
            ".reprise-track-cell.reprise-density-comfortable",
            ".album-card",
            ".album-now-playing-glow",
            ".library-album-card",
            ".artist-list-row",
            ".reprise-now-playing-stage",
            ".lyrics-line-active",
            "checkbutton.reprise-choice-card",
            ".reprise-rating-star",
            ".reprise-list-status-bar",
            ".reprise-track-list > header label",
            ".reprise-reorder-target",
            ".stats-chart",
            ".reprise-tag-editor",
            "floating-sheet > sheet",
            ".issue-card",
            ".mini-player-card",
            ".scan-card",
            ".device-storage-music progress",
        ] {
            assert!(css.contains(marker), "missing section marker: {marker}");
        }
    }

    #[test]
    fn app_css_has_no_palette_colors() {
        // Palette @define-colors belong to the separate theme provider, not
        // the structural CSS, so a theme switch never has to rebuild this.
        assert!(!super::app_css().contains("@define-color"));
    }

    #[test]
    fn install_without_a_display_does_not_arm_the_once_guard() {
        // Plain unit-test processes have no GDK display; install() must stay
        // re-runnable so the first real (xvfb) caller still gets the CSS.
        if gtk4::gdk::Display::default().is_some() {
            return;
        }
        super::install();
        super::install();
        super::INSTALLED.with(|installed| assert!(!installed.get()));
    }

    /// Iterates the default main context until `ms` wall-clock milliseconds
    /// have passed, so frame-clock driven CSS animation can progress.
    fn pump_ms(ms: u64) {
        let done = std::rc::Rc::new(std::cell::Cell::new(false));
        let done_setter = done.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
            done_setter.set(true);
        });
        let context = gtk4::glib::MainContext::default();
        while !done.get() {
            context.iteration(true);
        }
    }

    /// CSS T-V probe (motion plan, task T2): establishes whether GTK's CSS
    /// `transition:` and `@keyframes` machinery follows
    /// `gtk-enable-animations=false`. A probe box animates `min-height`
    /// 10px⇄90px over 600 ms; sampling mid-flight distinguishes an
    /// interpolating run from a hard switch. The finding is recorded in the
    /// `ui/motion.rs` module docs (MOT-7 contract).
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_7_css_honours_enable_animations_setting() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        use gtk4::prelude::*;

        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();

        let provider = gtk4::CssProvider::new();
        provider.load_from_string(
            ".tv-probe { min-height: 10px; transition: min-height 600ms linear; }
             .tv-probe.tv-grown { min-height: 90px; }
             @keyframes tv-loop { from { min-height: 10px; } to { min-height: 90px; } }
             .tv-probe.tv-looping { animation: tv-loop 600ms linear infinite alternate; }",
        );
        let display = gtk4::gdk::Display::default().unwrap();
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let probe = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        probe.add_css_class("tv-probe");
        probe.set_valign(gtk4::Align::Start);
        let window = gtk4::Window::new();
        window.set_default_size(300, 300);
        window.set_child(Some(&probe));
        window.present();
        pump_ms(100);
        assert_eq!(probe.height(), 10);

        // Baseline: with animations enabled the transition must interpolate,
        // otherwise this environment cannot answer the question at all.
        settings.set_gtk_enable_animations(true);
        probe.add_css_class("tv-grown");
        pump_ms(200);
        let mid_enabled = probe.height();
        pump_ms(1200);
        let end_enabled = probe.height();
        probe.remove_css_class("tv-grown");
        pump_ms(1200);

        // The same style change with animations disabled.
        settings.set_gtk_enable_animations(false);
        probe.add_css_class("tv-grown");
        pump_ms(200);
        let mid_disabled = probe.height();
        pump_ms(1200);
        let end_disabled = probe.height();
        probe.remove_css_class("tv-grown");
        pump_ms(1200);

        // Keyframes while animations stay disabled: two mid-cycle samples.
        probe.add_css_class("tv-looping");
        pump_ms(150);
        let loop_sample_a = probe.height();
        pump_ms(220);
        let loop_sample_b = probe.height();
        probe.remove_css_class("tv-looping");

        println!(
            "T-V probe: enabled mid={mid_enabled} end={end_enabled}; \
             disabled mid={mid_disabled} end={end_disabled}; \
             disabled keyframes samples a={loop_sample_a} b={loop_sample_b}"
        );

        settings.set_gtk_enable_animations(previous);
        window.close();

        assert_eq!(end_enabled, 90);
        assert!(
            mid_enabled > 10 && mid_enabled < 90,
            "baseline transition did not interpolate (mid={mid_enabled}) — probe inconclusive"
        );
        // Finding (first executed 2026-07-18 on Xvfb): CSS honours the setting.
        assert_eq!(
            mid_disabled, 90,
            "CSS transition interpolated despite gtk-enable-animations=false"
        );
        assert_eq!(end_disabled, 90);
        assert_eq!(
            (loop_sample_a, loop_sample_b),
            (10, 10),
            "CSS @keyframes kept running despite gtk-enable-animations=false"
        );
    }
}
