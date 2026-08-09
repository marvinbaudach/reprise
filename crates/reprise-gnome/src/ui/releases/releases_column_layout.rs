//! Releases-table adapter for the shared column registry.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::library::settings::{RELEASES_COLUMN_LAYOUT_KEY, RELEASES_COLUMN_WIDTHS_KEY};
use reprise_view::columns::ReleaseColumn;

use crate::ui::table_column_widths as widths;
use crate::ui::table_columns::registry::{bind_columns_by_id, ColumnRegistry, TableKeys};
use crate::ui::table_columns::{width_persistence, EditorModel};

pub(super) fn registry(view: &gtk4::ColumnView, conn: Rc<Db>) -> Rc<ColumnRegistry<ReleaseColumn>> {
    let columns = bind_columns_by_id::<ReleaseColumn>(view);
    let registry = ColumnRegistry::new(
        view,
        conn,
        TableKeys {
            layout: RELEASES_COLUMN_LAYOUT_KEY,
            widths: RELEASES_COLUMN_WIDTHS_KEY,
        },
        columns,
    );
    width_persistence::wire(&registry, label, width, ReleaseColumn::Title);
    registry.apply(&registry.layout());
    registry
}

pub(super) fn model(registry: &Rc<ColumnRegistry<ReleaseColumn>>) -> Rc<dyn EditorModel> {
    registry.clone()
}

fn label(key: ReleaseColumn) -> String {
    let message = match key {
        ReleaseColumn::Cover => crate::ui::strings::COLUMN_COVER,
        ReleaseColumn::Date => crate::ui::strings::RELEASES_DATE,
        ReleaseColumn::Title => crate::ui::strings::RELEASES_TITLE,
        ReleaseColumn::Artist => crate::ui::strings::RELEASES_ARTIST,
        ReleaseColumn::Type => crate::ui::strings::RELEASES_TYPE,
        ReleaseColumn::Status => crate::ui::strings::RELEASES_STATUS,
        ReleaseColumn::Buy => crate::ui::strings::RELEASES_BUY,
    };
    crate::ui::strings::text(message)
}

fn width(key: ReleaseColumn) -> i32 {
    match key {
        ReleaseColumn::Cover => 40,
        ReleaseColumn::Date => widths::DATE,
        ReleaseColumn::Title => widths::TITLE_MIN,
        ReleaseColumn::Artist => widths::NAME,
        ReleaseColumn::Type => widths::SHORT_LABEL,
        ReleaseColumn::Status => widths::PILL,
        ReleaseColumn::Buy => widths::ACTION,
    }
}
