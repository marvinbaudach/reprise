use super::device_sync_strings::{
    formatted, text, DAYS_AGO, HOURS_AGO, JUST_NOW, MINUTES_AGO, VERIFIED_AGO,
};

pub fn relative_time(
    now: chrono::DateTime<chrono::Utc>,
    then: chrono::DateTime<chrono::Utc>,
) -> String {
    let minutes = now.signed_duration_since(then).num_minutes().max(0);
    if minutes < 1 {
        text(JUST_NOW)
    } else if minutes < 60 {
        formatted(MINUTES_AGO, &[("minutes", &minutes.to_string())])
    } else if minutes < 24 * 60 {
        formatted(HOURS_AGO, &[("hours", &(minutes / 60).to_string())])
    } else {
        formatted(DAYS_AGO, &[("days", &(minutes / (24 * 60)).to_string())])
    }
}

pub fn verified_ago(
    now: chrono::DateTime<chrono::Utc>,
    verified_at: chrono::DateTime<chrono::Utc>,
) -> String {
    let time = relative_time(now, verified_at);
    formatted(VERIFIED_AGO, &[("time", &time)])
}
