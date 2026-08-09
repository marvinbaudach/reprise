//! Radio-table adapter for the shared column registry.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::library::settings::{RADIO_COLUMN_LAYOUT_KEY, RADIO_COLUMN_WIDTHS_KEY};
use reprise_view::columns::RadioColumn;

use crate::ui::table_column_widths as widths;
use crate::ui::table_columns::registry::{bind_columns_by_id, ColumnRegistry, TableKeys};
use crate::ui::table_columns::{width_persistence, EditorModel};

pub(super) fn registry(view: &gtk4::ColumnView, conn: Rc<Db>) -> Rc<ColumnRegistry<RadioColumn>> {
    let columns = bind_columns_by_id::<RadioColumn>(view);
    let registry = ColumnRegistry::new(
        view,
        conn,
        TableKeys {
            layout: RADIO_COLUMN_LAYOUT_KEY,
            widths: RADIO_COLUMN_WIDTHS_KEY,
        },
        columns,
    );
    width_persistence::wire(&registry, label, width, RadioColumn::Station);
    registry.apply(&registry.layout());
    registry
}

pub(super) fn model(registry: &Rc<ColumnRegistry<RadioColumn>>) -> Rc<dyn EditorModel> {
    registry.clone()
}

pub(super) fn install(view: &gtk4::ColumnView, conn: Rc<Db>) -> Rc<dyn EditorModel> {
    let registry = registry(view, conn);
    let model = model(&registry);
    crate::ui::table_columns::header_popover::install_header_popover(view, &model);
    crate::ui::table_columns::header_dnd::install_header_drag(view, &model);
    model
}

fn label(key: RadioColumn) -> String {
    let message = match key {
        RadioColumn::Artwork => crate::ui::strings::COLUMN_COVER,
        RadioColumn::State => crate::ui::strings::COLUMN_STATUS,
        RadioColumn::Station => crate::ui::strings::RADIO_STATION,
        RadioColumn::Genre => crate::ui::strings::RADIO_GENRE,
        RadioColumn::Bitrate => crate::ui::strings::RADIO_BITRATE,
        RadioColumn::Country => crate::ui::strings::RADIO_COUNTRY,
        RadioColumn::NowPlaying => crate::ui::strings::RADIO_NOW_PLAYING,
    };
    crate::ui::strings::text(message)
}

fn width(key: RadioColumn) -> i32 {
    match key {
        RadioColumn::Artwork => crate::ui::source_row::MEDIA_WIDTH,
        RadioColumn::State => widths::ICON_ACTION,
        RadioColumn::Station => widths::TITLE_MIN,
        RadioColumn::Genre => widths::LABEL,
        RadioColumn::Bitrate => widths::NUMERIC,
        RadioColumn::Country => widths::SHORT_LABEL,
        RadioColumn::NowPlaying => widths::NAME,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_radio_editor_label_uses_the_string_catalog() {
        assert_eq!(
            label(RadioColumn::Artwork),
            crate::ui::strings::text(crate::ui::strings::COLUMN_COVER)
        );
        assert_eq!(
            label(RadioColumn::State),
            crate::ui::strings::text(crate::ui::strings::COLUMN_STATUS)
        );
    }
}
