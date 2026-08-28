//! What the column editor needs to know about a table — and nothing more.
//!
//! The editor is one widget tree serving four tables whose column identities
//! are four different Rust types. Making the widget code generic over them
//! would monomorphise every closure, gesture and drag payload four times for
//! no benefit, so the type disappears at this boundary: the editor sees ids
//! and labels, and the per-table adapter behind this trait turns an id back
//! into its typed key.

pub(in crate::ui) struct ColumnDescriptor {
    pub id: String,
    pub label: String,
}

pub(in crate::ui) trait EditorModel: 'static {
    fn title(&self) -> String;

    /// The editable columns, in their current order. Pinned columns are never
    /// listed — they cannot be moved or hidden, so a row for them would be a
    /// row that does nothing.
    fn columns(&self) -> Vec<ColumnDescriptor>;

    fn sortable_columns(&self) -> Vec<ColumnDescriptor> {
        Vec::new()
    }

    fn sort(&self) -> Option<(String, gtk4::SortType)> {
        None
    }

    fn set_sort(&self, _id: &str, _order: gtk4::SortType) {}

    fn is_visible(&self, id: &str) -> bool;
    fn set_visible(&self, id: &str, visible: bool);
    fn move_column(&self, id: &str, target: &str, after: bool);
    fn reset(&self);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        hidden: RefCell<Vec<String>>,
    }

    impl EditorModel for Fake {
        fn title(&self) -> String {
            "Edit column layout".to_owned()
        }

        fn columns(&self) -> Vec<ColumnDescriptor> {
            vec![ColumnDescriptor {
                id: "date".to_owned(),
                label: "Date".to_owned(),
            }]
        }

        fn is_visible(&self, id: &str) -> bool {
            !self.hidden.borrow().iter().any(|hidden| hidden == id)
        }

        fn set_visible(&self, id: &str, visible: bool) {
            if visible {
                self.hidden.borrow_mut().retain(|hidden| hidden != id);
            } else {
                self.hidden.borrow_mut().push(id.to_owned());
            }
        }

        fn move_column(&self, _id: &str, _target: &str, _after: bool) {}

        fn reset(&self) {
            self.hidden.borrow_mut().clear();
        }
    }

    #[test]
    fn an_editor_model_reports_and_flips_visibility() {
        let model = Fake {
            hidden: RefCell::new(Vec::new()),
        };
        assert!(model.is_visible("date"));
        model.set_visible("date", false);
        assert!(!model.is_visible("date"));
        model.reset();
        assert!(model.is_visible("date"));
    }

    #[test]
    fn an_editor_model_without_sorting_uses_inert_defaults() {
        let model = Fake {
            hidden: RefCell::new(Vec::new()),
        };

        assert!(model.sortable_columns().is_empty());
        assert_eq!(model.sort(), None);
        model.set_sort("date", gtk4::SortType::Descending);
        assert_eq!(model.sort(), None);
    }
}
