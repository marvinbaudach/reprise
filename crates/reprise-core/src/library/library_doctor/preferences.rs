//! Persistent opt-in state shared by every Library Doctor surface.

use crate::db::Db;
use std::time::Duration;

pub const REMOTE_CONSENT_VERSION: u32 = 1;
const REMOTE_ENABLED_KEY: &str = "library_doctor.remote.enabled";
const REMOTE_CONSENT_VERSION_KEY: &str = "library_doctor.remote.consent_version";
const LOCAL_RATE_KEY: &str = "library_doctor.rate.local_tracks_per_minute";
const REMOTE_RATE_KEY: &str = "library_doctor.rate.remote_tracks_per_minute";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteSuggestionPreference {
    pub enabled: bool,
    pub consent_required: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DoctorScanRates {
    pub local_tracks_per_minute: Option<f64>,
    pub remote_tracks_per_minute: Option<f64>,
}

pub fn scan_rates(db: &Db) -> Result<DoctorScanRates, rusqlite::Error> {
    let conn = db.conn();
    Ok(DoctorScanRates {
        local_tracks_per_minute: stored_rate(conn, LOCAL_RATE_KEY)?,
        remote_tracks_per_minute: stored_rate(conn, REMOTE_RATE_KEY)?,
    })
}

pub fn record_scan_rates(
    db: &Db,
    checked_tracks: usize,
    local_elapsed: Duration,
    remote_elapsed: Option<Duration>,
) -> Result<(), rusqlite::Error> {
    if checked_tracks == 0 {
        return Ok(());
    }
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    if let Some(rate) = measured_rate(checked_tracks, local_elapsed) {
        crate::library::settings::set_setting_in(&transaction, LOCAL_RATE_KEY, &rate.to_string())?;
    }
    if let Some(rate) = remote_elapsed.and_then(|elapsed| measured_rate(checked_tracks, elapsed)) {
        crate::library::settings::set_setting_in(&transaction, REMOTE_RATE_KEY, &rate.to_string())?;
    }
    transaction.commit()
}

fn stored_rate(conn: &rusqlite::Connection, key: &str) -> Result<Option<f64>, rusqlite::Error> {
    Ok(crate::library::settings::get_setting_in(conn, key)?
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|rate| rate.is_finite() && *rate > 0.0))
}

fn measured_rate(checked_tracks: usize, elapsed: Duration) -> Option<f64> {
    let minutes = elapsed.as_secs_f64() / 60.0;
    (minutes > 0.0).then(|| checked_tracks as f64 / minutes)
}

pub fn remote_suggestion_preference(
    db: &Db,
) -> Result<RemoteSuggestionPreference, rusqlite::Error> {
    let conn = db.conn();
    let consented = crate::library::settings::get_setting_in(conn, REMOTE_CONSENT_VERSION_KEY)?
        .and_then(|value| value.parse::<u32>().ok())
        == Some(REMOTE_CONSENT_VERSION);
    let enabled =
        consented && crate::library::settings::get_bool_in(conn, REMOTE_ENABLED_KEY, false)?;
    Ok(RemoteSuggestionPreference {
        enabled,
        consent_required: !consented,
    })
}

pub fn accept_remote_suggestions(db: &Db) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    crate::library::settings::set_setting_in(
        &transaction,
        REMOTE_CONSENT_VERSION_KEY,
        &REMOTE_CONSENT_VERSION.to_string(),
    )?;
    crate::library::settings::set_bool_in(&transaction, REMOTE_ENABLED_KEY, true)?;
    transaction.commit()
}

pub fn disable_remote_suggestions(db: &Db) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::library::settings::set_bool_in(conn, REMOTE_ENABLED_KEY, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn doc_7c_remote_opt_in_is_versioned_persistent_and_independent() {
        let conn = Db::open_in_memory().unwrap();

        assert_eq!(
            remote_suggestion_preference(&conn).unwrap(),
            RemoteSuggestionPreference {
                enabled: false,
                consent_required: true,
            }
        );

        accept_remote_suggestions(&conn).unwrap();
        assert_eq!(
            remote_suggestion_preference(&conn).unwrap(),
            RemoteSuggestionPreference {
                enabled: true,
                consent_required: false,
            }
        );

        disable_remote_suggestions(&conn).unwrap();
        assert_eq!(
            remote_suggestion_preference(&conn).unwrap(),
            RemoteSuggestionPreference {
                enabled: false,
                consent_required: false,
            }
        );

        crate::library::settings::set_setting(
            &conn,
            "library_doctor.remote.consent_version",
            &(REMOTE_CONSENT_VERSION - 1).to_string(),
        )
        .unwrap();
        assert_eq!(
            remote_suggestion_preference(&conn).unwrap(),
            RemoteSuggestionPreference {
                enabled: false,
                consent_required: true,
            }
        );
    }

    #[test]
    fn doc_8d_the_estimate_comes_from_the_last_measured_rate() {
        let conn = Db::open_in_memory().unwrap();

        record_scan_rates(
            &conn,
            120,
            Duration::from_secs(120),
            Some(Duration::from_secs(240)),
        )
        .unwrap();
        record_scan_rates(
            &conn,
            120,
            Duration::from_secs(60),
            Some(Duration::from_secs(120)),
        )
        .unwrap();

        assert_eq!(
            scan_rates(&conn).unwrap(),
            DoctorScanRates {
                local_tracks_per_minute: Some(120.0),
                remote_tracks_per_minute: Some(60.0),
            }
        );
    }
}
