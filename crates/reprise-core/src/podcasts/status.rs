//! Pure episode status derivation.

use super::EpisodeStatus;

#[must_use]
pub fn derive(played_at: Option<i64>, position_ms: i64) -> EpisodeStatus {
    if played_at.is_some() {
        EpisodeStatus::Played
    } else if position_ms > 0 {
        EpisodeStatus::Resume
    } else {
        EpisodeStatus::New
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_1_status_matrix() {
        assert_eq!(derive(None, 0), EpisodeStatus::New);
        assert_eq!(derive(None, 42), EpisodeStatus::Resume);
        assert_eq!(derive(Some(1), 0), EpisodeStatus::Played);
        assert_eq!(derive(Some(1), 42), EpisodeStatus::Played);
    }
}
