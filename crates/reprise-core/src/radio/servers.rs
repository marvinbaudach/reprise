//! radio-browser server discovery and rotation.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};

use serde::Deserialize;

use super::RadioError;

const DISCOVERY_URL: &str = "https://all.api.radio-browser.info/json/servers";
const MAX_ATTEMPTS: usize = 3;

static CACHED_SERVERS: Mutex<Option<ServerPool>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerPool {
    servers: Vec<String>,
    start: usize,
}

impl ServerPool {
    #[must_use]
    pub fn new(servers: Vec<String>, start: usize) -> Self {
        let start = if servers.is_empty() {
            0
        } else {
            start % servers.len()
        };
        Self { servers, start }
    }

    #[must_use]
    pub fn attempts(&self) -> Vec<&str> {
        let count = self.servers.len().min(MAX_ATTEMPTS);
        (0..count)
            .map(|offset| self.servers[(self.start + offset) % self.servers.len()].as_str())
            .collect()
    }
}

#[derive(Deserialize)]
struct ServerDocument {
    #[serde(default)]
    name: String,
}

pub fn parse_servers(json: &str) -> Result<Vec<String>, RadioError> {
    let documents: Vec<ServerDocument> =
        serde_json::from_str(json).map_err(|error| RadioError::Parse(error.to_string()))?;
    let mut seen = HashSet::new();
    Ok(documents
        .into_iter()
        .filter_map(|document| normalize_server(&document.name))
        .filter(|server| seen.insert(server.clone()))
        .collect())
}

pub fn discover() -> Result<ServerPool, RadioError> {
    if let Some(cached) = lock_unpoisoned(&CACHED_SERVERS).clone() {
        return Ok(cached);
    }
    let servers = parse_servers(&super::http::get(DISCOVERY_URL)?)?;
    if servers.is_empty() {
        return Err(RadioError::Unavailable(
            "radio-browser returned no available servers".into(),
        ));
    }
    let pool = ServerPool::new(servers.clone(), fastrand::usize(..servers.len()));
    *lock_unpoisoned(&CACHED_SERVERS) = Some(pool.clone());
    Ok(pool)
}

pub fn try_servers<T>(
    mut operation: impl FnMut(&str) -> Result<T, RadioError>,
) -> Result<T, RadioError> {
    let pool = discover()?;
    try_pool(&pool, &mut operation)
}

pub fn try_pool<T>(
    pool: &ServerPool,
    operation: &mut impl FnMut(&str) -> Result<T, RadioError>,
) -> Result<T, RadioError> {
    let mut last_error = None;
    for server in pool.attempts() {
        match operation(server) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| RadioError::Unavailable("radio-browser has no reachable server".into())))
}

#[cfg(test)]
pub(crate) fn reset_cache_for_tests() {
    *lock_unpoisoned(&CACHED_SERVERS) = None;
}

fn normalize_server(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return None;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_owned())
    } else {
        Some(format!("https://{value}"))
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_parser_deduplicates_and_normalizes_https_endpoints() {
        let servers = parse_servers(
            r#"[
                {"name":"de1.api.radio-browser.info","ip":"1.2.3.4"},
                {"name":"https://de1.api.radio-browser.info/"},
                {"name":"nl1.api.radio-browser.info"},
                {"name":""}
            ]"#,
        )
        .unwrap();

        assert_eq!(
            servers,
            vec![
                "https://de1.api.radio-browser.info",
                "https://nl1.api.radio-browser.info"
            ]
        );
    }

    #[test]
    fn rad_3_rotation_tries_at_most_three_servers_from_selected_start() {
        let pool = ServerPool::new(
            vec!["one".into(), "two".into(), "three".into(), "four".into()],
            2,
        );

        assert_eq!(pool.attempts(), vec!["three", "four", "one"]);
    }

    #[test]
    fn rotation_continues_after_failure_and_returns_the_first_success() {
        let pool = ServerPool::new(vec!["one".into(), "two".into(), "three".into()], 0);
        let mut visited = Vec::new();

        let result = try_pool(&pool, &mut |server| {
            visited.push(server.to_owned());
            if server == "two" {
                Ok("fresh")
            } else {
                Err(RadioError::Timeout)
            }
        })
        .unwrap();

        assert_eq!(result, "fresh");
        assert_eq!(visited, vec!["one", "two"]);
    }
}
