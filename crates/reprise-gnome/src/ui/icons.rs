//! Icon names the app leans on, where the obvious name is the wrong one — and
//! the guard that stops the next wrong one from shipping.
//!
//! Only names with a reason to be here. Everything else stays inline at its
//! call site, which is this codebase's habit.

/// "This is done." **Not** `emblem-ok-symbolic`, which is what seven call sites
/// used to say: that name is absent from the installed Adwaita symbolic set
/// (`adwaita-icon-theme 50` dropped it, and `adwaita-icon-theme-legacy` does not
/// carry it either), so GTK silently drew the missing-image box instead of a
/// checkmark. Nothing catches that by itself — it is a string that resolves at
/// runtime — and in a screenshot the box reads as a small rectangle that looks
/// like a layout detail. `every_icon_name_the_app_asks_for_can_be_drawn` below
/// is what catches it now.
pub(in crate::ui) const DONE: &str = "object-select-symbolic";

/// Quiet context for an Apple search result whose visible title and publisher
/// do not contain the query. Present in the installed Adwaita symbolic set.
pub(in crate::ui) const UNEXPLAINED_SEARCH_MATCH: &str = "dialog-information-symbolic";

/// Lyrics are words carried by music: three text lines beside one note. This
/// app-owned symbolic avoids presenting the tab as document editing, which it
/// is not.
pub(in crate::ui) const LYRICS: &str = "reprise-lyrics-symbolic";

/// Four unequal level bars, matching the Bars mode shown by the page. This is
/// intentionally not a network-strength glyph borrowed from the system theme.
pub(in crate::ui) const VISUAL_BARS: &str = "reprise-visual-bars-symbolic";

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// Names this system's icon theme cannot draw, each paired **in code** with
    /// a name it can. Asking for a nicer icon and accepting a plainer one is
    /// fine; asking and not noticing is the bug this guard is about. Every
    /// entry names the guard that makes it safe, so a reader can check the
    /// claim instead of trusting the list.
    const GUARDED: &[(&str, &str)] = &[
        (
            "ticket-symbolic",
            "sidebar_presentation::NavIcon::fallback_icon_name → x-office-calendar-symbolic",
        ),
        (
            "external-link-symbolic",
            "updates::release_row_actions::icon_with_fallback → web-browser-symbolic",
        ),
        (
            "io.github.marvinbaudach.Reprise-first-aid-symbolic",
            "library_doctor::doctor_glyph, and sidebar_presentation::NavIcon::LibraryDoctor \
             through nav_icon's theme check, both → system-search-symbolic",
        ),
    ];

    fn ui_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui")
    }

    /// Every `"…-symbolic"` literal under `src/ui`.
    ///
    /// A literal is the only form GTK ever sees: `Image::from_icon_name` takes a
    /// string, so a name that does not exist is not a compile error anywhere.
    /// Reading the sources back is therefore the only way to enumerate what the
    /// app actually asks the theme for.
    fn icon_names_in_sources(directory: &Path) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let Ok(entries) = std::fs::read_dir(directory) else {
            return names;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                names.extend(icon_names_in_sources(&path));
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            for literal in source.split('"').skip(1).step_by(2) {
                // A whole name, not a fragment: `radio_columns` assembles two of
                // its names with `["starred", "-symbolic"].concat()`, and the
                // suffix on its own is not something GTK is ever asked for.
                // Names built that way are invisible to this scan — the price of
                // reading sources rather than running every widget.
                let is_whole_name = literal.len() > "-symbolic".len()
                    && !literal.starts_with('-')
                    && literal.ends_with("-symbolic")
                    && literal.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '-' | '.')
                    });
                if is_whole_name {
                    names.insert(literal.to_owned());
                }
            }
        }
        names
    }

    #[test]
    fn the_icon_scan_finds_the_names_this_module_declares() {
        let names = icon_names_in_sources(&ui_root());
        assert!(
            names.contains(super::DONE),
            "the scan must see the names it is meant to check: {names:?}"
        );
        assert!(
            names.contains(super::UNEXPLAINED_SEARCH_MATCH),
            "the scan must guard the podcast search marker: {names:?}"
        );
        assert!(
            names.contains(crate::ui::library_doctor::DOCTOR_GLYPH),
            "the scan must guard the app-ID-prefixed first-aid icon: {names:?}"
        );
        assert!(names.len() > 40, "only {} names found", names.len());
    }

    #[test]
    fn embedded_private_symbolics_need_no_system_fallback_guard() {
        let guarded = GUARDED
            .iter()
            .map(|(name, _)| *name)
            .collect::<BTreeSet<_>>();
        for name in ["reprise-radio-symbolic", "reprise-stats-symbolic"] {
            assert!(
                !guarded.contains(name),
                "{name} is embedded and must be checked as directly drawable"
            );
        }
    }

    #[test]
    fn private_symbolics_are_embedded_at_icon_theme_paths() {
        crate::register_app_resources();
        for name in [
            super::LYRICS,
            super::VISUAL_BARS,
            "reprise-radio-symbolic",
            "reprise-stats-symbolic",
        ] {
            let path =
                format!("/io/github/marvinbaudach/Reprise/icons/scalable/actions/{name}.svg");
            let bytes =
                gtk4::gio::resources_lookup_data(&path, gtk4::gio::ResourceLookupFlags::NONE)
                    .unwrap_or_else(|error| panic!("{path} is not embedded: {error}"));
            let bytes = bytes.as_ref();
            assert!(
                bytes.windows(b"<svg".len()).any(|window| window == b"<svg")
                    && bytes.ends_with(b"</svg>\n"),
                "{path} is not a complete SVG"
            );
        }
    }

    /// The app may not ask for an icon this system cannot draw.
    ///
    /// Environment-dependent on purpose: it asks the icon theme that is actually
    /// installed, because that is the one the user sees. On a machine whose
    /// theme is missing a name this test names it, which is the point — a red
    /// test instead of a silent missing-image box.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn every_icon_name_the_app_asks_for_can_be_drawn() {
        crate::register_app_resources();
        if gtk4::init().is_err() {
            return;
        }
        crate::install_app_icon_resource_path();
        let Some(display) = gtk4::gdk::Display::default() else {
            return;
        };
        let theme = gtk4::IconTheme::for_display(&display);
        let guarded = GUARDED
            .iter()
            .map(|(name, _)| *name)
            .collect::<BTreeSet<_>>();
        let missing = icon_names_in_sources(&ui_root())
            .into_iter()
            .filter(|name| !guarded.contains(name.as_str()))
            .filter(|name| !theme.has_icon(name))
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "the theme cannot draw these, and nothing in the code falls back for them — \
             GTK will render the missing-image box: {missing:?}"
        );
    }

    /// The fallbacks the guarded names rely on have to exist themselves.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn every_guarded_name_falls_back_to_one_that_exists() {
        crate::register_app_resources();
        if gtk4::init().is_err() {
            return;
        }
        crate::install_app_icon_resource_path();
        let Some(display) = gtk4::gdk::Display::default() else {
            return;
        };
        let theme = gtk4::IconTheme::for_display(&display);
        for (name, guard) in GUARDED {
            let fallback = guard.rsplit_once('→').map_or_else(
                || panic!("{name}: the guard note must name its fallback"),
                |(_, fallback)| fallback.trim(),
            );
            assert!(
                theme.has_icon(fallback),
                "{name} falls back to {fallback}, which this theme cannot draw either"
            );
        }
    }
}
