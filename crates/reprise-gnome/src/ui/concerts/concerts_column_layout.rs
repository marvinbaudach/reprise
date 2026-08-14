//! Concerts-table adapter for the shared column registry.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::library::settings::{CONCERTS_COLUMN_LAYOUT_KEY, CONCERTS_COLUMN_WIDTHS_KEY};
use reprise_view::columns::{ColumnKey, Pin};

use crate::ui::table_column_widths as widths;
use crate::ui::table_columns::registry::{bind_columns_by_id, ColumnRegistry, TableKeys};
use crate::ui::table_columns::{width_persistence, EditorModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ConcertTableColumn {
    Date,
    Artist,
    City,
    Venue,
    Distance,
    Tickets,
    Source,
}

const ALL: [ConcertTableColumn; 7] = [
    ConcertTableColumn::Date,
    ConcertTableColumn::Artist,
    ConcertTableColumn::City,
    ConcertTableColumn::Venue,
    ConcertTableColumn::Distance,
    ConcertTableColumn::Tickets,
    ConcertTableColumn::Source,
];

const DEFAULT_VISIBLE: [ConcertTableColumn; 6] = [
    ConcertTableColumn::Date,
    ConcertTableColumn::Artist,
    ConcertTableColumn::City,
    ConcertTableColumn::Venue,
    ConcertTableColumn::Distance,
    ConcertTableColumn::Tickets,
];

impl ColumnKey for ConcertTableColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Artist => "artist",
            Self::City => "city",
            Self::Venue => "venue",
            Self::Distance => "distance",
            Self::Tickets => "tickets",
            Self::Source => "source",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        ALL.iter().copied().find(|column| column.as_str() == value)
    }

    fn all() -> &'static [Self] {
        &ALL
    }

    fn default_visible() -> &'static [Self] {
        &DEFAULT_VISIBLE
    }

    fn pin(self) -> Option<Pin> {
        None
    }
}

pub(super) fn registry(
    view: &gtk4::ColumnView,
    conn: Rc<Db>,
) -> Rc<ColumnRegistry<ConcertTableColumn>> {
    let columns = bind_columns_by_id::<ConcertTableColumn>(view);
    let registry = ColumnRegistry::new(
        view,
        conn,
        TableKeys {
            layout: CONCERTS_COLUMN_LAYOUT_KEY,
            widths: CONCERTS_COLUMN_WIDTHS_KEY,
        },
        columns,
    );
    width_persistence::wire(&registry, label, width, ConcertTableColumn::Venue);
    registry.apply(&registry.layout());
    registry
}

pub(super) fn model(registry: &Rc<ColumnRegistry<ConcertTableColumn>>) -> Rc<dyn EditorModel> {
    registry.clone()
}

fn label(key: ConcertTableColumn) -> String {
    let message = match key {
        ConcertTableColumn::Date => crate::ui::strings::CONCERTS_DATE,
        ConcertTableColumn::Artist => crate::ui::strings::CONCERTS_ARTIST,
        ConcertTableColumn::City => crate::ui::strings::CONCERTS_CITY,
        ConcertTableColumn::Venue => crate::ui::strings::CONCERTS_VENUE,
        ConcertTableColumn::Distance => crate::ui::strings::CONCERTS_DISTANCE,
        ConcertTableColumn::Tickets => crate::ui::strings::CONCERTS_TICKETS,
        ConcertTableColumn::Source => crate::ui::strings::CONCERTS_SOURCE,
    };
    crate::ui::strings::text(message)
}

fn width(key: ConcertTableColumn) -> i32 {
    match key {
        ConcertTableColumn::Date => widths::DATE,
        ConcertTableColumn::Artist => widths::TITLE_MIN,
        ConcertTableColumn::City => widths::LABEL,
        ConcertTableColumn::Venue => widths::NAME,
        ConcertTableColumn::Distance => widths::NUMERIC,
        ConcertTableColumn::Tickets => widths::ACTION,
        ConcertTableColumn::Source => widths::LABEL,
    }
}
