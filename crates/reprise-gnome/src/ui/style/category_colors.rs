//! Fixed identities for the three device-sync content categories.

use reprise_core::device_sync::SyncTargetKind;

use super::accent::APP_ACCENT;

/// Music's dark-surface tone is the fixed Reprise teal. Its lowest contrast
/// against the three dark card surfaces is 8.23:1.
const MUSIC_DARK: &str = APP_ACCENT;
/// The brand teal itself is only 1.69:1 against the light card surface, so its
/// fixed light-mode counterpart is darkened to 5.02:1.
const MUSIC_LIGHT: &str = "#147C78";
/// YouTube's tones measure at least 6.72:1 on dark cards and 5.32:1 on light
/// cards.
const YOUTUBE_DARK: &str = "#3FCB8E";
const YOUTUBE_LIGHT: &str = "#1B7A50";
/// Podcasts' tones measure at least 5.15:1 on dark cards and 6.76:1 on light
/// cards.
const PODCASTS_DARK: &str = "#7C9BEE";
const PODCASTS_LIGHT: &str = "#3355B5";

/// Returns a category's fixed identity tone for the current appearance.
///
/// These colors deliberately do not belong to a theme [`super::theme::Palette`]
/// and do not follow the resolved accent source: changing either must not make
/// Music become another hue or collide with another category.
pub(in crate::ui) fn category_color(kind: SyncTargetKind, is_dark: bool) -> &'static str {
    match (kind, is_dark) {
        (SyncTargetKind::Playlists, true) => MUSIC_DARK,
        (SyncTargetKind::Playlists, false) => MUSIC_LIGHT,
        (SyncTargetKind::YoutubeAudio, true) => YOUTUBE_DARK,
        (SyncTargetKind::YoutubeAudio, false) => YOUTUBE_LIGHT,
        (SyncTargetKind::PodcastEpisodes, true) => PODCASTS_DARK,
        (SyncTargetKind::PodcastEpisodes, false) => PODCASTS_LIGHT,
    }
}

pub(in crate::ui) fn category_css_class(kind: SyncTargetKind) -> &'static str {
    match kind {
        SyncTargetKind::Playlists => "reprise-sync-category-music",
        SyncTargetKind::YoutubeAudio => "reprise-sync-category-youtube",
        SyncTargetKind::PodcastEpisodes => "reprise-sync-category-podcasts",
    }
}

pub(super) fn css() -> String {
    format!(
        ".{} {{ color: @reprise_sync_music_color; }}\n\
         .{} {{ color: @reprise_sync_youtube_color; }}\n\
         .{} {{ color: @reprise_sync_podcasts_color; }}\n",
        category_css_class(SyncTargetKind::Playlists),
        category_css_class(SyncTargetKind::YoutubeAudio),
        category_css_class(SyncTargetKind::PodcastEpisodes),
    )
}

pub(super) fn theme_definitions(is_dark: bool) -> String {
    format!(
        "@define-color reprise_sync_music_color {};\n\
         @define-color reprise_sync_youtube_color {};\n\
         @define-color reprise_sync_podcasts_color {};\n",
        category_color(SyncTargetKind::Playlists, is_dark),
        category_color(SyncTargetKind::YoutubeAudio, is_dark),
        category_color(SyncTargetKind::PodcastEpisodes, is_dark),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn sync_categories_have_three_distinct_colours_in_both_modes() {
        for is_dark in [true, false] {
            let colors = SyncTargetKind::ALL.map(|kind| category_color(kind, is_dark));

            assert_eq!(colors.into_iter().collect::<HashSet<_>>().len(), 3);
        }
    }

    #[test]
    fn category_tones_match_design_2c_without_following_the_system_accent() {
        assert_eq!(category_color(SyncTargetKind::Playlists, true), APP_ACCENT);
        assert_eq!(category_color(SyncTargetKind::Playlists, false), "#147C78");
        assert_eq!(
            category_color(SyncTargetKind::YoutubeAudio, true),
            "#3FCB8E"
        );
        assert_eq!(
            category_color(SyncTargetKind::YoutubeAudio, false),
            "#1B7A50"
        );
        assert_eq!(
            category_color(SyncTargetKind::PodcastEpisodes, true),
            "#7C9BEE"
        );
        assert_eq!(
            category_color(SyncTargetKind::PodcastEpisodes, false),
            "#3355B5"
        );
    }

    #[test]
    fn row_icon_classes_resolve_through_mode_aware_category_roles() {
        let css = css();
        for (kind, class, role) in [
            (
                SyncTargetKind::Playlists,
                "reprise-sync-category-music",
                "reprise_sync_music_color",
            ),
            (
                SyncTargetKind::YoutubeAudio,
                "reprise-sync-category-youtube",
                "reprise_sync_youtube_color",
            ),
            (
                SyncTargetKind::PodcastEpisodes,
                "reprise-sync-category-podcasts",
                "reprise_sync_podcasts_color",
            ),
        ] {
            assert_eq!(category_css_class(kind), class);
            assert!(
                css.contains(&format!(".{class} {{ color: @{role}; }}")),
                "missing class rule for {kind:?}"
            );
            for is_dark in [true, false] {
                assert!(
                    theme_definitions(is_dark).contains(&format!(
                        "@define-color {role} {};",
                        category_color(kind, is_dark)
                    )),
                    "missing dark={is_dark} named color for {kind:?}"
                );
            }
        }
    }

    #[test]
    fn category_tones_keep_text_level_contrast_on_every_card_surface() {
        for theme in super::super::theme::Theme::all() {
            for (is_dark, card_bg) in [
                (true, theme.palette().card_bg),
                (false, theme.light_palette().card_bg),
            ] {
                for kind in SyncTargetKind::ALL {
                    let ratio = contrast_ratio(category_color(kind, is_dark), card_bg);
                    assert!(
                        ratio >= 4.5,
                        "{kind:?} is only {ratio:.2}:1 on {theme:?}'s card in dark={is_dark}"
                    );
                }
            }
        }
    }

    fn contrast_ratio(first: &str, second: &str) -> f64 {
        let (lighter, darker) = {
            let first = luminance(first);
            let second = luminance(second);
            if first > second {
                (first, second)
            } else {
                (second, first)
            }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn luminance(hex: &str) -> f64 {
        let hex = hex.strip_prefix('#').expect("color uses #RRGGBB");
        let linear = |offset| {
            let channel = f64::from(
                u8::from_str_radix(&hex[offset..offset + 2], 16)
                    .expect("color uses hexadecimal channels"),
            ) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(0) + 0.7152 * linear(2) + 0.0722 * linear(4)
    }
}
