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

pub(super) mod cover_accent;
pub(super) mod interactions;
pub(super) mod menus;
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
        interactions::css(),
        menus::css(),
        super::browse_bar::css(),
        super::column_layout_editor::css(),
        super::eq_bars::css(),
        super::list_density::css(),
        super::library_chrome::css(),
        super::library_view_css::css(),
        super::album_card_css::css(),
        super::lyrics_view::css(),
        super::player_bar_layout::css(),
        super::preference_choice_cards::css(),
        super::preference_playback::css(),
        super::rating::css(),
        super::track_list_header_style::css(),
        super::track_list_row_interaction::css(),
        super::stats_css::css(),
        super::toasts::css(),
        super::tag_editor_style::css(),
        info_panel_clip_css(),
        super::compact_player_layouts::mini_css(),
        super::scan_card_css::css(),
    ]
    .join("\n")
}

/// The OverlaySplitView positions children with GPU transforms without
/// clipping the content pane. Clip the internal wrapper widgets so resized
/// columns cannot paint behind the info-panel sidebar.
fn info_panel_clip_css() -> String {
    concat!(
        "overlay-split-view > widget { overflow: hidden; } ",
        "overlay-split-view > widget > * { overflow: hidden; } ",
    ).into()
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
    let theme = CURRENT_THEME.with(|slot| slot.get());
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
    fn app_css_contains_every_feature_section() {
        let css = super::app_css();

        for marker in [
            ".reprise-equalizer scale > trough > highlight",
            "popover.menu > contents",
            ".toast button.text-button",
            ".player-bar-play",
            ".reprise-surface",
            ".reprise-hover:hover",
            ".reprise-filter-chip",
            ".reprise-column-drop-before",
            ".reprise-column-row:hover",
            ".reprise-track-cell.reprise-density-comfortable",
            ".album-card",
            ".lyrics-line-active",
            "checkbutton.reprise-choice-card",
            ".reprise-rating-star",
            ".reprise-track-list > header label",
            ".reprise-reorder-target",
            ".stats-chart",
            ".reprise-tag-editor",
            "floating-sheet > sheet",
            ".mini-player-card",
            ".scan-card",
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
}
