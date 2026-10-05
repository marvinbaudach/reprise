use super::*;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);
/// ureq follows up to ten redirects unless a policy says otherwise.
const DEFAULT_MAX_REDIRECTS: u32 = 10;

#[test]
fn user_agent_identifies_version_and_maintainer() {
    let value = user_agent();
    assert!(value.contains(env!("CARGO_PKG_VERSION")));
    assert!(value.contains(CONTACT_URL));
    assert!(value.contains("https://github.com/marvinbaudach"));
}

#[test]
fn source_policy_turns_status_errors_off_and_leaves_the_rest_default() {
    assert_eq!(
        AgentPolicy::source(TIMEOUT),
        AgentPolicy {
            timeout: TIMEOUT,
            status_as_error: false,
            https_only: false,
            max_redirects: None,
            proxy_from_env: true,
        }
    );
}

#[test]
fn strict_policy_turns_status_errors_on() {
    assert_eq!(
        AgentPolicy::strict(TIMEOUT),
        AgentPolicy {
            timeout: TIMEOUT,
            status_as_error: true,
            https_only: false,
            max_redirects: None,
            proxy_from_env: true,
        }
    );
}

#[test]
fn build_config_applies_exactly_the_policy() {
    let source = build_config(AgentPolicy::source(TIMEOUT));
    assert_eq!(source.timeouts().global, Some(TIMEOUT));
    assert!(!source.http_status_as_error());
    assert!(!source.https_only());
    assert_eq!(source.max_redirects(), DEFAULT_MAX_REDIRECTS);
    assert!(format!("{:?}", source.user_agent()).contains(&user_agent()));

    let strict_https = build_config(AgentPolicy {
        timeout: Duration::from_secs(15),
        status_as_error: true,
        https_only: true,
        max_redirects: None,
        proxy_from_env: true,
    });
    assert_eq!(
        strict_https.timeouts().global,
        Some(Duration::from_secs(15))
    );
    assert!(strict_https.http_status_as_error());
    assert!(strict_https.https_only());
    assert_eq!(strict_https.max_redirects(), DEFAULT_MAX_REDIRECTS);

    let locked_down = build_config(AgentPolicy {
        timeout: Duration::from_secs(15),
        status_as_error: false,
        https_only: false,
        max_redirects: Some(0),
        proxy_from_env: false,
    });
    assert_eq!(locked_down.timeouts().global, Some(Duration::from_secs(15)));
    assert!(!locked_down.http_status_as_error());
    assert_eq!(locked_down.max_redirects(), 0);
    assert!(locked_down.proxy().is_none());
    assert!(format!("{:?}", locked_down.user_agent()).contains(&user_agent()));
}
