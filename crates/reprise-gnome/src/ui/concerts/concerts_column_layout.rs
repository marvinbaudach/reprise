//! Concerts-table adapter for the shared column registry.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::library::settings::{CONCERTS_COLUMN_LAYOUT_KEY, CONCERTS_COLUMN_WIDTHS_KEY};
use reprise_view::columns::ConcertColumn;

use crate::ui::table_column_widths as widths;
use crate::ui::table_columns::registry::{bind_columns_by_id, ColumnRegistry, TableKeys};
use crate::ui::table_columns::width_persistence;
#[cfg(test)]
use crate::ui::table_columns::EditorModel;

pub(super) fn registry(view: &gtk4::ColumnView, conn: Rc<Db>) -> Rc<ColumnRegistry<ConcertColumn>> {
    let columns = bind_columns_by_id::<ConcertColumn>(view);
    let registry = ColumnRegistry::new(
        view,
        conn,
        TableKeys {
            layout: CONCERTS_COLUMN_LAYOUT_KEY,
            widths: CONCERTS_COLUMN_WIDTHS_KEY,
        },
        columns,
    );
    width_persistence::wire(&registry, label, width, ConcertColumn::Venue);
    registry.apply(&registry.layout());
    registry
}

#[cfg(test)]
pub(super) fn model(registry: &Rc<ColumnRegistry<ConcertColumn>>) -> Rc<dyn EditorModel> {
    registry.clone()
}

fn label(key: ConcertColumn) -> String {
    let message = match key {
        ConcertColumn::Date => crate::ui::strings::CONCERTS_DATE,
        ConcertColumn::Artist => crate::ui::strings::CONCERTS_ARTIST,
        ConcertColumn::City => crate::ui::strings::CONCERTS_CITY,
        ConcertColumn::Venue => crate::ui::strings::CONCERTS_VENUE,
        ConcertColumn::Distance => crate::ui::strings::CONCERTS_DISTANCE,
        ConcertColumn::Tickets => crate::ui::strings::CONCERTS_TICKETS,
        ConcertColumn::Source => crate::ui::strings::CONCERTS_SOURCE,
    };
    crate::ui::strings::text(message)
}

fn width(key: ConcertColumn) -> i32 {
    match key {
        ConcertColumn::Date => widths::DATE,
        ConcertColumn::Artist => widths::TITLE_MIN,
        ConcertColumn::City => widths::LABEL,
        ConcertColumn::Venue => widths::NAME,
        ConcertColumn::Distance => widths::NUMERIC,
        ConcertColumn::Tickets => widths::ACTION,
        ConcertColumn::Source => widths::LABEL,
    }
}
