use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::concerts::{ConcertRow, TicketAvailability};
use reprise_view::columns::{ColumnKey, ConcertColumn};

use super::concerts_model::ConcertObject;
use super::concerts_presentation::source_name;
use crate::ui::strings;
use crate::ui::table_column_widths as widths;

pub(super) type RadiusSource = Rc<dyn Fn() -> Option<f64>>;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct RowLinkPresentation {
    pub activatable: bool,
    pub tooltip: String,
    pub accessible_description: String,
}

pub(super) fn row_link_presentation(row: &ConcertRow) -> RowLinkPresentation {
    let tooltip = if super::concerts_columns::ticket_target(row).is_some() {
        strings::concerts_opens_source(source_name(row))
    } else {
        strings::text(strings::CONCERTS_NO_LINK)
    };
    RowLinkPresentation {
        activatable: super::concerts_columns::ticket_target(row).is_some(),
        accessible_description: tooltip.clone(),
        tooltip,
    }
}

pub(super) fn apply_row_link_presentation<W>(widget: &W, row: &ConcertRow)
where
    W: IsA<gtk4::Widget> + IsA<gtk4::Accessible>,
{
    let presentation = row_link_presentation(row);
    widget.set_tooltip_text(Some(&presentation.tooltip));
    widget.update_property(&[gtk4::accessible::Property::Description(
        &presentation.accessible_description,
    )]);
}

#[derive(Clone, Copy)]
struct TicketPresentation {
    label: &'static str,
    class: &'static str,
    tooltip: Option<&'static str>,
}

fn ticket_presentation(availability: TicketAvailability) -> TicketPresentation {
    match availability {
        TicketAvailability::OnSale => TicketPresentation {
            label: strings::CONCERTS_ON_SALE,
            class: "on-sale",
            tooltip: None,
        },
        TicketAvailability::OffSale => TicketPresentation {
            label: strings::CONCERTS_OFF_SALE,
            class: "off-sale",
            tooltip: Some(strings::CONCERTS_OFF_SALE_TOOLTIP),
        },
        TicketAvailability::Unknown => TicketPresentation {
            label: strings::CONCERTS_UNKNOWN,
            class: "unknown",
            tooltip: None,
        },
    }
}

pub(super) fn ticket_column(view: &gtk4::ColumnView) {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::builder()
            .xalign(1.0)
            .halign(gtk4::Align::End)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        label.add_css_class("reprise-concert-ticket-tag");
        item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        let row = object.row();
        let presentation = ticket_presentation(row.availability);
        for class in ["on-sale", "off-sale", "unknown"] {
            label.remove_css_class(class);
        }
        label.add_css_class(presentation.class);
        label.set_label(&strings::text(presentation.label));
        let row_link = row_link_presentation(&row);
        let tooltip = presentation
            .tooltip
            .map_or_else(|| row_link.tooltip.clone(), strings::text);
        label.set_tooltip_text(Some(&tooltip));
        label.update_property(&[gtk4::accessible::Property::Description(
            &row_link.accessible_description,
        )]);
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        label.set_label("");
        label.set_tooltip_text(None);
    });
    let column = gtk4::ColumnViewColumn::builder()
        .id(ConcertColumn::Tickets.as_str())
        .title(strings::text(strings::CONCERTS_TICKETS))
        .factory(&factory)
        .resizable(false)
        .build();
    widths::pin(&column, widths::ACTION);
    view.append_column(&column);
}

pub(super) fn source_column(view: &gtk4::ColumnView) {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        let row = object.row();
        label.set_label(source_name(&row));
        apply_row_link_presentation(&label, &row);
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        label.set_label("");
        label.set_tooltip_text(None);
    });
    let column = gtk4::ColumnViewColumn::builder()
        .id(ConcertColumn::Source.as_str())
        .title(strings::text(strings::CONCERTS_SOURCE))
        .factory(&factory)
        .resizable(true)
        .build();
    widths::pin(&column, widths::LABEL);
    view.append_column(&column);
}

#[must_use]
pub(super) fn distance_class(distance_km: Option<f64>, radius_km: Option<f64>) -> &'static str {
    if distance_km
        .zip(radius_km)
        .is_some_and(|(distance, radius)| distance <= radius)
    {
        "reprise-concert-distance-near"
    } else {
        "reprise-concert-distance-far"
    }
}

pub(super) fn distance_column(
    view: &gtk4::ColumnView,
    radius_source: &RadiusSource,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::builder()
            .xalign(1.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        label.add_css_class("numeric");
        item.set_child(Some(&label));
    });
    let radius_source = radius_source.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        let row = object.row();
        label.set_label(&super::concerts_presentation::format_distance_km(
            row.distance_km,
        ));
        for class in [
            "reprise-concert-distance-near",
            "reprise-concert-distance-far",
        ] {
            label.remove_css_class(class);
        }
        label.add_css_class(distance_class(row.distance_km, radius_source()));
        apply_row_link_presentation(&label, &row);
    });
    let column = gtk4::ColumnViewColumn::builder()
        .id(ConcertColumn::Distance.as_str())
        .title(strings::text(strings::CONCERTS_DISTANCE))
        .factory(&factory)
        .resizable(true)
        .build();
    widths::pin(&column, widths::NUMERIC);
    column.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
    view.append_column(&column);
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_status_copy_is_source_faithful() {
        let on_sale = ticket_presentation(TicketAvailability::OnSale);
        assert_eq!(on_sale.label, "On sale");
        assert_eq!(on_sale.class, "on-sale");
        assert_eq!(on_sale.tooltip, None);

        let off_sale = ticket_presentation(TicketAvailability::OffSale);
        assert_eq!(off_sale.label, "Off sale");
        assert_eq!(off_sale.class, "off-sale");
        assert_eq!(
            off_sale.tooltip,
            Some(
                "The ticket source reports no active sale. This can mean sold out, or not on sale yet."
            )
        );

        let unknown = ticket_presentation(TicketAvailability::Unknown);
        assert_eq!(unknown.label, "Unknown");
        assert_eq!(unknown.class, "unknown");
    }
}
