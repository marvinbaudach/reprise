//! Live artwork permission shared by the My Stats artist-portrait surfaces.

use std::cell::Cell;
use std::rc::Rc;

use reprise_core::db::Db;

pub(in crate::ui) struct ArtistPortraitRuntime {
    pub enabled: Rc<Cell<bool>>,
}

impl ArtistPortraitRuntime {
    pub(in crate::ui) fn setup(conn: &Db) -> Rc<Self> {
        Rc::new(Self {
            enabled: Rc::new(Cell::new(
                reprise_core::online_sources::network_allowed_or_off(
                    conn,
                    &reprise_core::modules::ARTWORK_MODULE,
                ),
            )),
        })
    }

    /// `NET-1a`: re-derives `enabled` from the global online-sources gate.
    pub(in crate::ui) fn recompute_enabled(&self, conn: &Db) {
        self.enabled
            .set(reprise_core::online_sources::network_allowed_or_off(
                conn,
                &reprise_core::modules::ARTWORK_MODULE,
            ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_recomputes_the_live_artwork_setting() {
        let conn = crate::test_db::open().unwrap();
        let runtime = ArtistPortraitRuntime::setup(&conn);
        assert!(!runtime.enabled.get());

        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
            .unwrap();
        runtime.recompute_enabled(&conn);

        assert!(runtime.enabled.get());
        assert!(
            reprise_core::modules::is_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE)
                .unwrap()
        );
    }

    #[test]
    fn net_1a_recompute_enabled_reflects_the_global_gate() {
        let conn = crate::test_db::open().unwrap();
        let runtime = ArtistPortraitRuntime::setup(&conn);
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
            .unwrap();
        runtime.recompute_enabled(&conn);
        assert!(runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, false).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(!runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(runtime.enabled.get());
    }
}
