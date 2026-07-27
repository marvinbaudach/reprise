//! Ephemeral narrow-window column folding.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::column_layout::{ColumnId, ColumnLayout, ColumnRegistry};

const ALWAYS_REACHABLE: [ColumnId; 3] = [ColumnId::Cover, ColumnId::Title, ColumnId::Artist];
const FOLD_BREAKPOINT_WIDTH: i32 = 760;
const SHOW_FOLDED_ACTION: &str = "show-folded-columns";

type OnFolded = Rc<dyn Fn()>;

fn secondary_columns(layout: &ColumnLayout) -> Vec<ColumnId> {
    layout
        .order
        .iter()
        .copied()
        .filter(|id| !ALWAYS_REACHABLE.contains(id))
        .collect()
}

pub(super) struct ResponsiveColumns {
    host: adw::BreakpointBin,
    breakpoint: adw::Breakpoint,
    folded: Rc<Cell<bool>>,
    expanded_by_user: Cell<bool>,
    on_folded: Rc<RefCell<Option<OnFolded>>>,
}

impl ResponsiveColumns {
    pub(super) fn new(
        host: &adw::BreakpointBin,
        registry: &ColumnRegistry,
        layout: &ColumnLayout,
    ) -> Rc<Self> {
        let condition = adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            f64::from(FOLD_BREAKPOINT_WIDTH),
            adw::LengthUnit::Px,
        );
        let breakpoint = adw::Breakpoint::new(condition);
        // Install setters for every secondary column, including columns that
        // are hidden right now. BreakpointBin captures each live property
        // value when applying and restores it when removed, so preferences
        // changed after startup remain authoritative.
        for id in secondary_columns(layout) {
            if let Some(column) = registry.column(id) {
                breakpoint.add_setter(column, "visible", Some(&false.to_value()));
            }
        }

        let folded = Rc::new(Cell::new(false));
        let on_folded: Rc<RefCell<Option<OnFolded>>> = Rc::new(RefCell::new(None));
        {
            let folded = folded.clone();
            let on_folded = on_folded.clone();
            breakpoint.connect_apply(move |_| {
                folded.set(true);
                let callback = on_folded.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        {
            let folded = folded.clone();
            breakpoint.connect_unapply(move |_| folded.set(false));
        }

        host.add_breakpoint(breakpoint.clone());

        Rc::new(Self {
            host: host.clone(),
            breakpoint,
            folded,
            expanded_by_user: Cell::new(false),
            on_folded,
        })
    }

    pub(super) fn install_notice(
        self: &Rc<Self>,
        window: &adw::ApplicationWindow,
        overlay: &adw::ToastOverlay,
    ) {
        let action = gio::SimpleAction::new(SHOW_FOLDED_ACTION, None);
        {
            let this = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(this) = this.upgrade() {
                    this.show_all();
                }
            });
        }
        window.add_action(&action);

        let overlay = overlay.downgrade();
        let show_notice: OnFolded = Rc::new(move || {
            let Some(overlay) = overlay.upgrade() else {
                return;
            };
            let toast = adw::Toast::new(&crate::ui::strings::text(
                crate::ui::strings::COLUMNS_FOLDED,
            ));
            toast.set_button_label(Some(&crate::ui::strings::text(
                crate::ui::strings::SHOW_COLUMNS,
            )));
            toast.set_action_name(Some(&format!("win.{SHOW_FOLDED_ACTION}")));
            overlay.add_toast(toast);
        });
        *self.on_folded.borrow_mut() = Some(show_notice.clone());
        if self.folded.get() && !self.expanded_by_user.get() {
            show_notice();
        }
    }

    fn show_all(&self) {
        if self.expanded_by_user.replace(true) {
            return;
        }
        // Removing an applied breakpoint restores the live values captured at
        // apply time; do not overwrite them with the startup layout.
        self.host.remove_breakpoint(&self.breakpoint);
        self.folded.set(false);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn style_6_narrow_folding_is_ephemeral_and_preserves_primary_columns() {
        let layout = ColumnLayout {
            order: vec![
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Album,
                ColumnId::Artist,
                ColumnId::Year,
                ColumnId::Duration,
            ],
            visible: HashSet::from([
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Album,
                ColumnId::Artist,
                ColumnId::Year,
            ]),
        };
        let serialized_before = super::super::column_layout::serialize_layout(&layout);

        assert_eq!(
            secondary_columns(&layout),
            vec![ColumnId::Album, ColumnId::Year, ColumnId::Duration]
        );
        assert_eq!(
            super::super::column_layout::serialize_layout(&layout),
            serialized_before,
            "responsive folding must not rewrite the persisted layout"
        );
    }
}
