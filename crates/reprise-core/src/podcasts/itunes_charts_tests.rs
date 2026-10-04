use std::collections::VecDeque;

use super::*;
use crate::log_capture::CapturedLogs;
use crate::podcasts::http::Response;
use crate::podcasts::itunes::SearchResult;

const FIRST_ID: &str = "1535809341";
const SECOND_ID: &str = "1200361736";

struct ScriptedFetch {
    responses: VecDeque<Result<Response, PodcastError>>,
    requests: Vec<String>,
}

impl ScriptedFetch {
    fn new(responses: impl IntoIterator<Item = Result<Response, PodcastError>>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
            requests: Vec::new(),
        }
    }

    fn fetch(&mut self, url: &str) -> Result<Response, PodcastError> {
        self.requests.push(url.to_owned());
        self.responses
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected request beyond script: {url}"))
    }
}

fn response(body: &str) -> Result<Response, PodcastError> {
    Ok(Response {
        body: body.to_owned(),
        etag: None,
        last_modified: None,
    })
}

fn chart_response() -> Result<Response, PodcastError> {
    response(&format!(
        r#"{{"feed":{{"results":[{{"id":"{FIRST_ID}"}},{{"id":"{SECOND_ID}"}}]}}}}"#
    ))
}

fn empty_chart_response() -> Result<Response, PodcastError> {
    response(r#"{"feed":{"results":[]}}"#)
}

fn lookup_response() -> Result<Response, PodcastError> {
    response(&format!(
        r#"{{"results":[
            {{"collectionId":{SECOND_ID},"collectionName":"Second show","feedUrl":"https://feeds.example/second"}},
            {{"collectionId":{FIRST_ID},"collectionName":"First show","feedUrl":"https://feeds.example/first"}}
        ]}}"#
    ))
}

fn request_steps(requests: &[String]) -> Vec<&'static str> {
    requests
        .iter()
        .map(|url| {
            if url.starts_with(CHART_ENDPOINT) {
                "chart"
            } else if url.starts_with(LOOKUP_ENDPOINT) {
                "lookup"
            } else {
                panic!("unexpected endpoint: {url}");
            }
        })
        .collect()
}

fn assert_chart_order(rows: &[SearchResult]) {
    assert_eq!(
        rows.iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        ["First show", "Second show"]
    );
}

fn chart_log_lines(logs: &CapturedLogs) -> Vec<String> {
    logs.joined()
        .lines()
        .filter(|line| line.contains("podcast chart request failed"))
        .map(str::to_owned)
        .collect()
}

fn result(title: &str, feed_url: &str) -> SearchResult {
    SearchResult {
        title: title.to_owned(),
        author: None,
        feed_url: feed_url.to_owned(),
        episode_count: None,
        image_url: None,
        last_episode: None,
    }
}

#[test]
fn src_19_the_chart_flow_asks_the_chart_then_one_lookup() {
    let mut fake = ScriptedFetch::new([chart_response(), lookup_response()]);

    let rows = top_podcasts_with("CH", &mut |url| fake.fetch(url)).unwrap();

    assert_eq!(
        fake.requests,
        [
            chart_url("CH"),
            lookup_url(&[FIRST_ID.to_owned(), SECOND_ID.to_owned()])
        ]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        ["First show", "Second show"]
    );
}

#[test]
fn src_19_an_empty_chart_skips_the_lookup() {
    let mut fake = ScriptedFetch::new([empty_chart_response()]);

    let rows = top_podcasts_with("CH", &mut |url| fake.fetch(url)).unwrap();

    assert!(rows.is_empty());
    assert_eq!(fake.requests, [chart_url("CH")]);
}

#[test]
fn src_19a_a_chart_request_that_times_out_is_asked_once_more() {
    let mut fake = ScriptedFetch::new([
        Err(PodcastError::Timeout),
        chart_response(),
        lookup_response(),
    ]);

    let rows = top_podcasts_with("CH", &mut |url| fake.fetch(url)).unwrap();

    assert_chart_order(&rows);
    assert_eq!(request_steps(&fake.requests), ["chart", "chart", "lookup"]);
}

#[test]
fn src_19a_a_server_error_on_the_chart_is_asked_once_more() {
    let mut fake = ScriptedFetch::new([
        Err(PodcastError::HttpStatus(502)),
        chart_response(),
        lookup_response(),
    ]);

    let rows = top_podcasts_with("CH", &mut |url| fake.fetch(url)).unwrap();

    assert_chart_order(&rows);
    assert_eq!(request_steps(&fake.requests), ["chart", "chart", "lookup"]);
}

#[test]
fn src_19a_a_failed_lookup_is_retried_without_refetching_the_chart() {
    let mut fake = ScriptedFetch::new([
        chart_response(),
        Err(PodcastError::Timeout),
        lookup_response(),
    ]);

    let rows = top_podcasts_with("CH", &mut |url| fake.fetch(url)).unwrap();

    assert_chart_order(&rows);
    assert_eq!(request_steps(&fake.requests), ["chart", "lookup", "lookup"]);
}

#[test]
fn src_19a_the_second_failure_ends_the_step() {
    let mut fake = ScriptedFetch::new([
        Err(PodcastError::Timeout),
        Err(PodcastError::HttpStatus(503)),
    ]);

    let error = top_podcasts_with("CH", &mut |url| fake.fetch(url)).unwrap_err();

    assert!(matches!(error, PodcastError::HttpStatus(503)));
    assert_eq!(request_steps(&fake.requests), ["chart", "chart"]);
}

#[test]
fn src_19a_a_failure_that_will_not_change_is_not_asked_again() {
    for error in [
        PodcastError::RateLimited { retry_after: None },
        PodcastError::SourceGone(404),
        PodcastError::HttpStatus(403),
        PodcastError::Transport("offline".to_owned()),
    ] {
        let expected_kind = std::mem::discriminant(&error);
        let mut fake = ScriptedFetch::new([Err(error)]);

        let actual = top_podcasts_with("CH", &mut |url| fake.fetch(url)).unwrap_err();

        assert_eq!(std::mem::discriminant(&actual), expected_kind);
        assert_eq!(request_steps(&fake.requests), ["chart"]);
    }
}

#[test]
fn src_19a_every_failed_request_leaves_one_log_line() {
    let logs = CapturedLogs::default();
    let mut fake = ScriptedFetch::new([
        Err(PodcastError::Timeout),
        Err(PodcastError::HttpStatus(502)),
    ]);

    let error = logs
        .capture(|| top_podcasts_with("CH", &mut |url| fake.fetch(url)))
        .unwrap_err();

    assert!(matches!(error, PodcastError::HttpStatus(502)));
    let lines = chart_log_lines(&logs);
    assert_eq!(lines.len(), 2, "captured logs: {}", logs.joined());
    assert!(lines[0].contains("step=\"chart\""));
    assert!(lines[0].contains("storefront=\"ch\""));
    assert!(lines[0].contains("attempt=1"));
    assert!(lines[0].contains("retrying=true"));
    assert!(lines[0].contains("reason=\"podcast source timed out\""));
    assert!(!lines[0].contains("status="));
    assert!(lines[1].contains("step=\"chart\""));
    assert!(lines[1].contains("storefront=\"ch\""));
    assert!(lines[1].contains("attempt=2"));
    assert!(lines[1].contains("retrying=false"));
    assert!(lines[1].contains("status=502"));
    assert!(lines[1].contains("reason=\"podcast source returned an HTTP error\""));
}

#[test]
fn src_19a_a_recovered_flow_logs_only_its_failed_attempt() {
    let logs = CapturedLogs::default();
    let mut fake = ScriptedFetch::new([
        Err(PodcastError::Timeout),
        chart_response(),
        lookup_response(),
    ]);

    let rows = logs
        .capture(|| top_podcasts_with("CH", &mut |url| fake.fetch(url)))
        .unwrap();

    assert_chart_order(&rows);
    assert_eq!(chart_log_lines(&logs).len(), 1);
}

#[test]
fn src_19a_a_first_time_success_logs_nothing() {
    let logs = CapturedLogs::default();
    let mut fake = ScriptedFetch::new([chart_response(), lookup_response()]);

    let rows = logs
        .capture(|| top_podcasts_with("CH", &mut |url| fake.fetch(url)))
        .unwrap();

    assert_chart_order(&rows);
    assert!(chart_log_lines(&logs).is_empty());
}

#[test]
fn src_19a_an_unreadable_answer_is_logged_and_not_asked_again() {
    let logs = CapturedLogs::default();
    let mut fake = ScriptedFetch::new([response("not json")]);

    let error = logs
        .capture(|| top_podcasts_with("CH", &mut |url| fake.fetch(url)))
        .unwrap_err();

    assert!(matches!(error, PodcastError::Parse(_)));
    assert_eq!(request_steps(&fake.requests), ["chart"]);
    let lines = chart_log_lines(&logs);
    assert_eq!(lines.len(), 1, "captured logs: {}", logs.joined());
    assert!(lines[0].contains("reason=\"podcast source returned invalid data\""));
}

#[test]
fn src_19a_the_log_line_never_carries_the_url_or_provider_text() {
    let logs = CapturedLogs::default();
    let provider_text = format!(
        "provider failed at https://itunes.apple.com/lookup?id={FIRST_ID},{SECOND_ID}&entity=podcast"
    );
    let mut fake = ScriptedFetch::new([
        chart_response(),
        Err(PodcastError::Transport(provider_text)),
    ]);

    let error = logs
        .capture(|| top_podcasts_with("CH", &mut |url| fake.fetch(url)))
        .unwrap_err();

    assert!(matches!(error, PodcastError::Transport(_)));
    let logged = logs.joined();
    assert!(logged.contains("reason=\"podcast source could not be reached\""));
    for forbidden in ["://", "apple.com", "lookup?", FIRST_ID, SECOND_ID] {
        assert!(
            !logged.contains(forbidden),
            "chart log leaked {forbidden:?}: {logged}"
        );
    }
}

#[test]
fn src_19_the_chart_request_uses_the_lowercase_storefront_code() {
    assert_eq!(
        chart_url("DE"),
        "https://rss.marketingtools.apple.com/api/v2/de/podcasts/top/12/podcasts.json"
    );
}

#[test]
fn src_19_the_lookup_batches_every_charted_id_into_one_request() {
    let ids = (1..=12).map(|id| id.to_string()).collect::<Vec<_>>();
    let url = lookup_url(&ids);

    assert!(url.contains("id=1,2,3,4,5,6,7,8,9,10,11,12"));
    assert!(url.contains("entity=podcast"));
    assert_eq!(url.matches("id=").count(), 1);
}

#[test]
fn src_19_chart_ids_are_read_in_chart_order() {
    let ids =
        parse_chart_ids(r#"{"feed":{"results":[{"id":"42"},{"id":"7"},{"id":"99"}]}}"#).unwrap();

    assert_eq!(ids, ["42", "7", "99"]);
}

#[test]
fn src_19_the_lookup_answer_is_restored_to_chart_order() {
    let rows = vec![
        (Some(7), result("Seven", "https://e.test/7")),
        (Some(42), result("Forty-two", "https://e.test/42")),
        (Some(99), result("Ninety-nine", "https://e.test/99")),
    ];

    let ordered = in_chart_order(&["42".into(), "7".into(), "99".into()], rows);

    assert_eq!(
        ordered
            .iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        ["Forty-two", "Seven", "Ninety-nine"]
    );
}

#[test]
fn src_19_an_id_the_lookup_drops_falls_out_rather_than_leaving_a_hole() {
    let ids = (1..=12).map(|id| id.to_string()).collect::<Vec<_>>();
    let rows = (1..=12)
        .filter(|id| *id != 6)
        .rev()
        .map(|id| {
            (
                Some(id),
                result(&format!("Show {id}"), &format!("https://e.test/{id}")),
            )
        })
        .collect();

    let ordered = in_chart_order(&ids, rows);

    assert_eq!(ordered.len(), 11);
    assert_eq!(
        ordered
            .iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        [
            "Show 1", "Show 2", "Show 3", "Show 4", "Show 5", "Show 7", "Show 8", "Show 9",
            "Show 10", "Show 11", "Show 12"
        ]
    );
}

/// `SRC-19`: the ids come from Apple's chart feed, and the only thing the
/// lookup can do with them is `i64`. Rejecting an unusable one *after* the
/// request has gone out — which is where `in_chart_order`'s parse sits —
/// would mean asking on behalf of a value we already know we cannot use,
/// so the boundary parser drops it instead.
#[test]
fn src_19_a_chart_id_the_lookup_cannot_use_never_reaches_the_request() {
    let ids = parse_chart_ids(
        r#"{"feed":{"results":[
            {"id":"42"},{"id":"not-an-id"},{"id":"7x"},{"id":""},
            {"id":"1,2"},{"id":" 7"},{"id":"7"}
        ]}}"#,
    )
    .unwrap();

    assert_eq!(ids, ["42", "7"]);
    assert_eq!(
        lookup_url(&ids),
        format!("{LOOKUP_ENDPOINT}?id=42,7&entity=podcast")
    );
}

#[test]
fn malformed_chart_body_is_a_parse_error() {
    let error = parse_chart_ids(r#"{"feed":{"results":not-json}}"#).unwrap_err();
    assert!(matches!(error, crate::podcasts::PodcastError::Parse(_)));
}
