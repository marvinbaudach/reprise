//! Bounded, silent history of coarse track-list events.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const CAPACITY: usize = 64;
const PAYLOAD_LIMIT: usize = 1024;
const RELOAD_STEP_COUNT: usize = ReloadStep::ALL.len();
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
    reload_id: u64,
}

pub(crate) struct PendingReloadMeasurement {
    started: Instant,
    work_done: Instant,
    cause: ReloadCause,
    source: String,
    rows: usize,
    query_elapsed: Option<Duration>,
    reload_id: u64,
}

#[derive(Clone, Copy)]
pub(crate) enum ReloadStep {
    Geometry,
    Query,
    StateSwap,
    ItemsChanged,
    QueueHeader,
    BrowseCount,
    EmptyState,
    TrailLogging,
    OnReload,
}

impl ReloadStep {
    const ALL: &'static [Self] = &[
        Self::Geometry,
        Self::Query,
        Self::StateSwap,
        Self::ItemsChanged,
        Self::QueueHeader,
        Self::BrowseCount,
        Self::EmptyState,
        Self::TrailLogging,
        Self::OnReload,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Geometry => 0,
            Self::Query => 1,
            Self::StateSwap => 2,
            Self::ItemsChanged => 3,
            Self::QueueHeader => 4,
            Self::BrowseCount => 5,
            Self::EmptyState => 6,
            Self::TrailLogging => 7,
            Self::OnReload => 8,
        }
    }
}

const _: () = {
    let mut index = 0;
    while index < RELOAD_STEP_COUNT {
        assert!(ReloadStep::ALL[index].index() == index);
        index += 1;
    }
};

#[derive(Default)]
struct ActiveReloadBreakdown {
    reload_id: u64,
    steps_us: [u128; RELOAD_STEP_COUNT],
    item_calls: u64,
    item_us: u128,
    window_calls: u64,
    window_us: u128,
}

impl ReloadTimer {
    pub(crate) fn started_at(started: Instant, cause: ReloadCause, reload_id: u64) -> Self {
        Self {
            started,
            cause,
            reload_id,
        }
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
            reload_id: self.reload_id,
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
            reload_id: self.reload_id,
        });
        tracing::debug!(
            cause = self.cause.label(),
            source = self.source,
            rows = self.rows,
            query_us,
            work_done_us,
            next_frame_us,
            reload_id = self.reload_id,
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
        reload_id: u64,
        cause: ReloadCause,
        source: String,
        rows: usize,
        query_us: Option<u128>,
        work_done_us: u128,
        next_frame_us: u128,
    },
    ReloadBreakdown {
        reload_id: u64,
        steps_us: [u128; RELOAD_STEP_COUNT],
        step_sum_us: u128,
        whole_us: u128,
        old_total: u32,
        new_total: usize,
        selected: u64,
        adjustment_value: f64,
        adjustment_upper: f64,
        item_calls: u64,
        item_us: u128,
        window_calls: u64,
        window_us: u128,
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
    ScrollRestoreStandDown {
        writer: String,
        destination: f64,
        rejected: f64,
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
                reload_id,
                cause,
                source,
                rows,
                query_us,
                work_done_us,
                next_frame_us,
            } => (
                "Reload",
                format!(
                    "reload_id={reload_id} cause={} rows={rows} query_us={} work_done_us={work_done_us} next_frame_us={next_frame_us} source={source}",
                    cause.label(),
                    query_us.map_or_else(|| "none".into(), |value| value.to_string())
                ),
            ),
            Self::ReloadBreakdown {
                reload_id,
                steps_us,
                step_sum_us,
                whole_us,
                old_total,
                new_total,
                selected,
                adjustment_value,
                adjustment_upper,
                item_calls,
                item_us,
                window_calls,
                window_us,
            } => (
                "ReloadBreakdown",
                format!(
                    "reload_id={reload_id} geometry_us={} query_us={} state_swap_us={} items_changed_us={} queue_header_us={} browse_count_us={} empty_state_us={} trail_logging_us={} on_reload_us={} step_sum_us={step_sum_us} whole_us={whole_us} old_total={old_total} new_total={new_total} selected={selected} adjustment_value={adjustment_value:.2} adjustment_upper={adjustment_upper:.2} item_calls={item_calls} item_us={item_us} window_calls={window_calls} window_us={window_us}",
                    steps_us[0], steps_us[1], steps_us[2], steps_us[3], steps_us[4],
                    steps_us[5], steps_us[6], steps_us[7], steps_us[8]
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
            Self::ScrollRestoreStandDown {
                writer,
                destination,
                rejected,
            } => (
                "ScrollRestoreStandDown",
                format!(
                    "writer={writer} destination={destination:.2} rejected={rejected:.2}"
                ),
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
    static RELOAD_RECORDING_ARMED: Cell<bool> = const { Cell::new(false) };
    static ACTIVE_RELOAD: RefCell<Option<ActiveReloadBreakdown>> = const { RefCell::new(None) };
    static NEXT_RELOAD_ID: Cell<u64> = const { Cell::new(1) };
    #[cfg(test)]
    static RECORDING_TIMESTAMP_READS: Cell<u64> = const { Cell::new(0) };
}

fn recording_now() -> Instant {
    #[cfg(test)]
    RECORDING_TIMESTAMP_READS.with(|reads| reads.set(reads.get().saturating_add(1)));
    Instant::now()
}

#[cfg(test)]
fn reset_recording_timestamp_reads() {
    RECORDING_TIMESTAMP_READS.with(|reads| reads.set(0));
}

#[cfg(test)]
fn recording_timestamp_reads() -> u64 {
    RECORDING_TIMESTAMP_READS.with(Cell::get)
}

pub(crate) fn arm_reload_recording() {
    RELOAD_RECORDING_ARMED.set(true);
}

pub(crate) fn reload_recording_armed() -> bool {
    RELOAD_RECORDING_ARMED.get()
}

pub(crate) fn begin_reload_breakdown() -> Option<(Instant, u64)> {
    if !reload_recording_armed() {
        return None;
    }
    let reload_id = NEXT_RELOAD_ID.with(|next| {
        let current = next.get();
        next.set(current.wrapping_add(1).max(1));
        current
    });
    ACTIVE_RELOAD.with(|active| {
        *active.borrow_mut() = Some(ActiveReloadBreakdown {
            reload_id,
            ..ActiveReloadBreakdown::default()
        });
    });
    Some((recording_now(), reload_id))
}

pub(crate) fn record_reload_step(step: ReloadStep, elapsed: Duration) {
    ACTIVE_RELOAD.with(|active| {
        if let Some(active) = active.borrow_mut().as_mut() {
            let index = step.index();
            active.steps_us[index] = active.steps_us[index].saturating_add(elapsed.as_micros());
        }
    });
}

pub(crate) fn measure_reload_step<T>(step: ReloadStep, operation: impl FnOnce() -> T) -> T {
    measure_recorded(operation, |elapsed| record_reload_step(step, elapsed))
}

pub(crate) fn start_reload_step() -> Option<Instant> {
    reload_recording_armed().then(recording_now)
}

pub(crate) fn finish_reload_step(step: ReloadStep, started: Option<Instant>) -> Option<Duration> {
    started.map(|started| {
        let elapsed = recording_now().duration_since(started);
        record_reload_step(step, elapsed);
        elapsed
    })
}

pub(crate) fn measure_item_call<T>(operation: impl FnOnce() -> T) -> T {
    measure_recorded(operation, record_item_call)
}

pub(crate) fn measure_window_query<T>(operation: impl FnOnce() -> T) -> T {
    measure_recorded(operation, record_window_query)
}

fn measure_recorded<T>(operation: impl FnOnce() -> T, record: impl FnOnce(Duration)) -> T {
    if !reload_recording_armed() {
        return operation();
    }
    let started = recording_now();
    let result = operation();
    record(recording_now().duration_since(started));
    result
}

pub(crate) fn record_item_call(elapsed: Duration) {
    ACTIVE_RELOAD.with(|active| {
        if let Some(active) = active.borrow_mut().as_mut() {
            active.item_calls = active.item_calls.saturating_add(1);
            active.item_us = active.item_us.saturating_add(elapsed.as_micros());
        }
    });
}

pub(crate) fn record_window_query(elapsed: Duration) {
    ACTIVE_RELOAD.with(|active| {
        if let Some(active) = active.borrow_mut().as_mut() {
            active.window_calls = active.window_calls.saturating_add(1);
            active.window_us = active.window_us.saturating_add(elapsed.as_micros());
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_reload_breakdown(
    trail: &DiagnosticTrail,
    reload_id: u64,
    whole: Duration,
    old_total: u32,
    new_total: usize,
    selected: u64,
    adjustment_value: f64,
    adjustment_upper: f64,
) {
    let active = ACTIVE_RELOAD.with(|active| active.borrow_mut().take());
    let Some(active) = active.filter(|active| active.reload_id == reload_id) else {
        return;
    };
    let step_sum_us = active.steps_us.iter().copied().sum();
    trail.record(Event::ReloadBreakdown {
        reload_id,
        steps_us: active.steps_us,
        step_sum_us,
        whole_us: whole.as_micros(),
        old_total,
        new_total,
        selected,
        adjustment_value,
        adjustment_upper,
        item_calls: active.item_calls,
        item_us: active.item_us,
        window_calls: active.window_calls,
        window_us: active.window_us,
    });
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
