use std::cell::Cell;
use std::rc::Rc;

use reprise_core::playback::PlaybackState;

use super::player_controller::NowPlaying;
use super::strings;
use crate::ui::playback::external_media::ExternalPlaybackSnapshot;

pub(super) const UP_NEXT_PAGE: &str = "up-next";
pub(super) const LYRICS_PAGE: &str = "lyrics";
pub(super) const VISUAL_PAGE: &str = "visual";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum PanelTab {
    #[default]
    UpNext,
    Lyrics,
    Visual,
}

pub(super) const PANEL_TABS: [PanelTab; 3] = [PanelTab::UpNext, PanelTab::Lyrics, PanelTab::Visual];

impl PanelTab {
    pub(super) fn page_name(self) -> &'static str {
        match self {
            Self::UpNext => UP_NEXT_PAGE,
            Self::Lyrics => LYRICS_PAGE,
            Self::Visual => VISUAL_PAGE,
        }
    }

    pub(super) fn from_page_name(name: &str) -> Option<Self> {
        PANEL_TABS.into_iter().find(|tab| tab.page_name() == name)
    }

    pub(super) const fn icon_name(self) -> &'static str {
        match self {
            Self::UpNext => "view-list-symbolic",
            Self::Lyrics => crate::ui::icons::LYRICS,
            Self::Visual => crate::ui::icons::VISUAL_BARS,
        }
    }
}

pub(super) fn should_render_up_next(panel_visible: bool, selected_tab: PanelTab) -> bool {
    panel_visible && selected_tab == PanelTab::UpNext
}

pub(super) fn album_label_text(artist: &str, album: &str) -> String {
    if album.is_empty() {
        return String::new();
    }
    if artist.is_empty() {
        return album.to_owned();
    }
    format!(" ·\u{00a0}{album}")
}

/// The page the stack has to show after `tab` became unavailable (`NPP-15`).
///
/// `None` means "leave the selection alone". Both callers — the Lyrics tab
/// yielding to an external session and the Visual tab following the Song
/// Visuals module — ask here rather than repeating the condition, because the
/// same decision written twice is how two audible bugs got in before.
pub(super) fn page_after_tab_hidden(
    selected: PanelTab,
    tab: PanelTab,
    tab_visible: bool,
) -> Option<&'static str> {
    (!tab_visible && selected == tab).then_some(UP_NEXT_PAGE)
}

#[derive(Default)]
pub(super) struct TabSession {
    pub(super) selected: Cell<PanelTab>,
}

#[derive(Default)]
pub(super) struct TabFooters {
    pub(super) up_next: String,
    pub(super) lyrics: String,
    pub(super) visual: String,
}

#[cfg(test)]
mod tests {
    use super::{album_label_text, page_after_tab_hidden, PanelTab, PANEL_TABS, UP_NEXT_PAGE};

    #[test]
    fn album_label_text_joins_only_the_present_artist_and_album() {
        assert_eq!(album_label_text("Artist", "Album"), " ·\u{00a0}Album");
        assert_eq!(album_label_text("", "Album"), "Album");
        assert_eq!(album_label_text("Artist", ""), "");
        assert_eq!(album_label_text("", ""), "");
        assert_eq!(album_label_text("Artist", "A & B"), " ·\u{00a0}A & B");
    }

    #[test]
    fn npp_14_has_the_three_built_in_tabs_in_order() {
        assert_eq!(
            PANEL_TABS,
            [PanelTab::UpNext, PanelTab::Lyrics, PanelTab::Visual]
        );
    }

    #[test]
    fn npp_15_hiding_the_selected_tab_falls_back_to_up_next() {
        assert_eq!(
            page_after_tab_hidden(PanelTab::Lyrics, PanelTab::Lyrics, false),
            Some(UP_NEXT_PAGE)
        );
        assert_eq!(
            page_after_tab_hidden(PanelTab::Visual, PanelTab::Visual, false),
            Some(UP_NEXT_PAGE)
        );
        assert_eq!(
            page_after_tab_hidden(PanelTab::Lyrics, PanelTab::Visual, false),
            None
        );
        assert_eq!(
            page_after_tab_hidden(PanelTab::Visual, PanelTab::Visual, true),
            None
        );
    }
}

thread_local! {
    pub(super) static TAB_SESSION: Rc<TabSession> = Rc::new(TabSession::default());
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PanelPresentation {
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) idle: bool,
}

pub(super) fn panel_presentation(
    track: Option<&NowPlaying>,
    _playback_state: PlaybackState,
) -> PanelPresentation {
    let Some(track) = track else {
        return PanelPresentation {
            title: strings::text(strings::NOW_PLAYING_NOTHING),
            subtitle: String::new(),
            idle: true,
        };
    };
    let subtitle = match (track.artist.trim(), track.album.trim()) {
        ("", "") => String::new(),
        (artist, "") => artist.to_owned(),
        ("", album) => album.to_owned(),
        (artist, album) => format!("{artist} · {album}"),
    };
    PanelPresentation {
        title: track.title.clone(),
        subtitle,
        idle: false,
    }
}

/// `idle` is the panel's "nothing is loaded at all": no queue track and no
/// external session. `render_track` hangs both the idle styling and — per
/// `PLAY-12` — the link surfaces' sensitivity on it.
pub(super) fn panel_presentation_with_external(
    track: Option<&NowPlaying>,
    external: Option<&ExternalPlaybackSnapshot>,
    playback_state: PlaybackState,
) -> PanelPresentation {
    if let Some(external) = external {
        let display = crate::ui::player_bar_state::external_bar_display(external);
        return PanelPresentation {
            title: display.title,
            subtitle: display.subtitle,
            idle: false,
        };
    }
    panel_presentation(track, playback_state)
}
