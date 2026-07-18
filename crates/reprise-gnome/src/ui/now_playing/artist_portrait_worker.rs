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
    worker: async_channel::Sender<ArtistPortraitRequest>,
}

impl ArtistPortraitRuntime {
    pub(in crate::ui) fn setup() -> Rc<Self> {
        Rc::new(Self { worker: spawn() })
    }

    pub(in crate::ui) fn request(&self, request: ArtistPortraitRequest) {
        if request.artist.trim().is_empty() {
            return;
        }
        if let Err(error) = self.worker.try_send(request) {
            tracing::warn!(%error, "could not queue Artist Portrait request");
        }
    }
}

fn spawn() -> async_channel::Sender<ArtistPortraitRequest> {
    let (sender, receiver) = async_channel::unbounded::<ArtistPortraitRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-artist-portrait".into())
        .spawn(move || {
            while let Ok(request) = receiver.recv_blocking() {
                let result = reprise_core::artist_portrait::load_or_fetch(&request.artist);
                let _ = request.response.try_send(ArtistPortraitResponse {
                    generation: request.generation,
                    artist: request.artist,
                    result,
                });
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start Artist Portrait worker");
    }
    sender
}
