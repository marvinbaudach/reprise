//! Bounded, silent history of coarse track-list events.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Instant;

const CAPACITY: usize = 64;
const PAYLOAD_LIMIT: usize = 120;
static PROCESS_START: OnceLock<Instant> = OnceLock::new();

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    QuerySet {
        total: u32,
        source: String,
        sort_field: String,
        sort_dir: String,
        filter_len: usize,
        exclude_ai: bool,
    },
    ItemsChanged {
        position: u32,
        removed: u32,
        added: u32,
    },
    SectionsChanged {
        position: u32,
        n_items: u32,
    },
    Reload {
        source: String,
        count: usize,
    },
    StackPage {
        page: String,
    },
    Reveal {
        track_id: i64,
        position: Option<u32>,
        change: String,
        outcome: String,
    },
    PlaybackState {
        state: String,
    },
    WindowQueryError {
        position: u32,
        window_start: u32,
        error: String,
    },
    ScrollJump {
        from: f64,
        to: f64,
        upper: f64,
        page: f64,
    },
    RowLoss {
        n_items: u32,
    },
    RowLossRecovered {
        after_ms: u64,
        rows: usize,
    },
    SelfHeal {
        recovery: String,
    },
}

#[derive(Debug)]
struct Entry {
    elapsed_ms: u128,
    event: Event,
}

#[derive(Debug, Default)]
pub(crate) struct DiagnosticTrail {
    entries: RefCell<VecDeque<Entry>>,
}

impl DiagnosticTrail {
    pub(crate) fn record(&self, event: Event) {
        let start = *PROCESS_START.get_or_init(Instant::now);
        self.push(start.elapsed().as_millis(), event);
    }

    pub(crate) fn snapshot(&self) -> Vec<String> {
        self.entries.borrow().iter().map(Entry::render).collect()
    }

    fn push(&self, elapsed_ms: u128, event: Event) {
        let mut entries = self.entries.borrow_mut();
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back(Entry { elapsed_ms, event });
    }
}

impl Entry {
    fn render(&self) -> String {
        let (category, payload) = self.event.render();
        format!(
            "{}ms {category} {}",
            self.elapsed_ms,
            truncate_payload(&payload)
        )
    }
}

impl Event {
    fn render(&self) -> (&'static str, String) {
        match self {
            Self::QuerySet {
                total,
                source,
                sort_field,
                sort_dir,
                filter_len,
                exclude_ai,
            } => (
                "QuerySet",
                format!(
                    "total={total} source={source} sort_field={sort_field} sort_dir={sort_dir} filter_len={filter_len} exclude_ai={exclude_ai}"
                ),
            ),
            Self::ItemsChanged {
                position,
                removed,
                added,
            } => (
                "ItemsChanged",
                format!("position={position} removed={removed} added={added}"),
            ),
            Self::SectionsChanged { position, n_items } => (
                "SectionsChanged",
                format!("position={position} n_items={n_items}"),
            ),
            Self::Reload { source, count } => {
                ("Reload", format!("source={source} count={count}"))
            }
            Self::StackPage { page } => ("StackPage", format!("page={page}")),
            Self::Reveal {
                track_id,
                position,
                change,
                outcome,
            } => (
                "Reveal",
                format!(
                    "track_id={track_id} position={} change={change} outcome={outcome}",
                    position.map_or_else(|| "none".into(), |value| value.to_string())
                ),
            ),
            Self::PlaybackState { state } => ("PlaybackState", format!("state={state}")),
            Self::WindowQueryError {
                position,
                window_start,
                error,
            } => (
                "WindowQueryError",
                format!("position={position} window_start={window_start} error={error}"),
            ),
            Self::ScrollJump {
                from,
                to,
                upper,
                page,
            } => (
                "ScrollJump",
                format!("from={from:.2} to={to:.2} upper={upper:.2} page={page:.2}"),
            ),
            Self::RowLoss { n_items } => ("RowLoss", format!("n_items={n_items}")),
            Self::RowLossRecovered { after_ms, rows } => (
                "RowLossRecovered",
                format!("after_ms={after_ms} rows={rows}"),
            ),
            Self::SelfHeal { recovery } => ("SelfHeal", format!("recovery={recovery}")),
        }
    }
}

fn truncate_payload(payload: &str) -> String {
    let payload = payload.replace('\n', "\\n").replace('\r', "\\r");
    if payload.chars().count() <= PAYLOAD_LIMIT {
        return payload;
    }
    payload
        .chars()
        .take(PAYLOAD_LIMIT - 1)
        .chain(std::iter::once('…'))
        .collect()
}

thread_local! {
    static THREAD_TRAIL: Rc<DiagnosticTrail> = Rc::new(DiagnosticTrail::default());
}

pub(crate) fn mark_process_start() {
    PROCESS_START.get_or_init(Instant::now);
}

pub(crate) fn handle() -> Rc<DiagnosticTrail> {
    THREAD_TRAIL.with(Rc::clone)
}

pub(crate) fn record(event: Event) {
    THREAD_TRAIL.with(|trail| trail.record(event));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trail_keeps_the_newest_64_entries_in_oldest_first_order() {
        let trail = DiagnosticTrail::default();
        for count in 0..70 {
            trail.push(
                count,
                Event::Reload {
                    source: "library".into(),
                    count: count as usize,
                },
            );
        }

        let lines = trail.snapshot();
        assert_eq!(lines.len(), 64);
        assert!(lines[0].contains("count=6"), "{}", lines[0]);
        assert!(lines[63].contains("count=69"), "{}", lines[63]);
    }

    #[test]
    fn trail_renders_elapsed_category_and_payload_on_one_line() {
        let trail = DiagnosticTrail::default();
        trail.push(
            42,
            Event::PlaybackState {
                state: "playing".into(),
            },
        );

        assert_eq!(trail.snapshot(), ["42ms PlaybackState state=playing"]);
    }

    #[test]
    fn sections_changed_renders_its_exact_range() {
        let trail = DiagnosticTrail::default();
        trail.push(
            9,
            Event::SectionsChanged {
                position: 3,
                n_items: 12,
            },
        );
        assert_eq!(
            trail.snapshot(),
            ["9ms SectionsChanged position=3 n_items=12"]
        );
    }

    #[test]
    fn trail_truncates_long_payloads_without_splitting_unicode() {
        let trail = DiagnosticTrail::default();
        trail.push(
            7,
            Event::Reload {
                source: format!("{}\nsecond line", "ä".repeat(200)),
                count: 1,
            },
        );

        let line = &trail.snapshot()[0];
        let payload = line.splitn(3, ' ').nth(2).unwrap();
        assert_eq!(payload.chars().count(), PAYLOAD_LIMIT);
        assert!(payload.ends_with('…'));
        assert_eq!(line.lines().count(), 1);
    }
}
