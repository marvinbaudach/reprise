//! Bounded, silent history of coarse track-list events.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const CAPACITY: usize = 64;
const PAYLOAD_LIMIT: usize = 120;
static PROCESS_START: OnceLock<Instant> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReloadCause {
    TypedSearch,
    ClearedSearch,
    SortChange,
    SourceSwitch,
    Other,
}

impl ReloadCause {
    fn label(self) -> &'static str {
        match self {
            Self::TypedSearch => "typed-search",
            Self::ClearedSearch => "cleared-search",
            Self::SortChange => "sort-change",
            Self::SourceSwitch => "source-switch",
            Self::Other => "other",
        }
    }
}

pub(crate) struct ReloadTimer {
    started: Instant,
    cause: ReloadCause,
}

pub(crate) struct PendingReloadMeasurement {
    started: Instant,
    work_done: Instant,
    cause: ReloadCause,
    source: String,
    rows: usize,
    query_elapsed: Option<Duration>,
}

impl ReloadTimer {
    pub(crate) fn started_at(started: Instant, cause: ReloadCause) -> Self {
        Self { started, cause }
    }

    pub(crate) fn work_done(
        self,
        source: &str,
        rows: usize,
        query_elapsed: Option<Duration>,
    ) -> PendingReloadMeasurement {
        self.work_done_at(Instant::now(), source, rows, query_elapsed)
    }

    fn work_done_at(
        self,
        work_done: Instant,
        source: &str,
        rows: usize,
        query_elapsed: Option<Duration>,
    ) -> PendingReloadMeasurement {
        PendingReloadMeasurement {
            started: self.started,
            work_done,
            cause: self.cause,
            source: source.to_owned(),
            rows,
            query_elapsed,
        }
    }
}

impl PendingReloadMeasurement {
    pub(crate) fn next_frame(self, trail: &DiagnosticTrail) {
        self.next_frame_at(trail, Instant::now());
    }

    fn next_frame_at(self, trail: &DiagnosticTrail, next_frame: Instant) {
        let query_us = self.query_elapsed.map(|elapsed| elapsed.as_micros());
        let work_done_us = self.work_done.duration_since(self.started).as_micros();
        let next_frame_us = next_frame.duration_since(self.started).as_micros();
        trail.record(Event::Reload {
            cause: self.cause,
            source: self.source.clone(),
            rows: self.rows,
            query_us,
            work_done_us,
            next_frame_us,
        });
        tracing::debug!(
            cause = self.cause.label(),
            source = self.source,
            rows = self.rows,
            query_us,
            work_done_us,
            next_frame_us,
            "track-list reload measured"
        );
    }
}

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
        cause: ReloadCause,
        source: String,
        rows: usize,
        query_us: Option<u128>,
        work_done_us: u128,
        next_frame_us: u128,
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
            Self::Reload {
                cause,
                source,
                rows,
                query_us,
                work_done_us,
                next_frame_us,
            } => (
                "Reload",
                format!(
                    "cause={} rows={rows} query_us={} work_done_us={work_done_us} next_frame_us={next_frame_us} source={source}",
                    cause.label(),
                    query_us.map_or_else(|| "none".into(), |value| value.to_string())
                ),
            ),
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

pub(crate) fn process_start() -> Option<Instant> {
    PROCESS_START.get().copied()
}

pub(crate) fn handle() -> Rc<DiagnosticTrail> {
    THREAD_TRAIL.with(Rc::clone)
}

pub(crate) fn record(event: Event) {
    THREAD_TRAIL.with(|trail| trail.record(event));
}

#[cfg(test)]
#[path = "diagnostic_trail_tests.rs"]
mod tests;
