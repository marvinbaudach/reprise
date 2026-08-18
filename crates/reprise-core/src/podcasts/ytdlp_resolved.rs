//! Full-extraction metadata returned while resolving YouTube audio.

use serde_json::Value;

use super::{audio_unavailable_error, duration_secs, response_error, ResolvedAudio};

pub(super) fn parse(
    operation: &'static str,
    body: &str,
) -> Result<ResolvedAudio, super::PodcastError> {
    let value: Value = serde_json::from_str(body).map_err(|_| response_error(operation))?;
    let stream_url = value
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| audio_unavailable_error(operation))?;
    Ok(ResolvedAudio {
        stream_url: stream_url.to_owned(),
        content_len: content_len(&value, stream_url),
        duration_secs: duration_secs(value.get("duration")),
        categories: categories(value.get("categories")),
        track: optional_text(value.get("track")),
        artist: optional_text(value.get("artist")),
    })
}

fn content_len(value: &Value, stream_url: &str) -> Option<u64> {
    value
        .get("filesize")
        .and_then(Value::as_u64)
        .filter(|length| *length > 0)
        .or_else(|| {
            url::Url::parse(stream_url)
                .ok()?
                .query_pairs()
                .find_map(|(key, value)| {
                    (key == "clen")
                        .then(|| value.parse::<u64>().ok())
                        .flatten()
                        .filter(|length| *length > 0)
                })
        })
}

pub(in crate::podcasts) fn categories(value: Option<&Value>) -> Vec<String> {
    // Keep the strings and skip anything else, rather than discarding the
    // whole array over one odd element: a single non-string would otherwise
    // throw away a perfectly good "Music" sitting next to it.
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn optional_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
