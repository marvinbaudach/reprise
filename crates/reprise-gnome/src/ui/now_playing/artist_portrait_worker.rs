use std::cell::Cell;
use std::rc::Rc;

use reprise_core::artist_portrait::{PortraitError, PortraitOutcome};

pub(in crate::ui) struct ArtistPortraitRequest {
    pub generation: u64,
    pub artist: String,
    pub response: async_channel::Sender<ArtistPortraitResponse>,
}

#[derive(Debug)]
pub(in crate::ui) struct ArtistPortraitResponse {
    pub generation: u64,
    pub artist: String,
    pub result: Result<PortraitOutcome, PortraitError>,
}

pub(in crate::ui) struct ArtistPortraitRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker: async_channel::Sender<WorkerRequest>,
}

struct WorkerRequest {
    request: ArtistPortraitRequest,
    allow_network: bool,
}

impl ArtistPortraitRuntime {
    pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> Rc<Self> {
        let enabled = reprise_core::modules::is_enabled(
            conn,
            &reprise_core::modules::ARTIST_PORTRAITS_MODULE,
        )
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read Artist Portrait module state; defaulting to off");
            false
        });
        Rc::new(Self {
            enabled: Rc::new(Cell::new(enabled)),
            worker: spawn(),
        })
    }

    #[cfg(test)]
    pub(in crate::ui) fn setup_for_test() -> Rc<Self> {
        Rc::new(Self {
            enabled: Rc::new(Cell::new(false)),
            worker: spawn(),
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
        self.enabled.set(enabled);
        Ok(())
    }

    pub(in crate::ui) fn request(&self, request: ArtistPortraitRequest) {
        if request.artist.trim().is_empty() {
            return;
        }
        let request = WorkerRequest {
            request,
            allow_network: self.enabled.get(),
        };
        if let Err(error) = self.worker.try_send(request) {
            tracing::warn!(%error, "could not queue Artist Portrait request");
        }
    }
}

fn spawn() -> async_channel::Sender<WorkerRequest> {
    let (sender, receiver) = async_channel::unbounded::<WorkerRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-artist-portrait".into())
        .spawn(move || {
            while let Ok(worker_request) = receiver.recv_blocking() {
                let WorkerRequest {
                    request,
                    allow_network,
                } = worker_request;
                let ArtistPortraitRequest {
                    generation,
                    artist,
                    response,
                } = request;
                let result = if allow_network {
                    reprise_core::artist_portrait::load_or_fetch(&artist)
                } else {
                    Ok(reprise_core::artist_portrait::load_cached(&artist))
                };
                let _ = response.try_send(ArtistPortraitResponse {
                    generation,
                    artist,
                    result,
                });
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start Artist Portrait worker");
    }
    sender
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn disabled_runtime_dispatches_cache_only_lookup() {
        let (worker, receiver) = async_channel::unbounded();
        let runtime = ArtistPortraitRuntime {
            enabled: Rc::new(Cell::new(false)),
            worker,
        };
        let (response, _result) = async_channel::bounded(1);

        runtime.request(ArtistPortraitRequest {
            generation: 1,
            artist: "Band".into(),
            response,
        });

        assert!(!receiver.try_recv().unwrap().allow_network);
    }

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
}
