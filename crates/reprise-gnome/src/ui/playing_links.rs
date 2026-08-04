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

#[allow(dead_code)] // Delivered to the Now Playing panel in AP8.
pub(in crate::ui) fn panel_labels(mode: PlaybackMode, available: LinkAvailability) -> LinkLabels {
    labels(mode, available, panel_label)
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

        assert_eq!(PlaybackMode::ALL.len(), 5);
        for mode in PlaybackMode::ALL {
            assert_eq!(PlaybackMode::ALL[index_of(mode)], mode);
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
}
