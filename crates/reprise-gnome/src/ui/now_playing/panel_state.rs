use std::cell::Cell;
use std::rc::Rc;

use reprise_core::playback::PlaybackState;

use super::player_controller::NowPlaying;
use super::strings;

pub(super) const UP_NEXT_PAGE: &str = "up-next";
pub(super) const LYRICS_PAGE: &str = "lyrics";
pub(super) const AUDIO_CHARACTER_PAGE: &str = "audio-character";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum PanelTab {
    #[default]
    UpNext,
    Lyrics,
    AudioCharacter,
}

pub(super) const PANEL_TABS: [PanelTab; 3] =
    [PanelTab::UpNext, PanelTab::Lyrics, PanelTab::AudioCharacter];

impl PanelTab {
    pub(super) fn page_name(self) -> &'static str {
        match self {
            Self::UpNext => UP_NEXT_PAGE,
            Self::Lyrics => LYRICS_PAGE,
            Self::AudioCharacter => AUDIO_CHARACTER_PAGE,
        }
    }

    pub(super) fn from_page_name(name: &str) -> Option<Self> {
        PANEL_TABS.into_iter().find(|tab| tab.page_name() == name)
    }
}

pub(super) fn should_render_up_next(panel_visible: bool, selected_tab: PanelTab) -> bool {
    panel_visible && selected_tab == PanelTab::UpNext
}

#[derive(Default)]
pub(super) struct TabSession {
    pub(super) selected: Cell<PanelTab>,
}

#[derive(Default)]
pub(super) struct TabFooters {
    pub(super) up_next: String,
    pub(super) lyrics: String,
    pub(super) audio_character: String,
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
