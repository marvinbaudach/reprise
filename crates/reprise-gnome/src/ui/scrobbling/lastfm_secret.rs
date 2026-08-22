//! Secure Last.fm credential storage in the system keyring.

// The keyring error this module forwards is `oo7::Error`, which is 128 bytes
// wide on its own — the size is not ours to shrink, and the alternative is
// boxing an error type these callers match on directly.
#![allow(clippy::result_large_err)]

use std::fmt;

use serde::{Deserialize, Serialize};

pub(in crate::ui) const ATTRIBUTES: [(&str, &str); 2] =
    [("application", crate::APP_ID), ("service", "lastfm")];
const LEGACY_ATTRIBUTES: [(&str, &str); 2] =
    [("application", super::LEGACY_APP_ID), ("service", "lastfm")];

const LABEL: &str = "Reprise Last.fm credentials";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::ui) struct LastFmCredentials {
    pub api_key: String,
    pub shared_secret: String,
    pub session_key: String,
    pub user_name: String,
}

impl fmt::Debug for LastFmCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LastFmCredentials")
            .field("api_key", &"<redacted>")
            .field("shared_secret", &"<redacted>")
            .field("session_key", &"<redacted>")
            .field("user_name", &self.user_name)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub(in crate::ui) enum SecretError {
    #[error("keyring unavailable: {0}")]
    Keyring(#[from] oo7::Error),
    #[error("stored Last.fm credentials are invalid: {0}")]
    Json(#[from] serde_json::Error),
}

pub(in crate::ui) async fn load() -> Result<Option<LastFmCredentials>, SecretError> {
    let keyring = oo7::Keyring::new().await?;
    let (item, is_legacy) =
        if let Some(item) = keyring.search_items(&ATTRIBUTES).await?.into_iter().next() {
            (item, false)
        } else if let Some(item) = keyring
            .search_items(&LEGACY_ATTRIBUTES)
            .await?
            .into_iter()
            .next()
        {
            (item, true)
        } else {
            return Ok(None);
        };
    let secret = item.secret().await?;
    let credentials = serde_json::from_slice(secret.as_bytes())?;

    if is_legacy {
        if let Err(error) = keyring
            .create_item(LABEL, &ATTRIBUTES, secret.as_bytes(), true)
            .await
        {
            tracing::warn!(%error, service = "lastfm", "could not migrate legacy keyring item");
        } else if let Err(error) = item.delete().await {
            tracing::warn!(%error, service = "lastfm", "could not delete migrated legacy keyring item");
        }
    }

    Ok(Some(credentials))
}

pub(in crate::ui) async fn store(credentials: &LastFmCredentials) -> Result<(), SecretError> {
    let bytes = serde_json::to_vec(credentials)?;
    let keyring = oo7::Keyring::new().await?;
    keyring
        .create_item(LABEL, &ATTRIBUTES, &bytes, true)
        .await
        .map_err(SecretError::from)
}

pub(in crate::ui) async fn delete() -> Result<(), SecretError> {
    let keyring = oo7::Keyring::new().await?;
    keyring.delete(&ATTRIBUTES).await.map_err(SecretError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> LastFmCredentials {
        LastFmCredentials {
            api_key: "api-key".to_string(),
            shared_secret: "shared-secret".to_string(),
            session_key: "session-key".to_string(),
            user_name: "listener".to_string(),
        }
    }

    #[test]
    fn attributes_are_stable_and_contain_no_credentials() {
        assert_eq!(
            ATTRIBUTES,
            [("application", crate::APP_ID), ("service", "lastfm")]
        );
        let serialized = format!("{ATTRIBUTES:?}");
        for secret in ["api-key", "shared-secret", "session-key"] {
            assert!(!serialized.contains(secret));
        }
    }

    #[test]
    fn credentials_round_trip_as_json_and_debug_redacts_secrets() {
        let credentials = credentials();
        let json = serde_json::to_vec(&credentials).unwrap();
        assert_eq!(
            serde_json::from_slice::<LastFmCredentials>(&json).unwrap(),
            credentials
        );
        let debug = format!("{credentials:?}");
        assert!(debug.contains("listener"));
        for secret in ["api-key", "shared-secret", "session-key"] {
            assert!(!debug.contains(secret));
        }
    }
}
