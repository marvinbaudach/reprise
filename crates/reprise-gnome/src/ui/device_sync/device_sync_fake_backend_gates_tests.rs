use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct CopyGate {
    pub(super) started: async_channel::Sender<String>,
    pub(super) releases: HashMap<String, async_channel::Receiver<()>>,
}

#[derive(Clone)]
pub(super) struct PlaylistGate {
    pub(super) started: async_channel::Sender<()>,
    pub(super) release: async_channel::Receiver<()>,
}

#[derive(Clone)]
pub(super) struct InspectionGate {
    pub(super) started: async_channel::Sender<()>,
    pub(super) release: async_channel::Receiver<()>,
}
