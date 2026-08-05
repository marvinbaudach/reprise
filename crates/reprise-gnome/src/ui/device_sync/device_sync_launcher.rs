//! Callback type for opening a connected Android synchronization page.

use std::rc::Rc;

pub(in crate::ui) type OpenDevice = Rc<dyn Fn(String, String)>;
