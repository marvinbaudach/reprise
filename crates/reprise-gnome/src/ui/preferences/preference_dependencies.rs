//! Shared helpers for plugin activation state in the preferences UI.

pub(super) fn service_subtitle(description: &str, enabled: bool, status: &str) -> String {
    if enabled {
        format!("{description} · {status}")
    } else {
        description.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_status_is_only_shown_while_enabled() {
        assert_eq!(
            service_subtitle("Scrobble listens", false, "Connected as Ada"),
            "Scrobble listens"
        );
        assert_eq!(
            service_subtitle("Scrobble listens", true, "Connected as Ada"),
            "Scrobble listens · Connected as Ada"
        );
    }
}
