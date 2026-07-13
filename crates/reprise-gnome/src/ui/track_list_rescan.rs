//! Public reuse seam for the already-wired safe library rescan action.

use std::rc::Rc;

use super::track_list::TrackList;

fn invoke_rescan(callback: Option<Rc<dyn Fn()>>) -> bool {
    let Some(callback) = callback else {
        return false;
    };
    callback();
    true
}

impl TrackList {
    pub(super) fn rescan_library(&self) {
        let callback = self.shared.on_rescan_library.borrow().clone();
        if !invoke_rescan(callback) {
            tracing::warn!("library rescan requested before its callback was wired");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn invoking_a_wired_rescan_calls_it_exactly_once() {
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let callback: Rc<dyn Fn()> = Rc::new(move || observed.set(observed.get() + 1));

        assert!(invoke_rescan(Some(callback)));
        assert_eq!(calls.get(), 1);
        assert!(!invoke_rescan(None));
    }
}
