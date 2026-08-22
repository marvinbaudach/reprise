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

impl ReloadTimer {
    pub(crate) fn started_at(started: Instant, cause: ReloadCause) -> Self {
        Self { started, cause }
    }

    pub(crate) fn finish(
        self,
        trail: &DiagnosticTrail,
        source: &str,
        rows: usize,
        query_elapsed: Duration,
    ) {
        self.finish_at(trail, Instant::now(), source, rows, query_elapsed);
    }

    fn finish_at(
        self,
        trail: &DiagnosticTrail,
        finished: Instant,
        source: &str,
        rows: usize,
        query_elapsed: Duration,
    ) {
        let query_us = query_elapsed.as_micros();
        let displayable_us = finished.duration_since(self.started).as_micros();
        trail.record(Event::Reload {
            cause: self.cause,
            source: source.to_owned(),
            rows,
            query_us,
            displayable_us,
        });
        tracing::debug!(
            cause = self.cause.label(),
            source,
            rows,
            query_us,
            displayable_us,
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
        query_us: u128,
        displayable_us: u128,
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
                displayable_us,
            } => (
                "Reload",
                format!(
                    "cause={} rows={rows} query_us={query_us} displayable_us={displayable_us} source={source}",
                    cause.label()
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
mod tests {
    use super::*;

    #[test]
    fn reload_measurement_records_cause_rows_and_monotonic_durations() {
        let trail = DiagnosticTrail::default();
        let started = Instant::now();
        let timer = ReloadTimer::started_at(started, ReloadCause::TypedSearch);

        timer.finish_at(
            &trail,
            started + std::time::Duration::from_millis(17),
            "Library",
            42,
            std::time::Duration::from_millis(11),
        );

        let line = &trail.snapshot()[0];
        assert!(line.contains("cause=typed-search"), "{line}");
        assert!(line.contains("source=Library"), "{line}");
        assert!(line.contains("rows=42"), "{line}");
        assert!(line.contains("query_us=11000"), "{line}");
        assert!(line.contains("displayable_us=17000"), "{line}");

        let query_us = payload_number(line, "query_us");
        let displayable_us = payload_number(line, "displayable_us");
        assert!(displayable_us >= query_us);
    }

    #[test]
    fn reload_cause_distinguishes_search_clear_sort_and_source_transitions() {
        use reprise_core::view_source::ViewSource;

        let baseline = (
            ViewSource::Library,
            "artist".to_string(),
            "asc".to_string(),
            String::new(),
        );
        assert_eq!(
            super::super::track_list_reload::reload_cause(
                Some(&baseline),
                &ViewSource::Library,
                "artist",
                "asc",
                "n"
            ),
            ReloadCause::TypedSearch
        );
        let mid_search = (
            ViewSource::Library,
            "artist".to_string(),
            "asc".to_string(),
            "n".to_string(),
        );
        assert_eq!(
            super::super::track_list_reload::reload_cause(
                Some(&mid_search),
                &ViewSource::Library,
                "artist",
                "asc",
                "ne"
            ),
            ReloadCause::TypedSearch
        );
        assert_eq!(
            super::super::track_list_reload::reload_cause(
                Some(&mid_search),
                &ViewSource::Library,
                "artist",
                "asc",
                ""
            ),
            ReloadCause::ClearedSearch
        );
        assert_eq!(
            super::super::track_list_reload::reload_cause(
                Some(&baseline),
                &ViewSource::Library,
                "title",
                "asc",
                ""
            ),
            ReloadCause::SortChange
        );
        assert_eq!(
            super::super::track_list_reload::reload_cause(
                Some(&baseline),
                &ViewSource::Playlist(7),
                "playlist_order",
                "asc",
                ""
            ),
            ReloadCause::SourceSwitch
        );
        assert_eq!(
            super::super::track_list_reload::reload_cause(
                None,
                &ViewSource::Library,
                "artist",
                "asc",
                ""
            ),
            ReloadCause::Other
        );
    }

    fn payload_number(line: &str, field: &str) -> u128 {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(&format!("{field}=")))
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    #[ignore = "measurement: uses the generated database under the isolated XDG data root"]
    fn measure_generated_library_reload_latency() {
        use gtk4::prelude::*;

        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let db_path = reprise_core::db::default_path();
        let conn = reprise_core::db::Db::open_ready(&db_path).unwrap();
        let track_list = super::super::TrackList::new(
            Rc::new(conn),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            super::super::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let window = gtk4::Window::builder()
            .default_width(1600)
            .default_height(1000)
            .child(track_list.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        for sample in 1..=5 {
            super::super::track_list_reload::set_filter_and_reload(&track_list.shared, "N");
            print_latest_reload(&track_list.shared, sample, "first-keystroke");

            super::super::track_list_reload::set_filter_and_reload(&track_list.shared, "Ne");
            print_latest_reload(&track_list.shared, sample, "mid-typing");

            super::super::track_list_reload::set_filter_and_reload(&track_list.shared, "");
            print_latest_reload(&track_list.shared, sample, "clear-search");

            *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
                field: "title".into(),
                dir: "asc".into(),
            };
            super::super::track_list_reload::reload(&track_list.shared);
            print_latest_reload(&track_list.shared, sample, "sort-change");

            super::super::track_list_reload::set_source_and_reload(
                &track_list.shared,
                &reprise_core::view_source::ViewSource::Missing,
            );
            print_latest_reload(&track_list.shared, sample, "source-to-missing");
            super::super::track_list_reload::set_source_and_reload(
                &track_list.shared,
                &reprise_core::view_source::ViewSource::Library,
            );
            print_latest_reload(&track_list.shared, sample, "source-to-library");
        }
        window.close();
    }

    fn print_latest_reload(shared: &super::super::Shared, sample: usize, case: &str) {
        let line = shared
            .diagnostic_trail
            .snapshot()
            .into_iter()
            .rev()
            .find(|line| line.contains(" Reload "))
            .expect("a synchronous reload must append its timing event");
        eprintln!("RELOAD_SAMPLE sample={sample} case={case} {line}");
    }

    #[test]
    fn trail_keeps_the_newest_64_entries_in_oldest_first_order() {
        let trail = DiagnosticTrail::default();
        for count in 0..70 {
            trail.push(
                count,
                Event::Reload {
                    cause: ReloadCause::Other,
                    source: "library".into(),
                    rows: count as usize,
                    query_us: 1,
                    displayable_us: 2,
                },
            );
        }

        let lines = trail.snapshot();
        assert_eq!(lines.len(), 64);
        assert!(lines[0].contains("rows=6"), "{}", lines[0]);
        assert!(lines[63].contains("rows=69"), "{}", lines[63]);
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
                cause: ReloadCause::Other,
                source: format!("{}\nsecond line", "ä".repeat(200)),
                rows: 1,
                query_us: 1,
                displayable_us: 2,
            },
        );

        let line = &trail.snapshot()[0];
        let payload = line.splitn(3, ' ').nth(2).unwrap();
        assert_eq!(payload.chars().count(), PAYLOAD_LIMIT);
        assert!(payload.ends_with('…'));
        assert_eq!(line.lines().count(), 1);
    }
}
