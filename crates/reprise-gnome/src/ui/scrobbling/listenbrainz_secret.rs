//! Secure ListenBrainz token storage in the system keyring.

// The keyring error this module forwards is `oo7::Error`, which is 128 bytes
// wide on its own — the size is not ours to shrink, and the alternative is
// boxing an error type these callers match on directly.
#![allow(clippy::result_large_err)]

pub(in crate::ui) const ATTRIBUTES: [(&str, &str); 2] =
    [("application", crate::APP_ID), ("service", "listenbrainz")];
const LEGACY_ATTRIBUTES: [(&str, &str); 2] = [
    ("application", super::LEGACY_APP_ID),
    ("service", "listenbrainz"),
];

const LABEL: &str = "Reprise ListenBrainz token";

#[derive(Debug, thiserror::Error)]
pub(in crate::ui) enum SecretError {
    #[error("keyring unavailable: {0}")]
    Keyring(#[from] oo7::Error),
    #[error("stored ListenBrainz token is not valid UTF-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

pub(in crate::ui) async fn load() -> Result<Option<String>, SecretError> {
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
    let token = String::from_utf8(secret.as_bytes().to_vec())?;

    if is_legacy {
        if let Err(error) = keyring
            .create_item(LABEL, &ATTRIBUTES, secret.as_bytes(), true)
            .await
        {
            tracing::warn!(%error, service = "listenbrainz", "could not migrate legacy keyring item");
        } else if let Err(error) = item.delete().await {
            tracing::warn!(%error, service = "listenbrainz", "could not delete migrated legacy keyring item");
        }
    }

    Ok(Some(token))
}

pub(in crate::ui) async fn save(token: &str) -> Result<(), SecretError> {
    let keyring = oo7::Keyring::new().await?;
    keyring
        .create_item(LABEL, &ATTRIBUTES, token.as_bytes(), true)
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

    #[test]
    fn lookup_attributes_are_stable_and_contain_no_secret() {
        assert_eq!(
            ATTRIBUTES,
            [("application", crate::APP_ID), ("service", "listenbrainz")]
        );
        assert!(!ATTRIBUTES.iter().any(|(key, _)| *key == "token"));
    }
}
