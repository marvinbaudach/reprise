//! Guard for links Reprise hands to the desktop's URI handler.
//!
//! Ticket, event, and announcement URLs all arrive inside third-party JSON,
//! so nothing about them is trustworthy — least of all their scheme. Handing
//! an unchecked URL to the platform launcher would let a provider point the
//! desktop at a local file, a registered helper application, or a
//! `javascript:` payload. Only `http` and `https` ever leave the app; every
//! frontend routes its launches through this one predicate.

use url::Url;

/// Whether `value` may be opened externally.
///
/// A URL passes only when it parses and its scheme is `http` or `https`.
/// Scheme comparison is case-insensitive because the WHATWG parser
/// normalizes it, so `HTTPS://…` is accepted exactly like `https://…`.
#[must_use]
pub fn is_launchable_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

#[cfg(test)]
mod tests {
    use super::is_launchable_url;

    #[test]
    fn web_links_are_launchable_regardless_of_scheme_case() {
        assert!(is_launchable_url("https://tickets.example/offer"));
        assert!(is_launchable_url("http://events.example/event"));
        assert!(is_launchable_url("HTTPS://tickets.example/offer"));
        assert!(is_launchable_url("HtTp://events.example/event"));
    }

    #[test]
    fn every_other_scheme_is_refused() {
        for value in [
            "file:///etc/passwd",
            "FILE:///etc/passwd",
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "mailto:tickets@example.com",
            "reprise://play/1",
            "smb://share/tickets",
        ] {
            assert!(
                !is_launchable_url(value),
                "{value} should not be launchable"
            );
        }
    }

    #[test]
    fn unparsable_values_are_refused() {
        for value in ["", "   ", "tickets.example/offer", "http://", "://example"] {
            assert!(
                !is_launchable_url(value),
                "{value} should not be launchable"
            );
        }
    }
}
