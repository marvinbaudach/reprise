//! Runtime state for the optional artist-portrait module.
//!
//! The former Artists grid owned portrait requests. With Album and Artist now
//! represented by the canonical TrackList, the frontend only needs the live
//! module setting until another visible portrait surface is introduced.

use std::cell::Cell;
use std::rc::Rc;

pub(in crate::ui) struct ArtistPortraitRuntime {
    pub enabled: Rc<Cell<bool>>,
}

impl ArtistPortraitRuntime {
    pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> Rc<Self> {
        Rc::new(Self {
            enabled: Rc::new(Cell::new(
                reprise_core::online_sources::network_allowed_or_off(
                    conn,
                    &reprise_core::modules::ARTIST_PORTRAITS_MODULE,
                ),
            )),
        })
    }

    pub(in crate::ui) fn set_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(
            conn,
            &reprise_core::modules::ARTIST_PORTRAITS_MODULE,
            enabled,
        )?;
        self.enabled
            .set(reprise_core::online_sources::network_allowed_or_off(
                conn,
                &reprise_core::modules::ARTIST_PORTRAITS_MODULE,
            ));
        Ok(())
    }

    /// `NET-1a`: re-derives `enabled` from the global online-sources gate.
    pub(in crate::ui) fn recompute_enabled(&self, conn: &rusqlite::Connection) {
        self.enabled
            .set(reprise_core::online_sources::network_allowed_or_off(
                conn,
                &reprise_core::modules::ARTIST_PORTRAITS_MODULE,
            ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_reads_and_updates_the_live_module_setting() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = ArtistPortraitRuntime::setup(&conn);
        assert!(!runtime.enabled.get());

        runtime.set_enabled(&conn, true).unwrap();

        assert!(runtime.enabled.get());
        assert!(reprise_core::modules::is_enabled(
            &conn,
            &reprise_core::modules::ARTIST_PORTRAITS_MODULE
        )
        .unwrap());
    }

    #[test]
    fn net_1a_recompute_enabled_reflects_the_global_gate() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = ArtistPortraitRuntime::setup(&conn);
        runtime.set_enabled(&conn, true).unwrap();
        assert!(runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, false).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(!runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(runtime.enabled.get());
    }
}
