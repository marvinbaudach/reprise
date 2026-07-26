#![cfg(feature = "test-fixtures")]

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use reprise_core::concerts::{verify_credential, CredentialVerification, ProviderKind};

const FIXTURE_DIR_ENV: &str = "REPRISE_CONCERTS_FIXTURE_DIR";
const FIXTURE_LOG_ENV: &str = "REPRISE_CONCERTS_FIXTURE_LOG";
const SECRET: &str = "fixture-secret-must-not-leak";

static FIXTURE_ENV: Mutex<()> = Mutex::new(());

struct FixtureEnvironment {
    _lock: MutexGuard<'static, ()>,
    previous_dir: Option<OsString>,
    previous_log: Option<OsString>,
}

impl FixtureEnvironment {
    fn install(directory: &Path, log: &Path) -> Self {
        let lock = FIXTURE_ENV
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous_dir = std::env::var_os(FIXTURE_DIR_ENV);
        let previous_log = std::env::var_os(FIXTURE_LOG_ENV);
        std::env::set_var(FIXTURE_DIR_ENV, directory);
        std::env::set_var(FIXTURE_LOG_ENV, log);
        Self {
            _lock: lock,
            previous_dir,
            previous_log,
        }
    }
}

impl Drop for FixtureEnvironment {
    fn drop(&mut self) {
        if let Some(value) = self.previous_dir.take() {
            std::env::set_var(FIXTURE_DIR_ENV, value);
        } else {
            std::env::remove_var(FIXTURE_DIR_ENV);
        }
        if let Some(value) = self.previous_log.take() {
            std::env::set_var(FIXTURE_LOG_ENV, value);
        } else {
            std::env::remove_var(FIXTURE_LOG_ENV);
        }
    }
}

fn write_status(directory: &Path, route: &str, status: &str) {
    std::fs::write(directory.join(format!("{route}.json.status")), status).unwrap();
}

fn clear_status(directory: &Path, route: &str) {
    let path = directory.join(format!("{route}.json.status"));
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn conc_8_fixture_only_verification_maps_provider_outcomes_without_leaking_the_key() {
    let fixtures = tempfile::tempdir().unwrap();
    let log = fixtures.path().join("requests.log");
    let _environment = FixtureEnvironment::install(fixtures.path(), &log);
    let ticketmaster_route = "ticketmaster-attractions-test";
    let bandsintown_route = "bandsintown-artist-test";

    assert_eq!(
        verify_credential(ProviderKind::Ticketmaster, ""),
        CredentialVerification::Empty
    );

    std::fs::write(
        fixtures.path().join(format!("{ticketmaster_route}.json")),
        "{}",
    )
    .unwrap();
    assert_eq!(
        verify_credential(ProviderKind::Ticketmaster, SECRET),
        CredentialVerification::Valid
    );

    write_status(fixtures.path(), ticketmaster_route, "401");
    assert_eq!(
        verify_credential(ProviderKind::Ticketmaster, SECRET),
        CredentialVerification::Rejected
    );
    write_status(fixtures.path(), ticketmaster_route, "403");
    assert_eq!(
        verify_credential(ProviderKind::Ticketmaster, SECRET),
        CredentialVerification::Rejected
    );
    write_status(fixtures.path(), ticketmaster_route, "timeout");
    assert_eq!(
        verify_credential(ProviderKind::Ticketmaster, SECRET),
        CredentialVerification::CouldNotVerify
    );
    clear_status(fixtures.path(), ticketmaster_route);
    std::fs::remove_file(fixtures.path().join(format!("{ticketmaster_route}.json"))).unwrap();
    assert_eq!(
        verify_credential(ProviderKind::Ticketmaster, SECRET),
        CredentialVerification::CouldNotVerify
    );

    write_status(fixtures.path(), bandsintown_route, "404");
    assert_eq!(
        verify_credential(ProviderKind::Bandsintown, SECRET),
        CredentialVerification::Valid
    );

    let logged = std::fs::read_to_string(log).unwrap();
    assert!(!logged.contains(SECRET));
    assert_eq!(logged.lines().count(), 6);
    assert!(logged
        .lines()
        .all(|line| { line.contains(ticketmaster_route) || line.contains(bandsintown_route) }));
}
