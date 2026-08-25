//! Cover column kept separate so the interaction-heavy columns stay small.

use std::rc::Rc;

use gtk4::prelude::*;

use super::releases_cell_surface::{self as cell_surface, OnWireCell};
use super::releases_model::ReleaseObject;
use crate::ui::strings;
use crate::ui::table_column_widths as widths;

pub(super) fn append(
    view: &gtk4::ColumnView,
    on_wire_cell: &OnWireCell,
    artist_image: &Rc<crate::ui::artist_portrait_tiles::ArtistPortraitTiles>,
) {
    let factory = gtk4::SignalListItemFactory::new();
    let on_wire_cell = on_wire_cell.clone();
    let setup_image = artist_image.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cover = crate::ui::updates::release_cover::LazyReleaseCover::new_unbound(widths::COVER);
        cover.connect_artist_portrait_tiles(setup_image.clone());
        cell_surface::set_child(item, cover.widget(), on_wire_cell.as_ref());
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(root) = cell_surface::child::<gtk4::Overlay>(item) else {
            return;
        };
        let Some(cover) = crate::ui::updates::release_cover::LazyReleaseCover::from_widget(&root)
        else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ReleaseObject>() else {
            return;
        };
        let entry = object.entry();
        cover.set_release(&entry.release_group_mbid, &entry.artist_name);
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(root) = cell_surface::child::<gtk4::Overlay>(item) else {
            return;
        };
        if let Some(cover) = crate::ui::updates::release_cover::LazyReleaseCover::from_widget(&root)
        {
            cover.set_release("", "");
        }
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::COLUMN_COVER))
        .factory(&factory)
        .resizable(false)
        .build();
    widths::pin(&column, widths::COVER_COLUMN);
    view.append_column(&column);
}
