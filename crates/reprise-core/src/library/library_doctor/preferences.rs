//! Persistent opt-in state shared by every Library Doctor surface.

use rusqlite::Connection;

pub const REMOTE_CONSENT_VERSION: u32 = 1;
const REMOTE_ENABLED_KEY: &str = "library_doctor.remote.enabled";
const REMOTE_CONSENT_VERSION_KEY: &str = "library_doctor.remote.consent_version";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteSuggestionPreference {
    pub enabled: bool,
    pub consent_required: bool,
}

pub fn remote_suggestion_preference(
    conn: &Connection,
) -> Result<RemoteSuggestionPreference, rusqlite::Error> {
    let consented = crate::library::settings::get_setting(conn, REMOTE_CONSENT_VERSION_KEY)?
        .and_then(|value| value.parse::<u32>().ok())
        == Some(REMOTE_CONSENT_VERSION);
    let enabled = consented && crate::library::settings::get_bool(conn, REMOTE_ENABLED_KEY, false)?;
    Ok(RemoteSuggestionPreference {
        enabled,
        consent_required: !consented,
    })
}

pub fn accept_remote_suggestions(conn: &Connection) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    crate::library::settings::set_setting(
        &transaction,
        REMOTE_CONSENT_VERSION_KEY,
        &REMOTE_CONSENT_VERSION.to_string(),
    )?;
    crate::library::settings::set_bool(&transaction, REMOTE_ENABLED_KEY, true)?;
    transaction.commit()
}

pub fn disable_remote_suggestions(conn: &Connection) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_bool(conn, REMOTE_ENABLED_KEY, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_7a_remote_opt_in_is_versioned_persistent_and_independent() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

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
