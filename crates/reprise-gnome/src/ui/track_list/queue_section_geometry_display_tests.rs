//! Display measurement for restoring a deep viewport in the large Queue view
//! while its real GTK section headers are active.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use reprise_core::browser::{ArtistKey, BrowserPlace, LibraryScope, TrackCollection};
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
    fn prepare_sections(&self, sections: Vec<(u32, u32)>) {
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
fn queue_model() -> queue_sections::QueueViewModel {
    let play_next = (2..=ROWS).map(QueueItem::Track).collect::<Vec<_>>();
    queue_sections::compose(Some(QueueItem::Track(1)), &play_next, &[], Some("Music"))
}

fn sectioned_track_list() -> (TrackList, SectionedTrackModel, gtk4::Window) {
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

fn rendered_queue_headers(column_view: &gtk4::ColumnView) -> Vec<String> {
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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_back_to_a_large_sectioned_queue_never_visits_the_top() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
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

    sectioned.prepare_sections(Vec::new());
    assert!(track_list.restore_browser_place(&BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::Artist(ArtistKey::new(FILTER_ARTIST))),
        Default::default(),
    )));
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list.shared.model.n_items() == (ROWS / FILTER_EVERY) as u32
            && adjustment.upper() > adjustment.page_size()
    });
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    crate::ui::test_settle::settle_for(Duration::from_millis(100));

    let samples: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let sampler = {
        let samples = samples.clone();
        let adjustment = adjustment.clone();
        gtk4::glib::timeout_add_local(SAMPLE_INTERVAL, move || {
            samples.borrow_mut().push(adjustment.value());
            gtk4::glib::ControlFlow::Continue
        })
    };
    sectioned.prepare_sections(queue_ranges);
    assert!(track_list.restore_browser_place(&captured_queue));
    crate::ui::test_settle::settle_for(PAST_THE_RESTORE);
    sampler.remove();

    let restored_ids = track_list.shared.current_view_ids();
    let row_height = adjustment.upper() / restored_ids.len() as f64;
    let anchor_position = restored_ids
        .iter()
        .position(|id| *id == captured_anchor.track_id)
        .expect("the queue anchor must survive the round trip");
    let expected = row_height * anchor_position as f64 + captured_anchor.row_offset;
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
        "QUEUEPROBE headers={headers:?} rows={} row_h={row_height:.1} \
         expected={expected:.0} final={:.0} {sample_report}",
        restored_ids.len(),
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

    window.close();
}
