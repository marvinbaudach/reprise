//! Persistent opt-in state shared by every Library Doctor surface.

use crate::db::Db;

pub const REMOTE_CONSENT_VERSION: u32 = 1;
const REMOTE_ENABLED_KEY: &str = "library_doctor.remote.enabled";
const REMOTE_CONSENT_VERSION_KEY: &str = "library_doctor.remote.consent_version";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteSuggestionPreference {
    pub enabled: bool,
    pub consent_required: bool,
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

    #[test]
    fn doc_7a_remote_opt_in_is_versioned_persistent_and_independent() {
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
}
