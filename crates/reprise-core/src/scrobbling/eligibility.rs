//! Provider-specific rules for deciding whether a completed play may be submitted.

use super::ScrobbleProvider;

const FOUR_MINUTES_MS: i64 = 4 * 60 * 1_000;
const LASTFM_MIN_TRACK_DURATION_MS: i64 = 30 * 1_000;

/// ListenBrainz's documented threshold: half the track or four minutes,
/// whichever comes first.
pub fn should_scrobble(position_ms: i64, duration_ms: i64) -> bool {
    if duration_ms <= 0 {
        return false;
    }
    let half_duration = duration_ms / 2 + duration_ms % 2;
    let threshold = half_duration.min(FOUR_MINUTES_MS);
    position_ms >= threshold
}

/// Applies the provider's published eligibility rules to a completed play.
pub fn should_scrobble_for(provider: ScrobbleProvider, position_ms: i64, duration_ms: i64) -> bool {
    should_scrobble(position_ms, duration_ms)
        && match provider {
            ScrobbleProvider::ListenBrainz => true,
            ScrobbleProvider::LastFm => duration_ms > LASTFM_MIN_TRACK_DURATION_MS,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_track_scrobbles_at_exactly_half_but_not_before() {
        assert!(!should_scrobble(89_999, 180_000));
        assert!(should_scrobble(90_000, 180_000));
    }

    #[test]
    fn long_track_scrobbles_at_four_minutes() {
        assert!(!should_scrobble(239_999, 600_000));
        assert!(should_scrobble(240_000, 600_000));
    }

    #[test]
    fn non_positive_duration_never_scrobbles() {
        assert!(!should_scrobble(1_000, 0));
        assert!(!should_scrobble(1_000, -1));
        assert!(!should_scrobble(0, 1));
        assert!(should_scrobble(1, 1));
    }

    #[test]
    fn lastfm_rejects_tracks_at_or_below_thirty_seconds() {
        assert!(!should_scrobble_for(
            ScrobbleProvider::LastFm,
            15_000,
            30_000
        ));
        assert!(should_scrobble_for(
            ScrobbleProvider::ListenBrainz,
            15_000,
            30_000
        ));
    }
}
