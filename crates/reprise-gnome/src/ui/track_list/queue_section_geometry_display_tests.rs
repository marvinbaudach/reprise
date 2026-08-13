//! Display measurement for restoring a deep viewport in the large Queue view
//! while its real GTK section headers are active.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use reprise_core::browser::{ArtistKey, BrowserPlace, LibraryScope, TrackAnchor, TrackCollection};
use reprise_core::up_next::QueueItem;
use reprise_core::view_source::ViewSource;

use crate::ui::track_list::queue_sections;
use crate::ui::track_list::track_list_model::TrackListModel;
use crate::ui::track_list::TrackList;

const ROWS: i64 = 2_276;
const FILTER_ARTIST: &str = "Filter Artist";
const FILTER_EVERY: i64 = 100;
const QUEUE_ANCHOR_POSITION: u32 = 1_100;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(8);
const PAST_THE_RESTORE: Duration = Duration::from_millis(600);
const MIN_SAMPLES: usize = 20;

mod sectioned_model {
    use super::*;

    #[derive(Default)]
    pub struct SectionedTrackModel {
        pub inner: RefCell<Option<TrackListModel>>,
        pub sections: RefCell<Vec<(u32, u32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SectionedTrackModel {
        const NAME: &'static str = "RepriseQueueGeometryTestModel";
        type Type = super::SectionedTrackModel;
        type ParentType = glib::Object;
        type Interfaces = (gio::ListModel, gtk4::SectionModel);
    }

    impl ObjectImpl for SectionedTrackModel {}

    impl ListModelImpl for SectionedTrackModel {
        fn item_type(&self) -> glib::Type {
            self.inner
                .borrow()
                .as_ref()
                .expect("the proxy must have an inner model")
                .item_type()
        }

        fn n_items(&self) -> u32 {
            self.inner
                .borrow()
                .as_ref()
                .expect("the proxy must have an inner model")
                .n_items()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.inner
                .borrow()
                .as_ref()
                .expect("the proxy must have an inner model")
                .item(position)
        }
    }

    impl SectionModelImpl for SectionedTrackModel {
        fn section(&self, position: u32) -> (u32, u32) {
            let sections = self.sections.borrow();
            for &(start, end) in sections.iter() {
                if position >= start && position < end {
                    return (start, end);
                }
            }
            let last_end = sections.iter().map(|&(_, end)| end).max().unwrap_or(0);
            (last_end, self.n_items().max(position.saturating_add(1)))
        }
    }
}

glib::wrapper! {
    pub struct SectionedTrackModel(ObjectSubclass<sectioned_model::SectionedTrackModel>)
        @implements gio::ListModel, gtk4::SectionModel;
}

impl SectionedTrackModel {
    fn new(inner: &TrackListModel) -> Self {
        let model: Self = glib::Object::new();
        model.imp().inner.replace(Some(inner.clone()));
        let weak = model.downgrade();
        inner.connect_items_changed(move |_, position, removed, added| {
            let Some(model) = weak.upgrade() else {
                return;
            };
            // Items only, exactly like production: `TrackListModel` emits
            // `items-changed` and lets GTK re-read `section()` while it
            // rebuilds the rows. Measured: an extra `sections-changed` here
            // changes nothing either way.
            model.items_changed(position, removed, added);
        });
        model
    }

    /// Stages the section map before the inner model emits its row change.
    /// Emitting here would expose the next view's ranges against the current
    /// view's row count, the exact inconsistent handover this test must avoid.
    pub(in crate::ui::track_list) fn prepare_sections(&self, sections: Vec<(u32, u32)>) {
        *self.imp().sections.borrow_mut() = sections;
    }
}

/// Every queue row is materialised on purpose — no virtual context tail.
///
/// `run_query` builds the Queue's context window from the live
/// `PlayerController` (`QueueContextWindow::from_player`), and a display test
/// has no player, so `rows()` answers every request with an empty vector.
/// Measured with a virtual tail of 2273 rows: `item()` returns `None` for each
/// of them, GTK receives NULL for rows it was told exist, logs one
/// `g_object_unref: assertion 'G_IS_OBJECT (object)' failed` per fetch (2478 in
/// one run, against zero for every other display test of this app) and never
/// materialises that section — so its header widget is created and left
/// unbound, which is what the first version of this test died on as a missing
/// "Playing from …" precondition.
///
/// Two materialised sections answer the geometry question just as well: a
/// section header costs its height whichever section it titles.
pub(in crate::ui::track_list) fn queue_model() -> queue_sections::QueueViewModel {
    let play_next = (2..=ROWS).map(QueueItem::Track).collect::<Vec<_>>();
    queue_sections::compose(Some(QueueItem::Track(1)), &play_next, &[], Some("Music"))
}

pub(in crate::ui::track_list) fn sectioned_track_list(
) -> (TrackList, SectionedTrackModel, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=ROWS {
        let artist = if id % FILTER_EVERY == 0 {
            FILTER_ARTIST
        } else {
            "Bulk Artist"
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, ?4, 0)",
            (
                id,
                format!("/synthetic/{id:04}.flac"),
                format!("Track {id:04}"),
                artist,
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let queue = queue_model();
    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        move || queue.clone(),
        crate::ui::cover_download_worker::setup_for_test(),
    );
    // Production's TrackListModel implements SectionModel. The test build
    // deliberately omits that interface to keep parallel unit tests from
    // registering GTK types before `gtk4::init()`, so this display-only proxy
    // restores the production interface without adding a production hook.
    let sectioned = SectionedTrackModel::new(&track_list.shared.model);
    track_list.shared.selection.set_model(Some(&sectioned));
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list
            .shared
            .column_view
            .vadjustment()
            .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
    });
    (track_list, sectioned, window)
}

pub(in crate::ui::track_list) fn rendered_queue_headers(
    column_view: &gtk4::ColumnView,
) -> Vec<String> {
    let mut labels = Vec::new();
    let mut pending = vec![column_view.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            if label.has_css_class("queue-section-header") {
                labels.push(label.label().to_string());
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    labels
}

fn descendant_track_title(widget: &gtk4::Widget) -> Option<String> {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        let text = label.label();
        if text.starts_with("Track ") {
            return Some(text.to_string());
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(title) = descendant_track_title(&current) {
            return Some(title);
        }
        child = current.next_sibling();
    }
    None
}

/// Every realized track row as `(title, top edge in ColumnView coordinates)`,
/// sorted by y. A track-title label excludes the ColumnView's own title row.
fn rendered_rows(column_view: &gtk4::ColumnView) -> Vec<(String, f32)> {
    fn collect(
        widget: &gtk4::Widget,
        column_view: &gtk4::ColumnView,
        rows: &mut Vec<(String, f32)>,
    ) {
        if widget.type_().name().contains("ColumnViewRow") && widget.height() > 0 {
            if let (Some(title), Some(bounds)) = (
                descendant_track_title(widget),
                widget.compute_bounds(column_view),
            ) {
                // GTK keeps zero-height widgets in its recycling pool. They
                // are unrealized and do not describe a rendered row.
                if bounds.height() > 0.0 {
                    rows.push((title, bounds.y()));
                }
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, column_view, rows);
            child = current.next_sibling();
        }
    }

    let mut rows = Vec::new();
    collect(column_view.upcast_ref(), column_view, &mut rows);
    rows.sort_by(|left, right| left.1.total_cmp(&right.1));
    rows
}

/// The rendered height of a track row and section header, measured from the
/// widget tree rather than from the geometry cache.
fn rendered_band_heights(column_view: &gtk4::ColumnView) -> Option<(f32, f32)> {
    fn collect(
        widget: &gtk4::Widget,
        column_view: &gtk4::ColumnView,
        headers: &mut Vec<f32>,
        rows: &mut Vec<f32>,
    ) {
        if widget.height() > 0 {
            if let Some(bounds) = widget.compute_bounds(column_view) {
                // Zero-height widgets belong to GTK's recycling pool and are
                // not evidence about either rendered band.
                if bounds.height() > 0.0 {
                    let type_name = widget.type_().name();
                    if type_name.contains("ListHeader") {
                        headers.push(bounds.height());
                    } else if type_name.contains("ColumnViewRow")
                        && descendant_track_title(widget).is_some()
                    {
                        rows.push(bounds.height());
                    }
                }
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, column_view, headers, rows);
            child = current.next_sibling();
        }
    }

    fn uniform(values: &[f32]) -> Option<f32> {
        let first = *values.first()?;
        values
            .iter()
            .all(|value| (*value - first).abs() < 0.5)
            .then_some(first)
    }

    let mut headers = Vec::new();
    let mut rows = Vec::new();
    collect(
        column_view.upcast_ref(),
        column_view,
        &mut headers,
        &mut rows,
    );
    Some((uniform(&rows)?, uniform(&headers)?))
}

struct DeepQueueFixture {
    track_list: TrackList,
    sectioned: SectionedTrackModel,
    window: gtk4::Window,
    queue_ranges: Vec<(u32, u32)>,
    captured_queue: BrowserPlace,
    captured_anchor: TrackAnchor,
    headers: Vec<String>,
    rendered_band_heights: (f32, f32),
    rows_before: Vec<(String, f32)>,
}

impl DeepQueueFixture {
    fn new() -> Self {
        let (track_list, sectioned, window) = sectioned_track_list();
        let queue = queue_model();
        let queue_ranges = queue_sections::section_ranges(&queue.sections);

        sectioned.prepare_sections(queue_ranges.clone());
        assert!(track_list.restore_browser_place(&BrowserPlace::from(ViewSource::Queue)));
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            rendered_queue_headers(&track_list.shared.column_view).len() >= 2
        });
        let headers = rendered_queue_headers(&track_list.shared.column_view);
        assert!(
            headers.iter().any(|title| title == "Now Playing"),
            "precondition: the queue renders its section headers; got {headers:?}"
        );
        assert!(
            headers.iter().any(|title| title == "Play Next"),
            "precondition: the queue renders its section headers; got {headers:?}"
        );
        let rendered_band_heights = rendered_band_heights(&track_list.shared.column_view)
            .expect("the Queue top must expose uniform allocated row and header bands");

        let adjustment = track_list.shared.column_view.vadjustment().unwrap();
        track_list.shared.column_view.scroll_to(
            QUEUE_ANCHOR_POSITION,
            None,
            gtk4::ListScrollFlags::NONE,
            None,
        );
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            adjustment.value() > adjustment.page_size() * 2.0
        });
        let captured_queue = track_list.browser_place();
        let captured_anchor = captured_queue
            .track_state()
            .and_then(|state| state.anchor)
            .expect("a deep queue viewport must capture an anchor");
        let rows_before = rendered_rows(&track_list.shared.column_view);

        Self {
            track_list,
            sectioned,
            window,
            queue_ranges,
            captured_queue,
            captured_anchor,
            headers,
            rendered_band_heights,
            rows_before,
        }
    }

    fn visit_filtered_library(&self) {
        self.sectioned.prepare_sections(Vec::new());
        assert!(self.track_list.restore_browser_place(&BrowserPlace::tracks(
            TrackCollection::Library(LibraryScope::Artist(ArtistKey::new(FILTER_ARTIST))),
            Default::default(),
        )));
        let adjustment = self.track_list.shared.column_view.vadjustment().unwrap();
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            self.track_list.shared.model.n_items() == (ROWS / FILTER_EVERY) as u32
                && adjustment.upper() > adjustment.page_size()
        });
        adjustment.set_value(adjustment.upper() - adjustment.page_size());
        crate::ui::test_settle::settle_for(Duration::from_millis(100));
    }

    fn restore_queue(&self) {
        self.sectioned.prepare_sections(self.queue_ranges.clone());
        assert!(self.track_list.restore_browser_place(&self.captured_queue));
    }

    fn anchor_title(&self) -> String {
        format!("Track {:04}", self.captured_anchor.track_id)
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_back_to_a_large_sectioned_queue_never_visits_the_top() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let fixture = DeepQueueFixture::new();
    let adjustment = fixture.track_list.shared.column_view.vadjustment().unwrap();
    fixture.visit_filtered_library();

    let samples: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let sampler = {
        let samples = samples.clone();
        let adjustment = adjustment.clone();
        gtk4::glib::timeout_add_local(SAMPLE_INTERVAL, move || {
            samples.borrow_mut().push(adjustment.value());
            gtk4::glib::ControlFlow::Continue
        })
    };
    fixture.restore_queue();
    crate::ui::test_settle::settle_for(PAST_THE_RESTORE);
    sampler.remove();

    let restored_ids = fixture.track_list.shared.current_view_ids();
    let (row_height, header_height) = fixture.rendered_band_heights;
    let row_height = f64::from(row_height);
    let header_height = f64::from(header_height);
    let measured_content =
        restored_ids.len() as f64 * row_height + fixture.queue_ranges.len() as f64 * header_height;
    assert!(
        (measured_content - adjustment.upper()).abs() < row_height,
        "geometry precondition failed: rendered Queue bands imply {measured_content}, \
         but the adjustment upper is {}; this is a geometry finding, not necessarily an anchor defect",
        adjustment.upper()
    );
    let anchor_position = restored_ids
        .iter()
        .position(|id| *id == fixture.captured_anchor.track_id)
        .expect("the queue anchor must survive the round trip");
    let headers_above = fixture
        .queue_ranges
        .iter()
        .filter(|(start, _)| *start as usize <= anchor_position)
        .count();
    let expected = row_height * anchor_position as f64
        + header_height * headers_above as f64
        + fixture.captured_anchor.row_offset;
    let anchor_title = fixture.anchor_title();
    let y_before = fixture
        .rows_before
        .iter()
        .find(|(title, _)| title == &anchor_title)
        .map(|(_, y)| *y);
    let rows_after = rendered_rows(&fixture.track_list.shared.column_view);
    let y_after = rows_after
        .iter()
        .find(|(title, _)| title == &anchor_title)
        .map(|(_, y)| *y);
    let samples = samples.borrow();
    let first = samples.first().copied();
    let minimum = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sample_report = format!(
        "samples(n={} first={first:?} min={minimum} max={maximum})",
        samples.len()
    );
    // Printed on the green path too: the plan asks this test for the journey,
    // not just the endpoint, and a passing run is the interesting case for
    // decision 5 (do section headers have to enter the height model?).
    eprintln!(
        "QUEUEPROBE headers={:?} rows={} row_h={row_height:.1} header_h={header_height:.1} \
         headers_above={headers_above} anchor=({}, {:.1}) y_before={y_before:?} \
         y_after={y_after:?} expected={expected:.0} final={:.0} {sample_report}",
        fixture.headers,
        restored_ids.len(),
        fixture.captured_anchor.track_id,
        fixture.captured_anchor.row_offset,
        adjustment.value(),
    );
    assert!(
        samples.len() >= MIN_SAMPLES,
        "the sampler did not cover the sectioned-queue restore; {sample_report}"
    );
    assert!(
        minimum > expected - row_height * 2.0,
        "the sectioned Queue visited the top before restoring its anchor: \
         expected={expected}, row height={row_height}; {sample_report}"
    );
    assert!(
        (adjustment.value() - expected).abs() < row_height,
        "the sectioned Queue did not settle on its anchor: actual={}, expected={expected}, \
         row height={row_height}; {sample_report}",
        adjustment.value()
    );

    fixture.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn queue_anchor_names_the_row_at_the_viewport_top() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let fixture = DeepQueueFixture::new();
    let anchor_title = fixture.anchor_title();
    let first_rows = fixture
        .rows_before
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    let (topmost_title, topmost_y) = fixture
        .rows_before
        .first()
        .expect("the deep Queue viewport must realize track rows");
    assert_eq!(
        topmost_title, &anchor_title,
        "the captured Queue anchor does not name the topmost rendered row; \
         first rendered rows: {first_rows:?}"
    );

    fixture.visit_filtered_library();
    fixture.restore_queue();
    crate::ui::test_settle::settle_for(PAST_THE_RESTORE);

    let rows_after = rendered_rows(&fixture.track_list.shared.column_view);
    let anchor_y_after = rows_after
        .iter()
        .find(|(title, _)| title == &anchor_title)
        .map_or_else(
            || {
                panic!(
                    "the restored anchor row {anchor_title:?} is not rendered; first rows: {:?}",
                    rows_after.iter().take(5).collect::<Vec<_>>()
                )
            },
            |(_, y)| *y,
        );
    assert!(
        (anchor_y_after - *topmost_y).abs() <= 1.0,
        "the Queue anchor row changed its on-screen y across Back: \
         before={topmost_y}, after={anchor_y_after}, first restored rows: {:?}",
        rows_after.iter().take(5).collect::<Vec<_>>()
    );

    fixture.window.close();
}
