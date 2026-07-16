//! Secure ListenBrainz token storage in the system keyring.

pub(in crate::ui) const ATTRIBUTES: [(&str, &str); 2] = [
    ("application", "org.reprise.Reprise"),
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
    let Some(item) = keyring.search_items(&ATTRIBUTES).await?.into_iter().next() else {
        return Ok(None);
    };
    let secret = item.secret().await?;
    String::from_utf8(secret.as_bytes().to_vec())
        .map(Some)
        .map_err(SecretError::from)
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
            [
                ("application", "org.reprise.Reprise"),
                ("service", "listenbrainz")
            ]
        );
        assert!(!ATTRIBUTES.iter().any(|(key, _)| *key == "token"));
    }
}
