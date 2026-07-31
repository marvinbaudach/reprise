use super::TimedLine;

pub fn parse_lrc(input: &str) -> Vec<TimedLine> {
    let mut lines = Vec::new();
    for raw_line in input.lines() {
        let mut rest = raw_line.trim_start();
        let mut timestamps = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some(end) = after_open.find(']') else {
                break;
            };
            let tag = &after_open[..end];
            if let Some(timestamp) = parse_timestamp(tag) {
                timestamps.push(timestamp);
            }
            rest = &after_open[end + 1..];
        }
        if timestamps.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for start_ms in timestamps {
            lines.push(TimedLine::new(start_ms, text.clone()));
        }
    }
    lines.sort_by_key(|line| line.start_ms);
    lines
}

pub fn active_line_index(lines: &[TimedLine], position_ms: i64) -> Option<usize> {
    let insertion = lines.partition_point(|line| line.start_ms <= position_ms);
    insertion.checked_sub(1)
}

fn parse_timestamp(tag: &str) -> Option<i64> {
    let (minutes, seconds_fraction) = tag.split_once(':')?;
    let minutes = minutes.parse::<i64>().ok()?;
    let (seconds, fraction) = match seconds_fraction.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (seconds_fraction, None),
    };
    let seconds = seconds.parse::<i64>().ok()?;
    if minutes < 0 || !(0..60).contains(&seconds) {
        return None;
    }
    let fraction_ms = match fraction {
        None => 0,
        Some(value) if !value.is_empty() && value.len() <= 3 => {
            let parsed = value.parse::<i64>().ok()?;
            parsed * 10_i64.pow(u32::try_from(3 - value.len()).ok()?)
        }
        Some(_) => return None,
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds.checked_mul(1_000)?)?
        .checked_add(fraction_ms)
}

#[cfg(test)]
#[path = "lrc_tests.rs"]
mod tests;
