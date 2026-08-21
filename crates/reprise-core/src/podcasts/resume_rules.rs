use super::PodcastKind;

pub const MIN_RESUME_DURATION_SECS: i64 = 600;
pub const COMPLETE_TAIL_MS: i64 = 60_000;
pub const COMPLETE_PERCENT: i64 = 97;

/// Whether an episode keeps a resume position. An unknown duration is treated
/// as long so a late duration probe cannot discard a genuinely long episode.
#[must_use]
pub fn keeps_resume(kind: PodcastKind, duration_secs: Option<i64>) -> bool {
    kind == PodcastKind::Rss
        && duration_secs.is_none_or(|duration| duration >= MIN_RESUME_DURATION_SECS)
}

/// Whether a live playback position is close enough to the known end to count
/// as complete.
#[must_use]
pub fn is_complete(position_ms: i64, duration_secs: Option<i64>) -> bool {
    let Some(duration_secs) = duration_secs.filter(|duration| *duration > 0) else {
        return false;
    };
    if position_ms < 0 {
        return false;
    }

    let position_ms = i128::from(position_ms);
    let duration_ms = i128::from(duration_secs) * 1_000;
    let tail_complete = duration_ms > i128::from(COMPLETE_TAIL_MS)
        && duration_ms - position_ms < i128::from(COMPLETE_TAIL_MS);
    let percent_complete = position_ms * 100 >= duration_ms * i128::from(COMPLETE_PERCENT);
    tail_complete || percent_complete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_1_rss_keeps_resume_only_from_ten_minutes_or_with_unknown_duration() {
        assert!(keeps_resume(PodcastKind::Rss, Some(600)));
        assert!(!keeps_resume(PodcastKind::Rss, Some(599)));
        assert!(keeps_resume(PodcastKind::Rss, None));
    }

    #[test]
    fn pod_1_youtube_never_keeps_resume_even_when_long_or_unknown() {
        assert!(!keeps_resume(PodcastKind::Youtube, Some(3_600)));
        assert!(!keeps_resume(PodcastKind::Youtube, None));
    }

    #[test]
    fn pod_1_completion_tail_is_strictly_less_than_sixty_seconds() {
        assert!(!is_complete(940_000, Some(1_000)));
        assert!(is_complete(940_001, Some(1_000)));
        assert!(!is_complete(939_000, Some(1_000)));
    }

    #[test]
    fn pod_1_completion_percentage_includes_exactly_ninety_seven_percent() {
        assert!(is_complete(3_492_000, Some(3_600)));
        assert!(!is_complete(3_488_400, Some(3_600)));
    }

    #[test]
    fn pod_1_a_sub_minute_episode_uses_percentage_not_the_tail_rule() {
        assert!(!is_complete(1_000, Some(30)));
        assert!(is_complete(29_100, Some(30)));
    }

    #[test]
    fn pod_1_completion_requires_a_known_positive_duration_and_position() {
        assert!(!is_complete(10_000, None));
        assert!(!is_complete(-1, Some(600)));
        assert!(!is_complete(0, Some(0)));
        assert!(is_complete(600_001, Some(600)));
    }
}
