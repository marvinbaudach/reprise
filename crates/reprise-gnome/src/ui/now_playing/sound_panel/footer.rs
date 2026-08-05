use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

type IdsCallback = Rc<dyn Fn(&[i64])>;

pub(super) struct Footer {
    root: gtk4::Box,
    add: gtk4::Button,
    ids: Rc<RefCell<Vec<i64>>>,
    on_add: Rc<RefCell<Option<IdsCallback>>>,
}

impl Footer {
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        root.add_css_class("reprise-sound-footer");
        let add = gtk4::Button::with_label(&crate::ui::strings::text(
            crate::ui::strings::SOUND_ADD_TO_QUEUE,
        ));
        add.add_css_class("suggested-action");
        add.set_hexpand(true);
        let more = gtk4::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .tooltip_text(crate::ui::strings::text(
                crate::ui::strings::SOUND_MORE_ACTIONS,
            ))
            .build();
        root.append(&add);
        root.append(&more);
        let ids = Rc::new(RefCell::new(Vec::new()));
        let on_add: Rc<RefCell<Option<IdsCallback>>> = Rc::new(RefCell::new(None));
        add.connect_clicked({
            let ids = ids.clone();
            let on_add = on_add.clone();
            move |_| {
                let ids = ids.borrow().clone();
                let callback = on_add.borrow().clone();
                if let Some(callback) = callback {
                    callback(&ids);
                }
            }
        });
        Self {
            root,
            add,
            ids,
            on_add,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn set_ids(&self, ids: Vec<i64>) {
        self.add.set_sensitive(!ids.is_empty());
        *self.ids.borrow_mut() = ids;
    }

    pub(super) fn set_on_add(&self, callback: impl Fn(&[i64]) + 'static) {
        *self.on_add.borrow_mut() = Some(Rc::new(callback));
    }
}
