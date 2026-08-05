use std::cell::Cell;
use std::rc::Rc;

use reprise_core::playback::PlaybackState;

use super::player_controller::NowPlaying;
use super::strings;
use crate::ui::playback::external_media::ExternalPlaybackSnapshot;

pub(super) const UP_NEXT_PAGE: &str = "up-next";
pub(super) const LYRICS_PAGE: &str = "lyrics";
pub(super) const VISUAL_PAGE: &str = "visual";
pub(super) const SOUND_PAGE: &str = "sound";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum PanelTab {
    #[default]
    UpNext,
    Lyrics,
    Visual,
    Sound,
}

pub(super) const PANEL_TABS: [PanelTab; 4] = [
    PanelTab::UpNext,
    PanelTab::Lyrics,
    PanelTab::Visual,
    PanelTab::Sound,
];

impl PanelTab {
    pub(super) fn page_name(self) -> &'static str {
        match self {
            Self::UpNext => UP_NEXT_PAGE,
            Self::Lyrics => LYRICS_PAGE,
            Self::Visual => VISUAL_PAGE,
            Self::Sound => SOUND_PAGE,
        }
    }

    pub(super) fn from_page_name(name: &str) -> Option<Self> {
        PANEL_TABS.into_iter().find(|tab| tab.page_name() == name)
    }
}

pub(super) fn should_render_up_next(panel_visible: bool, selected_tab: PanelTab) -> bool {
    panel_visible && selected_tab == PanelTab::UpNext
}

pub(super) fn tab_after_sound_visibility_change(
    selected: PanelTab,
    sound_visible: bool,
) -> PanelTab {
    if !sound_visible && selected == PanelTab::Sound {
        PanelTab::UpNext
    } else {
        selected
    }
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
    pub(super) sound: String,
}

#[cfg(test)]
mod tests {
    use super::{tab_after_sound_visibility_change, PanelTab, PANEL_TABS, SOUND_PAGE};

    #[test]
    fn npp_14_extension_tab_follows_the_three_built_in_tabs() {
        assert_eq!(
            PANEL_TABS,
            [
                PanelTab::UpNext,
                PanelTab::Lyrics,
                PanelTab::Visual,
                PanelTab::Sound,
            ]
        );
        assert_eq!(PanelTab::Sound.page_name(), SOUND_PAGE);
        assert_eq!(PanelTab::from_page_name(SOUND_PAGE), Some(PanelTab::Sound));
    }

    #[test]
    fn npp_15_hiding_the_open_extension_tab_falls_back_to_up_next() {
        assert_eq!(
            tab_after_sound_visibility_change(PanelTab::Sound, false),
            PanelTab::UpNext
        );
        assert_eq!(
            tab_after_sound_visibility_change(PanelTab::Lyrics, false),
            PanelTab::Lyrics
        );
        assert_eq!(
            tab_after_sound_visibility_change(PanelTab::Sound, true),
            PanelTab::Sound
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
