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
        duration_secs: duration_secs(value.get("duration")),
        categories: categories(value.get("categories")),
        track: optional_text(value.get("track")),
        artist: optional_text(value.get("artist")),
    })
}

pub(in crate::podcasts) fn categories(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .map(|value| value.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
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
