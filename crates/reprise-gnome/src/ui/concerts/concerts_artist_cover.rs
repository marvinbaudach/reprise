//! Concerts cover column over the shared artist-portrait tile chain.

use std::rc::Rc;

#[cfg(test)]
use std::path::PathBuf;

use gtk4::prelude::*;

use super::concerts_model::ConcertObject;
pub(super) use crate::ui::artist_portrait_tiles::ArtistPortraitTiles as ConcertsArtistImage;
use crate::ui::table_column_widths as widths;
use crate::ui::updates::release_cover::LazyReleaseCover;

#[cfg(test)]
use crate::ui::artist_portrait_tiles::{CacheOnlyPortraitResolver, PRODUCTION_CACHE_ONLY_RESOLVER};

pub(super) fn cover_column(view: &gtk4::ColumnView, image: &Rc<ConcertsArtistImage>) {
    let factory = gtk4::SignalListItemFactory::new();
    let setup_image = image.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let tile = LazyReleaseCover::new_unbound(widths::COVER);
        let image = setup_image.clone();
        tile.widget().connect_map(move |root| {
            if let Some(tile) = LazyReleaseCover::from_widget(root) {
                image.show(&tile);
            }
        });
        item.set_child(Some(tile.widget()));
    });
    let bind_image = image.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(root) = item.child().and_downcast::<gtk4::Overlay>() else {
            return;
        };
        let Some(tile) = LazyReleaseCover::from_widget(&root) else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        tile.set_artist_key(&object.row().artist_name);
        bind_image.show(&tile);
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(root) = item.child().and_downcast::<gtk4::Overlay>() else {
            return;
        };
        if let Some(tile) = LazyReleaseCover::from_widget(&root) {
            tile.set_artist_key("");
        }
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(crate::ui::strings::text(crate::ui::strings::COLUMN_COVER))
        .factory(&factory)
        .resizable(false)
        .build();
    widths::pin(&column, widths::COVER_COLUMN);
    view.append_column(&column);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descendant_with_class(widget: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
        if widget.has_css_class(class) {
            return Some(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = descendant_with_class(&current, class) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_2a_concert_placeholder_is_fully_visible_and_square() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());
        let store = gtk4::gio::ListStore::new::<ConcertObject>();
        store.append(&ConcertObject::new(reprise_core::concerts::ConcertRow {
            id: 1,
            availability: reprise_core::concerts::TicketAvailability::Unknown,
            date_key: "2026-09-01".into(),
            starts_at: "2026-09-01T19:00:00".into(),
            artist_name: "Mental Cruelty".into(),
            venue: "Venue".into(),
            city: "Zurich".into(),
            region: None,
            country: Some("CH".into()),
            latitude: None,
            longitude: None,
            distance_km: None,
            ticket_url: None,
            ticket_source: None,
            event_url: None,
            provider: "fixture".into(),
            is_similar: false,
            similar_to: None,
        }));
        let view = gtk4::ColumnView::new(Some(gtk4::NoSelection::new(Some(store))));
        let image = ConcertsArtistImage::for_test(|_| None);
        cover_column(&view, &image);
        let window = gtk4::Window::builder().child(&view).build();
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let root = descendant_with_class(view.upcast_ref(), "new-release-cover")
            .expect("the Concerts cover cell was realized");
        let cell = root
            .ancestor(
                gtk4::glib::Type::from_name("GtkColumnViewCellWidget")
                    .expect("GTK registered its ColumnView cell widget type"),
            )
            .expect("the tile has a ColumnView cell ancestor");
        let bounds = root.compute_bounds(&cell).expect("tile bounds in its cell");
        assert_eq!((bounds.width(), bounds.height()), (56.0, 56.0));
        assert!(
            bounds.x() >= 0.0 && bounds.x() + bounds.width() <= cell.width() as f32,
            "the 56 px tile is horizontally clipped: x={:.1}, width={:.1}, cell={}",
            bounds.x(),
            bounds.width(),
            cell.width()
        );
        window.close();
    }

    #[test]
    fn test_construction_uses_only_the_injected_cache_resolver() {
        let image = ConcertsArtistImage::for_test(|artist| {
            assert_eq!(artist, "Falling Leaves");
            Some(PathBuf::from("/isolated/portrait.png"))
        });

        assert_eq!(
            (image.cached)("Falling Leaves"),
            Some(PathBuf::from("/isolated/portrait.png"))
        );
        assert!(image.loader.borrow().is_none());
        assert!(image.portrait.borrow().is_none());
    }

    #[test]
    fn production_portrait_resolver_is_pinned_to_the_cache_only_api() {
        let resolver: CacheOnlyPortraitResolver = PRODUCTION_CACHE_ONLY_RESOLVER;
        let cache_only = reprise_core::artist_portrait::load_cached as CacheOnlyPortraitResolver;
        assert!(std::ptr::fn_addr_eq(resolver, cache_only));
    }
}
