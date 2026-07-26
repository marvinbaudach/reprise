use super::{bandsintown, http, ticketmaster, ProviderError, ProviderKind};

const PROBE_ARTIST: &str = "test";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialVerification {
    Empty,
    Valid,
    Rejected,
    CouldNotVerify,
}

pub fn verify_credential(provider: ProviderKind, credential: &str) -> CredentialVerification {
    let credential = credential.trim();
    if credential.is_empty() {
        return CredentialVerification::Empty;
    }
    let result = match provider {
        ProviderKind::Bandsintown => http::get(&bandsintown::artist_url(PROBE_ARTIST, credential)),
        ProviderKind::Ticketmaster => http::get(&ticketmaster::credential_url(credential)),
    };
    classify_verification(provider, &result)
}

fn classify_verification(
    provider: ProviderKind,
    result: &Result<String, ProviderError>,
) -> CredentialVerification {
    match result {
        Ok(_) | Err(ProviderError::HttpStatus(404)) if provider == ProviderKind::Bandsintown => {
            CredentialVerification::Valid
        }
        Ok(_) => CredentialVerification::Valid,
        Err(ProviderError::HttpStatus(401 | 403)) => CredentialVerification::Rejected,
        Err(_) => CredentialVerification::CouldNotVerify,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conc_8_verification_classification_is_provider_specific_and_pure() {
        assert_eq!(
            classify_verification(ProviderKind::Ticketmaster, &Ok("{}".into())),
            CredentialVerification::Valid
        );
        assert_eq!(
            classify_verification(
                ProviderKind::Ticketmaster,
                &Err(ProviderError::HttpStatus(401))
            ),
            CredentialVerification::Rejected
        );
        assert_eq!(
            classify_verification(
                ProviderKind::Ticketmaster,
                &Err(ProviderError::HttpStatus(403))
            ),
            CredentialVerification::Rejected
        );
        assert_eq!(
            classify_verification(
                ProviderKind::Bandsintown,
                &Err(ProviderError::HttpStatus(404))
            ),
            CredentialVerification::Valid
        );
        assert_eq!(
            classify_verification(ProviderKind::Ticketmaster, &Err(ProviderError::Timeout)),
            CredentialVerification::CouldNotVerify
        );
    }
}
