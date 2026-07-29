//! Connected clients and the fan-out of events to them.
//!
//! Clients are stateless towards the runtime (§9.5): the runtime is the
//! truth, a client holds a mirror it replaces wholesale whenever it
//! (re)connects. Consequently a client's mailbox is *not* durable. It exists
//! only while the client is connected, it is bounded, and dropping it on
//! disconnect is the mechanism that makes "no replay after a reconnect" true
//! rather than merely intended.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use reprise_core::playback::SpectrumFrame;

use crate::error::Capability;
use crate::event::{Delivery, RuntimeEvent, SequencedEvent};

/// How many undelivered events one client may fall behind by.
///
/// A client that stops draining must not grow the runtime's memory without
/// bound, and it must not be served stale-but-plausible deltas either. Past
/// this many, the oldest are dropped and the client is told to take a fresh
/// snapshot — a refresh, which is always correct, instead of a partial
/// replay, which is not.
const MAILBOX_CAPACITY: usize = 256;

/// A connected client's handle. Opaque and non-reusable: a reconnect gets a
/// new one, so a command carrying an old id fails as `NotConnected` rather
/// than silently binding to a different session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(u64);

/// The id as the plain number a snapshot or a signal carries. Clients
/// compare it against the id they were handed at connect; nothing outside
/// the runtime is expected to interpret it further.
impl From<ClientId> for u64 {
    fn from(id: ClientId) -> Self {
        id.0
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "client-{}", self.0)
    }
}

/// What a client announces when it connects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHandshake {
    /// The protocol version the client was built against. A foreign major
    /// version is refused rather than served a payload it cannot read.
    pub protocol: reprise_runtime_protocol::ProtocolVersion,
    /// The capabilities this client holds. Who grants them is the client's
    /// own business — the GTK window holds all of them, `reprise-mcp` reads
    /// its persisted `agent.capability.*` settings — but which ones a
    /// command needs is the runtime's.
    pub capabilities: BTreeSet<Capability>,
}

impl ClientHandshake {
    /// A handshake for the current protocol version holding `capabilities`.
    #[must_use]
    pub fn new(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            protocol: reprise_runtime_protocol::PROTOCOL_VERSION,
            capabilities: capabilities.into_iter().collect(),
        }
    }
}

struct Client {
    capabilities: BTreeSet<Capability>,
    mailbox: VecDeque<SequencedEvent>,
    overflowed: bool,
    /// The newest spectrum frame this client has not taken yet.
    ///
    /// One slot rather than a queue, and deliberately not in `mailbox`. A
    /// frame arrives ~60 times a second: a 256-entry mailbox holds four
    /// seconds of them, after which a client that blinked is told to
    /// resynchronize — over data that was already worthless by then. A
    /// spectrum frame is not state to catch up on, it is a picture of this
    /// instant, so a newer one simply replaces an untaken older one. That is
    /// `SpectrumFrame::coalesce_latest`'s contract, held here.
    spectrum: Option<SpectrumFrame>,
    /// Whether this client asked to be offered frames at all. Off by
    /// default: most clients never draw a visualizer, and analysing audio
    /// for nobody is CPU spent on the hot path for no reason.
    watches_spectrum: bool,
}

/// The connected set, plus the runtime's single event sequence.
pub(crate) struct Clients {
    next_id: u64,
    connected: BTreeMap<ClientId, Client>,
    sequence: u64,
}

impl Clients {
    pub(crate) fn new() -> Self {
        Self {
            next_id: 1,
            connected: BTreeMap::new(),
            sequence: 0,
        }
    }

    /// The sequence of the most recent event, which is also the sequence a
    /// snapshot taken right now describes.
    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) fn connect(&mut self, capabilities: BTreeSet<Capability>) -> ClientId {
        let id = ClientId(self.next_id);
        self.next_id += 1;
        self.connected.insert(
            id,
            Client {
                capabilities,
                mailbox: VecDeque::new(),
                overflowed: false,
                spectrum: None,
                watches_spectrum: false,
            },
        );
        id
    }

    /// Drops the client and its mailbox. Returns whether it was connected,
    /// so a duplicate disconnect stays a no-op instead of an error.
    pub(crate) fn disconnect(&mut self, client: ClientId) -> bool {
        self.connected.remove(&client).is_some()
    }

    pub(crate) fn is_connected(&self, client: ClientId) -> bool {
        self.connected.contains_key(&client)
    }

    pub(crate) fn count(&self) -> usize {
        self.connected.len()
    }

    pub(crate) fn holds(&self, client: ClientId, capability: Capability) -> bool {
        self.connected
            .get(&client)
            .is_some_and(|client| client.capabilities.contains(&capability))
    }

    /// Appends one event to every connected client's mailbox under a fresh
    /// sequence number. One number for all recipients: two clients that
    /// compare notes must agree on the order.
    ///
    /// `initiator` is the client whose command provoked this, or `None` when
    /// nothing a client asked for did — see [`SequencedEvent::initiator`].
    pub(crate) fn publish(&mut self, initiator: Option<ClientId>, event: RuntimeEvent) {
        self.sequence += 1;
        let sequenced = SequencedEvent {
            sequence: self.sequence,
            initiator,
            event,
        };
        for client in self.connected.values_mut() {
            if client.mailbox.len() == MAILBOX_CAPACITY {
                client.mailbox.pop_front();
                client.overflowed = true;
            }
            client.mailbox.push_back(sequenced.clone());
        }
    }

    /// Offers a frame to every client that asked to see one.
    ///
    /// Not `publish`: this carries no sequence, enters no mailbox, and can
    /// never provoke a resynchronize. A frame that a client did not take
    /// before the next one arrived is simply gone, which is the only
    /// treatment that makes sense for a picture of a moment that has passed.
    pub(crate) fn offer_spectrum(&mut self, frame: SpectrumFrame) {
        for client in self.connected.values_mut() {
            if client.watches_spectrum {
                client.spectrum = Some(frame);
            }
        }
    }

    /// Whether anybody is watching. The backend's analysis costs CPU on the
    /// audio path, so it runs only while someone is actually looking.
    pub(crate) fn anyone_watches_spectrum(&self) -> bool {
        self.connected
            .values()
            .any(|client| client.watches_spectrum)
    }

    /// Starts or stops one client's interest. Returns whether the total
    /// changed, which is what decides if the backend is told.
    pub(crate) fn watch_spectrum(&mut self, client: ClientId, watching: bool) -> Option<bool> {
        let before = self.anyone_watches_spectrum();
        let entry = self.connected.get_mut(&client)?;
        entry.watches_spectrum = watching;
        if !watching {
            // Nothing to hand over to a client that stopped looking; keeping
            // it would deliver one frame from before they turned it off.
            entry.spectrum = None;
        }
        Some(before != self.anyone_watches_spectrum())
    }

    /// Takes the newest frame this client has not seen, if any.
    pub(crate) fn take_spectrum(&mut self, client: ClientId) -> Option<SpectrumFrame> {
        self.connected.get_mut(&client)?.spectrum.take()
    }

    /// Hands a client everything queued for it and clears the queue.
    pub(crate) fn drain(&mut self, client: ClientId) -> Option<Delivery> {
        let entry = self.connected.get_mut(&client)?;
        Some(Delivery {
            events: entry.mailbox.drain(..).collect(),
            resynchronize: std::mem::replace(&mut entry.overflowed, false),
        })
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod client_tests;
