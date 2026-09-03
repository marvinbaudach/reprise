//! Shared mutable state for the Concerts view's cohesive helper modules.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use libadwaita as adw;
use reprise_core::concerts::{ConcertFailure, ConcertRow};
use reprise_core::connectivity::Connectivity;
use reprise_core::db::Db;

use super::concerts_empty_state::ConcertsEmptyState;
use super::concerts_filter_bar::ConcertsFilterBar;
use super::concerts_location_banner::ConcertsLocationBanner;
use super::concerts_location_columns::LocationColumns;
use super::concerts_model::ConcertsModel;
use super::concerts_worker::ConcertsRuntime;
use crate::ui::external_link::LaunchErrorSlot;
use crate::ui::feed_footer::FeedFooter;
use crate::ui::source_empty_state::SourceFailureState;
use crate::ui::source_error_banner::SourceErrorBanner;

type Callback = Rc<dyn Fn()>;

pub(super) struct Shared {
    pub(super) conn: Rc<Db>,
    pub(super) runtime: Rc<ConcertsRuntime>,
    pub(super) model: Rc<ConcertsModel>,
    pub(super) filter_bar: Rc<ConcertsFilterBar>,
    pub(super) end_of_results: Rc<super::concerts_end_of_results::ConcertsEndOfResults>,
    pub(super) rows: RefCell<Vec<ConcertRow>>,
    pub(super) cached_items: Cell<usize>,
    pub(super) column_view: gtk4::ColumnView,
    pub(super) column_model: Rc<dyn crate::ui::table_columns::EditorModel>,
    pub(super) location_columns: LocationColumns,
    pub(super) stack: gtk4::Stack,
    pub(super) status: adw::StatusPage,
    pub(super) status_button: gtk4::Button,
    pub(super) footer: FeedFooter,
    pub(super) error_banner: SourceErrorBanner,
    pub(super) location_banner: ConcertsLocationBanner,
    pub(super) failure_state: SourceFailureState,
    pub(super) fetch_failure: RefCell<Option<ConcertFailure>>,
    pub(super) failure_occurred_at: RefCell<String>,
    pub(super) connectivity: Cell<Connectivity>,
    pub(super) fetching: Cell<bool>,
    pub(super) loaded_this_visit: Cell<bool>,
    pub(super) generation: Cell<u64>,
    pub(super) refresh_timer: Cell<Option<gtk4::glib::SourceId>>,
    pub(super) empty_state: Cell<ConcertsEmptyState>,
    pub(super) on_clear_filters: RefCell<Option<Callback>>,
    pub(super) on_refreshed: RefCell<Option<Callback>>,
    pub(super) on_open_preferences: RefCell<Option<Callback>>,
    pub(super) on_launch_error: LaunchErrorSlot,
}
