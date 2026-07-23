//! Wall-clock helper.
//!
//! The AI-job facades take an injected `now: i64` (unix **seconds**, matching
//! `reprise-core`'s own `scanner::now_unix`) so lease/reclaim timing is testable
//! without sleeps. The CLI is the real caller, so it supplies the real clock
//! here in exactly that unit.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current unix time in whole seconds — the unit every `now`/`lease_secs`
/// argument of the `ai_jobs` facades expects. A pre-epoch clock (only possible
/// with a badly misconfigured system) reads as `0` rather than panicking.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}
