use crate::ui::playback::external_media::ExternalMedia;
use crate::ui::playback::preview::PlaybackMode;
use crate::ui::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LinkSurface {
    Title,
    Subtitle,
    Cover,
}

#[allow(dead_code)] // Exhaustive contract exercised by PLAY-12 tests.
pub(in crate::ui) const SURFACES: [LinkSurface; 3] = [
    LinkSurface::Title,
    LinkSurface::Subtitle,
    LinkSurface::Cover,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LinkTarget {
    Track,
    Album,
    Artist,
    Episode,
    Channel,
    Station,
}

/// What the player bar knows about the loaded item right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct LinkAvailability {
    pub artist: bool,
    pub album: bool,
}

/// Total over `PlaybackMode` — a new playback source cannot compile without
/// answering all three surfaces. `PLAY-12`.
pub(in crate::ui) fn link_target(mode: PlaybackMode, surface: LinkSurface) -> LinkTarget {
    match (mode, surface) {
        (PlaybackMode::Queue, LinkSurface::Title) => LinkTarget::Track,
        (PlaybackMode::Queue, LinkSurface::Subtitle) => LinkTarget::Artist,
        (PlaybackMode::Queue, LinkSurface::Cover) => LinkTarget::Album,
        (PlaybackMode::QueuedEpisode | PlaybackMode::Podcast, LinkSurface::Title) => {
            LinkTarget::Episode
        }
        (
            PlaybackMode::QueuedEpisode | PlaybackMode::Podcast,
            LinkSurface::Subtitle | LinkSurface::Cover,
        ) => LinkTarget::Channel,
        // `begin_preview` has no productive caller on this base. If preview
        // is wired, revisit this row: a preview file need not be in Library.
        (PlaybackMode::Preview, LinkSurface::Title) => LinkTarget::Track,
        (PlaybackMode::Preview, LinkSurface::Subtitle) => LinkTarget::Artist,
        (PlaybackMode::Preview, LinkSurface::Cover) => LinkTarget::Album,
        (PlaybackMode::Radio, _) => LinkTarget::Station,
    }
}

pub(in crate::ui) fn external_mode(media: &ExternalMedia) -> PlaybackMode {
    match media {
        ExternalMedia::Podcast { .. } => PlaybackMode::Podcast,
        ExternalMedia::Radio { .. } => PlaybackMode::Radio,
    }
}

/// `PLAY-12`'s “never dead”: a surface whose own target does not exist falls
/// back to the nearest target that does.
pub(in crate::ui) fn resolve(target: LinkTarget, available: LinkAvailability) -> LinkTarget {
    match target {
        LinkTarget::Artist if !available.artist => LinkTarget::Track,
        LinkTarget::Album if !available.album => LinkTarget::Track,
        other => other,
    }
}

pub(in crate::ui) fn player_bar_label(target: LinkTarget) -> &'static str {
    match target {
        LinkTarget::Track => strings::JUMP_TO_NOW_PLAYING,
        LinkTarget::Album => strings::REVEAL_PLAYING_ALBUM,
        LinkTarget::Artist => strings::GO_TO_PLAYING_ARTIST,
        LinkTarget::Episode => strings::JUMP_TO_PLAYING_EPISODE,
        LinkTarget::Channel => strings::GO_TO_PLAYING_CHANNEL,
        LinkTarget::Station => strings::JUMP_TO_PLAYING_STATION,
    }
}

pub(in crate::ui) fn panel_label(target: LinkTarget) -> &'static str {
    match target {
        LinkTarget::Track => strings::REVEAL_PLAYING_TRACK,
        LinkTarget::Album => strings::GO_TO_PLAYING_ALBUM,
        LinkTarget::Artist => strings::GO_TO_PLAYING_ARTIST,
        LinkTarget::Episode => strings::JUMP_TO_PLAYING_EPISODE,
        LinkTarget::Channel => strings::GO_TO_PLAYING_CHANNEL,
        LinkTarget::Station => strings::JUMP_TO_PLAYING_STATION,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct LinkLabels {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub cover: &'static str,
}

pub(in crate::ui) fn player_bar_labels(
    mode: PlaybackMode,
    available: LinkAvailability,
) -> LinkLabels {
    labels(mode, available, player_bar_label)
}

pub(in crate::ui) fn panel_labels(mode: PlaybackMode, available: LinkAvailability) -> LinkLabels {
    labels(mode, available, panel_label)
}

/// `PLAY-12`: the labels the player bar's three surfaces carry while nothing
/// is loaded at all. The empty state is queue-shaped — it is what the bar is
/// built with — and naming it here is what keeps a finished podcast or radio
/// session from leaving "Go to playing channel" on a surface whose target is
/// gone. Those surfaces are insensitive in this state; the label is what a
/// screen reader and the tooltip still read.
pub(in crate::ui) fn idle_player_bar_labels() -> LinkLabels {
    player_bar_labels(
        PlaybackMode::Queue,
        LinkAvailability {
            artist: true,
            album: true,
        },
    )
}

fn labels(
    mode: PlaybackMode,
    available: LinkAvailability,
    label: fn(LinkTarget) -> &'static str,
) -> LinkLabels {
    LinkLabels {
        title: label(resolve(link_target(mode, LinkSurface::Title), available)),
        subtitle: label(resolve(link_target(mode, LinkSurface::Subtitle), available)),
        cover: label(resolve(link_target(mode, LinkSurface::Cover), available)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AVAILABILITY: [LinkAvailability; 4] = [
        LinkAvailability {
            artist: false,
            album: false,
        },
        LinkAvailability {
            artist: true,
            album: false,
        },
        LinkAvailability {
            artist: false,
            album: true,
        },
        LinkAvailability {
            artist: true,
            album: true,
        },
    ];

    #[test]
    fn play_12_every_playback_mode_lands_all_three_links() {
        for mode in PlaybackMode::ALL {
            for surface in SURFACES {
                for available in AVAILABILITY {
                    let resolved = resolve(link_target(mode, surface), available);
                    assert!(!crate::ui::strings::text(player_bar_label(resolved)).is_empty());
                    assert!(!crate::ui::strings::text(panel_label(resolved)).is_empty());
                    assert_ne!((resolved, available.artist), (LinkTarget::Artist, false));
                    assert_ne!((resolved, available.album), (LinkTarget::Album, false));
                }
            }
        }
    }

    /// Completeness of `ALL` is guaranteed by its declaration (`enumerated!`
    /// generates it from the variants), not by this test — a test can only
    /// read the list, which is the thing that would be short. What is checked
    /// here is its *order*: the exhaustive `match` names each mode's slot, so
    /// a reordering that would silently repoint the loops above shows up.
    #[test]
    fn play_12_all_lists_every_playback_mode() {
        fn index_of(mode: PlaybackMode) -> usize {
            match mode {
                PlaybackMode::Queue => 0,
                PlaybackMode::QueuedEpisode => 1,
                PlaybackMode::Preview => 2,
                PlaybackMode::Podcast => 3,
                PlaybackMode::Radio => 4,
            }
        }

        for mode in PlaybackMode::ALL {
            assert_eq!(
                PlaybackMode::ALL[index_of(mode)],
                mode,
                "{mode:?} does not stand where the match says it does"
            );
        }
    }

    #[test]
    fn play_12_external_modes_point_at_their_own_source() {
        for mode in [PlaybackMode::QueuedEpisode, PlaybackMode::Podcast] {
            assert_eq!(link_target(mode, LinkSurface::Title), LinkTarget::Episode);
            assert_eq!(
                link_target(mode, LinkSurface::Subtitle),
                LinkTarget::Channel
            );
            assert_eq!(link_target(mode, LinkSurface::Cover), LinkTarget::Channel);
        }
        for surface in SURFACES {
            assert_eq!(
                link_target(PlaybackMode::Radio, surface),
                LinkTarget::Station
            );
        }
    }

    /// `PLAY-12`: when external playback ends and nothing takes over, the bar
    /// falls back to the labels it was built with. Keeping the finished
    /// session's labels is what made an insensitive surface still read "Jump
    /// to the playing station".
    #[test]
    fn play_12_the_empty_state_keeps_no_finished_sessions_labels() {
        let idle = idle_player_bar_labels();
        assert_eq!(
            idle,
            player_bar_labels(
                PlaybackMode::Queue,
                LinkAvailability {
                    artist: true,
                    album: true,
                }
            )
        );

        for mode in [
            PlaybackMode::Radio,
            PlaybackMode::Podcast,
            PlaybackMode::QueuedEpisode,
        ] {
            let external = player_bar_labels(
                mode,
                LinkAvailability {
                    artist: true,
                    album: true,
                },
            );
            assert_ne!(idle.title, external.title, "{mode:?}");
            assert_ne!(idle.subtitle, external.subtitle, "{mode:?}");
            assert_ne!(idle.cover, external.cover, "{mode:?}");
        }
    }

    #[test]
    fn play_12_a_track_without_an_artist_falls_back_to_the_track() {
        assert_eq!(
            resolve(
                LinkTarget::Artist,
                LinkAvailability {
                    artist: false,
                    album: true,
                }
            ),
            LinkTarget::Track
        );
    }

    #[test]
    fn tip_1d_every_surface_names_its_action_in_every_mode() {
        for mode in PlaybackMode::ALL {
            let labels = player_bar_labels(
                mode,
                LinkAvailability {
                    artist: true,
                    album: true,
                },
            );
            for label in [labels.title, labels.subtitle, labels.cover] {
                assert!(!crate::ui::strings::text(label).trim().is_empty());
            }
            if mode == PlaybackMode::Radio {
                assert_eq!(labels.title, labels.subtitle);
            } else {
                assert_ne!(labels.title, labels.subtitle);
            }
        }
    }

    #[test]
    fn browse_4_external_podcast_playback_names_the_episode_and_channel_links() {
        let labels = player_bar_labels(
            PlaybackMode::Podcast,
            LinkAvailability {
                artist: true,
                album: true,
            },
        );

        assert_eq!(labels.title, strings::JUMP_TO_PLAYING_EPISODE);
        assert_eq!(labels.subtitle, strings::GO_TO_PLAYING_CHANNEL);
        assert_eq!(labels.cover, strings::GO_TO_PLAYING_CHANNEL);
    }

    #[test]
    fn browse_4_radio_playback_names_all_three_links_the_station() {
        let labels = player_bar_labels(
            PlaybackMode::Radio,
            LinkAvailability {
                artist: false,
                album: false,
            },
        );

        assert_eq!(labels.title, strings::JUMP_TO_PLAYING_STATION);
        assert_eq!(labels.subtitle, strings::JUMP_TO_PLAYING_STATION);
        assert_eq!(labels.cover, strings::JUMP_TO_PLAYING_STATION);
    }

    #[test]
    fn browse_4_leaving_external_playback_restores_the_library_labels() {
        let labels = player_bar_labels(
            PlaybackMode::Queue,
            LinkAvailability {
                artist: true,
                album: true,
            },
        );

        assert_eq!(labels.title, strings::JUMP_TO_NOW_PLAYING);
        assert_eq!(labels.subtitle, strings::GO_TO_PLAYING_ARTIST);
        assert_eq!(labels.cover, strings::REVEAL_PLAYING_ALBUM);
    }
}
