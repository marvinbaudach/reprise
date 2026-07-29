//! The runtime, driven over a real session bus.
//!
//! These are the only tests in the tree that put a client, a bus and the
//! serving loop together. Everything below them is covered elsewhere — the
//! reducer in `reprise-runtime`, the lease and the lifecycle in their own
//! unit tests — so what is proved here is exactly the wiring: a handshake
//! reaches the runtime, an error keeps its category on the way back, two
//! peers see their own events, and a runtime nobody is using goes away.
//!
//! They need a session bus and are therefore `#[ignore]`d; the merge gate
//! runs them under `dbus-run-session`.

use std::time::{Duration, Instant};

use reprise_platform_linux::runtime_service::{
    RuntimeLease, RuntimeService, ServeOptions, ServiceInbox, OBJECT_PATH,
};
use reprise_runtime::fakes::{FakeClock, FakeDevices, FakeLibrary, FakePlayback};
use reprise_runtime::{Ports, Runtime};

const INTERFACE: &str = "org.reprise.Reprise1";
const CAPABILITIES: [&str; 1] = ["playback:control"];

/// A service running on a name nobody else uses, so a test never fights the
/// developer's own runtime for the session identity.
struct Served {
    bus_name: String,
    thread: Option<std::thread::JoinHandle<()>>,
    _lease: tempfile::TempDir,
}

impl Served {
    fn start(label: &str, grace: Duration) -> Self {
        Self::start_on(Self::name_for(label), grace)
    }

    /// The name a given label will be served on, so a test can point a
    /// client at it *before* anything answers there.
    fn name_for(label: &str) -> String {
        format!("org.reprise.Reprise1.test{}{label}", std::process::id())
    }

    fn start_on(bus_name: String, grace: Duration) -> Self {
        let lease_dir = tempfile::tempdir().expect("a temporary runtime directory");
        let lease_path = lease_dir.path().join("runtime.lock");
        let options = ServeOptions {
            bus_name: bus_name.clone(),
            grace,
            // Fast enough that a test does not wait on the idle timer, and
            // still a real tick rather than a special case in the loop.
            tick: Duration::from_millis(25),
        };

        let thread = std::thread::spawn(move || {
            // The runtime is deliberately not `Send`: it is built here, on
            // the thread that will own it for its whole life.
            let inbox = ServiceInbox::new();
            let ports = Ports {
                playback: Box::new(FakePlayback::new()),
                library: Box::new(FakeLibrary::with_tracks([1, 2, 3])),
                devices: Box::new(FakeDevices::new()),
                clock: Box::new(FakeClock::starting_at(1_753_600_000)),
            };
            let conn = reprise_core::db::open_migrated(None).expect("an in-memory database");
            let lease = RuntimeLease::claim_at(&lease_path).expect("the test owns its own lease");
            RuntimeService::serve(Runtime::new(conn, ports), lease, &options, inbox, None)
                .expect("the service starts");
        });

        let served = Self {
            bus_name,
            thread: Some(thread),
            _lease: lease_dir,
        };
        served.await_name();
        served
    }

    /// Waits until the well-known name is owned, which is what `Type=dbus`
    /// makes systemd wait for too.
    fn await_name(&self) {
        let connection = zbus::blocking::Connection::session().expect("a session bus");
        let proxy = zbus::blocking::fdo::DBusProxy::new(&connection).expect("the bus daemon");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if proxy
                .name_has_owner(self.bus_name.as_str().try_into().expect("a valid bus name"))
                .unwrap_or(false)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the runtime never claimed {}", self.bus_name);
    }

    fn client(&self) -> Client {
        let connection = zbus::blocking::Connection::session().expect("a session bus");
        let proxy = zbus::blocking::Proxy::new_owned(
            connection.clone(),
            self.bus_name.clone(),
            OBJECT_PATH.to_owned(),
            INTERFACE.to_owned(),
        )
        .expect("the interface is reachable");
        Client {
            _connection: connection,
            proxy,
        }
    }

    fn wait_for_shutdown(&mut self) {
        let thread = self.thread.take().expect("the service was started");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !thread.is_finished() {
            assert!(
                Instant::now() < deadline,
                "the idle runtime never shut down"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        thread.join().expect("the service thread ended cleanly");
    }
}

struct Client {
    _connection: zbus::blocking::Connection,
    proxy: zbus::blocking::Proxy<'static>,
}

impl Client {
    fn connect(&self) -> zbus::Result<reprise_runtime_protocol::runtime::RuntimeSnapshot> {
        self.connect_as(
            reprise_runtime_protocol::PROTOCOL_VERSION.major,
            reprise_runtime_protocol::PROTOCOL_VERSION.minor,
        )
    }

    fn connect_as(
        &self,
        major: u32,
        minor: u32,
    ) -> zbus::Result<reprise_runtime_protocol::runtime::RuntimeSnapshot> {
        self.proxy.call(
            "Connect",
            &(major, minor, CAPABILITIES.map(ToOwned::to_owned).to_vec()),
        )
    }

    fn call(
        &self,
        method: &str,
        body: &(impl serde::ser::Serialize + zvariant::DynamicType),
    ) -> zbus::Result<()> {
        self.proxy.call(method, body)
    }
}

/// The error name a failed call came back with, minus the interface prefix.
fn error_kind(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(name, message, _) => {
            format!("{name}|{}", message.clone().unwrap_or_default())
        }
        other => format!("unexpected: {other}"),
    }
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_handshake_returns_the_whole_runtime_state() {
    let served = Served::start("handshake", Duration::from_secs(60));
    let client = served.client();

    let snapshot = client.connect().expect("the handshake succeeds");

    assert_eq!(
        snapshot.protocol_major,
        reprise_runtime_protocol::PROTOCOL_VERSION.major
    );
    assert_eq!(snapshot.playback.status, "stopped");
    assert_eq!(snapshot.sequence, 0);
    assert!(snapshot.device_runs.is_empty());
    assert!(snapshot.jobs.is_empty());
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_foreign_protocol_major_comes_back_as_refused() {
    let served = Served::start("major", Duration::from_secs(60));
    let client = served.client();

    // Derived from what this runtime speaks, not hardcoded: a major bump
    // otherwise turns "foreign" into "our own version", and the test goes on
    // passing a handshake it exists to refuse.
    let error = client
        .connect_as(reprise_runtime_protocol::PROTOCOL_VERSION.major + 1, 0)
        .expect_err("a foreign major cannot decode what this runtime sends");

    let kind = error_kind(&error);
    assert!(
        kind.contains("org.reprise.Reprise1.Error.Refused"),
        "the category survives the bus: {kind}"
    );
    assert!(
        kind.contains("refused:protocol_major"),
        "and so does the diagnostic kind: {kind}"
    );
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_command_before_the_handshake_is_unavailable_rather_than_obeyed() {
    let served = Served::start("nohandshake", Duration::from_secs(60));
    let client = served.client();

    let error = client
        .call("Play", &())
        .expect_err("a peer that never connected has no session");

    let kind = error_kind(&error);
    assert!(
        kind.contains("org.reprise.Reprise1.Error.Unavailable"),
        "{kind}"
    );
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_rejected_command_keeps_its_category_and_its_reason() {
    let served = Served::start("rejected", Duration::from_secs(60));
    let client = served.client();
    client.connect().expect("the handshake succeeds");

    let error = client
        .call("SetRepeat", &("sometimes",))
        .expect_err("that is not a repeat mode");

    let kind = error_kind(&error);
    assert!(
        kind.contains("org.reprise.Reprise1.Error.Rejected"),
        "{kind}"
    );
    assert!(kind.contains("rejected:unknown_repeat_mode"), "{kind}");
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn playing_moves_the_snapshot_every_client_reads() {
    let served = Served::start("play", Duration::from_secs(60));
    let watcher = served.client();
    let actor = served.client();
    watcher.connect().expect("the watcher connects");
    actor.connect().expect("the actor connects");

    actor
        .call("PlayTracks", &(vec![1_i64, 2, 3], 0_u64))
        .expect("playing three known tracks succeeds");

    let seen: reprise_runtime_protocol::runtime::RuntimeSnapshot = watcher
        .proxy
        .call("Snapshot", &())
        .expect("the watcher can resynchronize");
    assert_eq!(seen.playback.status, "playing");
    assert_eq!(seen.playback.track_id, Some(1));
    assert!(
        seen.sequence > 0,
        "the snapshot names the point in the event order it describes"
    );
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_client_receives_the_deltas_for_its_own_session() {
    let served = Served::start("signals", Duration::from_secs(60));
    let client = served.client();
    // Subscribing before connecting, so nothing that follows the snapshot can
    // slip through between the two calls.
    let mut changes = zbus::blocking::MessageIterator::for_match_rule(
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(INTERFACE)
            .expect("a valid interface")
            .member("PlaybackChanged")
            .expect("a valid member")
            .build(),
        &client._connection,
        Some(8),
    )
    .expect("the match rule is accepted");

    let handshake = client.connect().expect("the handshake succeeds");
    client
        .call("PlayTracks", &(vec![1_i64], 0_u64))
        .expect("playing succeeds");

    let message = changes
        .next()
        .expect("a delta arrives")
        .expect("and it is a message");
    let (sequence, initiator, snapshot): (
        u64,
        u64,
        reprise_runtime_protocol::playback::PlaybackSnapshot,
    ) = message.body().deserialize().expect("the delta decodes");
    assert!(sequence > 0);
    assert_eq!(snapshot.status, "playing");
    assert_eq!(snapshot.track_id, Some(1));
    assert_eq!(
        initiator, handshake.client_id,
        "a surface has to recognise its own change to follow somebody else's \
         quietly (RUN-5), and comparing snapshots cannot tell it apart from \
         an identical command another client sent"
    );
    assert_eq!(
        snapshot.initiated_by,
        Some(handshake.client_id),
        "and the session's owner has to survive on the facet, because the \
         quit policy stops only playback this surface started"
    );
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_runtime_nobody_is_using_shuts_itself_down() {
    // Zero grace: the rule under test is *whether* it shuts down and after
    // what, not how long two minutes is.
    let mut served = Served::start("idle", Duration::ZERO);
    {
        let client = served.client();
        client.connect().expect("the handshake succeeds");
        client
            .call("Disconnect", &())
            .expect("saying goodbye works");
    }

    served.wait_for_shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_client_that_vanishes_stops_holding_the_runtime_awake() {
    let mut served = Served::start("vanish", Duration::ZERO);
    {
        // No `Disconnect`: this peer simply stops existing, which is what a
        // crashed client does. The bus tells the runtime; nothing else can.
        let client = served.client();
        client.connect().expect("the handshake succeeds");
    }

    served.wait_for_shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn the_protocol_version_is_readable_without_connecting() {
    let served = Served::start("version", Duration::from_secs(60));
    let client = served.client();

    let (major, minor): (u32, u32) = client
        .proxy
        .get_property("ProtocolVersion")
        .expect("the property is readable");

    assert_eq!(major, reprise_runtime_protocol::PROTOCOL_VERSION.major);
    assert_eq!(minor, reprise_runtime_protocol::PROTOCOL_VERSION.minor);
}

// --- The client half ---------------------------------------------------
//
// Below this line the tests drive the same service through
// `runtime_client` instead of a hand-written proxy. That is the pairing
// that matters: a client and a service that agree in a test but not in the
// application would each look correct on their own.

use reprise_runtime_client::{
    start_with_bus_name, ClientError, ClientEvent, RuntimeCommand, RuntimeEvents,
};
use reprise_runtime_protocol::playback::PlaybackCommand;

/// Waits for the first event matching `wanted`, ignoring the rest.
///
/// Ignoring rather than asserting on position: the runtime publishes every
/// facet that changed, and a test that pinned the exact sequence would break
/// whenever an unrelated facet started changing too.
fn await_event(
    events: &RuntimeEvents,
    wanted: impl Fn(&ClientEvent) -> bool,
    what: &str,
) -> ClientEvent {
    // Polled rather than blocked on, because the deadline has to bind even
    // when *nothing* arrives. A blocking receive only lets the loop check the
    // clock between events, so a client that goes quiet — exactly what a
    // regression here looks like — turns the test into a hang instead of a
    // failure, and a hang tells nobody anything.
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match events.try_recv() {
            Some(event) if wanted(&event) => return event,
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    panic!("no {what} arrived within the deadline");
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_client_connects_and_is_handed_the_whole_state() {
    let served = Served::start("clientconnect", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());

    let event = await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    let ClientEvent::Connected(snapshot) = event else {
        unreachable!("the matcher only accepts Connected")
    };
    assert_eq!(snapshot.playback.status, "stopped");
    assert_eq!(
        snapshot.protocol_major,
        reprise_runtime_protocol::PROTOCOL_VERSION.major
    );
    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_sent_command_takes_effect_and_its_delta_comes_back() {
    let served = Served::start("clientsend", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    let connected = await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );
    let ClientEvent::Connected(opening) = connected else {
        unreachable!("the matcher only accepts Connected")
    };
    let me = opening.client_id;

    client.send(RuntimeCommand::PlayTracks {
        track_ids: vec![1, 2, 3],
        start_index: 0,
    });

    let event = await_event(
        &events,
        |event| matches!(event, ClientEvent::PlaybackChanged { .. }),
        "playback delta",
    );
    let ClientEvent::PlaybackChanged {
        sequence,
        initiator,
        snapshot,
    } = event
    else {
        unreachable!("the matcher only accepts PlaybackChanged")
    };
    assert!(sequence > 0);
    assert_eq!(snapshot.status, "playing");
    assert_eq!(snapshot.track_id, Some(1));
    assert_eq!(
        initiator,
        Some(me),
        "the attribution has to survive the whole way out and back — the \
         runtime, the signal, and the client's own decoding"
    );
    assert_eq!(snapshot.initiated_by, Some(me));
    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_rejected_send_reports_an_event_instead_of_stalling_the_caller() {
    let served = Served::start("clientreject", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    client.send(RuntimeCommand::Playback(PlaybackCommand::SetRepeat(
        "sometimes".into(),
    )));

    let event = await_event(
        &events,
        |event| matches!(event, ClientEvent::CommandFailed { .. }),
        "failure",
    );
    let ClientEvent::CommandFailed { error, .. } = event else {
        unreachable!("the matcher only accepts CommandFailed")
    };
    assert!(
        matches!(error, ClientError::Rejected(_)),
        "the category survives both hops: {error:?}"
    );
    assert_eq!(error.kind(), "rejected:unknown_repeat_mode");
    assert!(!error.is_retryable());
    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_call_answers_the_caller_directly() {
    let served = Served::start("clientcall", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    let error = client
        .call(RuntimeCommand::Playback(PlaybackCommand::Play))
        .expect_err("nothing is loaded and nothing is queued");

    assert!(
        matches!(error, ClientError::Rejected(_)),
        "a tool call gets its answer as a return value, not as an event: \
         {error:?}"
    );
    assert_eq!(error.kind(), "rejected:nothing_to_play");
    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_command_without_the_capability_is_rejected_over_the_wire_too() {
    let served = Served::start("clientcap", Duration::from_secs(60));
    // A read-only surface: connected, holding no mutation capability.
    let (client, events) = start_with_bus_name(Vec::new(), served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    let error = client
        .call(RuntimeCommand::Playback(PlaybackCommand::Play))
        .expect_err("without playback:control the command is not admissible");

    assert_eq!(error.kind(), "rejected:missing_capability:playback:control");
    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_runtime_that_goes_away_is_reported_rather_than_guessed_at() {
    let mut served = Served::start("clientgone", Duration::ZERO);
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    // The client says goodbye, which leaves the zero-grace runtime with
    // nothing to do; it exits and the bus name loses its owner.
    client.shutdown();
    served.wait_for_shutdown();

    let (next, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    let event = await_event(
        &events,
        |event| matches!(event, ClientEvent::Disconnected),
        "disconnection",
    );

    assert_eq!(event, ClientEvent::Disconnected);
    let error = next
        .call(RuntimeCommand::Playback(PlaybackCommand::Play))
        .expect_err("there is nothing to command");
    assert!(
        error.is_retryable(),
        "an absent runtime is a state to reconnect from, not a failure to \
         report to the user: {error:?}"
    );
    next.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_stream_plays_beside_the_queue_and_does_not_hand_back_to_it() {
    use reprise_runtime_protocol::playback::ExternalMedia;

    let served = Served::start("external", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );
    client
        .call(RuntimeCommand::PlayTracks {
            track_ids: vec![1, 2, 3],
            start_index: 0,
        })
        .expect("playing the queue succeeds");

    client
        .call(RuntimeCommand::PlayExternal(ExternalMedia {
            location: "https://stream.example/live".into(),
            remote: true,
            title: "Morning Show".into(),
            artist: "Example FM".into(),
            duration_ms: 0,
        }))
        .expect("playing a stream succeeds");

    let event = await_event(
        &events,
        |event| match event {
            ClientEvent::PlaybackChanged { snapshot, .. } => snapshot.title == "Morning Show",
            _ => false,
        },
        "the stream's delta",
    );
    let ClientEvent::PlaybackChanged { snapshot, .. } = event else {
        unreachable!("the matcher only accepts PlaybackChanged")
    };
    assert_eq!(
        snapshot.track_id, None,
        "a stream has no library id on the wire either"
    );
    assert_eq!(snapshot.artist, "Example FM");

    // The queue is still there to go back to. Read through a second peer,
    // because a snapshot is what any surface would take after a stream ends.
    let observer = served.client();
    let seen = observer.connect().expect("a second peer connects");
    assert!(
        !seen.queue.context_track_ids.is_empty(),
        "the music the stream interrupted is still queued"
    );
    assert_eq!(
        seen.playback.title, "Morning Show",
        "and the stream is what is playing"
    );

    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_command_formed_while_disconnected_never_runs_after_the_reconnection() {
    use reprise_runtime_protocol::queue::QueueCommand;

    // The client comes up first, against a name nobody serves yet.
    let bus_name = Served::name_for("earlycommand");
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], bus_name.clone());
    let error = client
        .call(RuntimeCommand::Queue(QueueCommand::AddLast(vec![1, 2, 3])))
        .expect_err("there is nothing connected to command");
    assert!(
        error.is_retryable(),
        "an absent runtime is a state to reconnect from: {error:?}"
    );

    // Now the runtime appears and the client's own backoff reconnects it.
    let served = Served::start_on(bus_name, Duration::from_secs(60));
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    let observer = served.client();
    let seen = observer.connect().expect("a second peer connects");
    assert!(
        seen.queue.play_next_track_ids.is_empty(),
        "the command was formed against a session that did not exist; \
         executing it once one appeared would run an intention against state \
         it never saw, which is the failure §9.5 calls the more dangerous one"
    );
    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_refused_client_is_told_why_instead_of_reconnecting_forever() {
    let served = Served::start("clientrefused", Duration::from_secs(60));
    // A client from a future major version: the runtime cannot serve it, and
    // no amount of retrying will change that.
    let (client, events) = reprise_runtime_client::start_with_bus_name_and_version(
        vec!["playback:control".to_owned()],
        served.bus_name.clone(),
        reprise_runtime_protocol::ProtocolVersion {
            major: reprise_runtime_protocol::PROTOCOL_VERSION.major + 1,
            minor: 0,
        },
    );

    let event = await_event(
        &events,
        |event| matches!(event, ClientEvent::Refused(_)),
        "refusal",
    );

    let ClientEvent::Refused(error) = event else {
        unreachable!("the matcher only accepts Refused")
    };
    assert_eq!(error.kind(), "refused:protocol_major");
    assert!(
        !error.is_retryable(),
        "a surface must be able to say why it will never connect, rather \
         than showing a spinner for a runtime that will never accept it"
    );
    client.shutdown();
}

#[path = "runtime_service/queue_surface.rs"]
mod queue_surface;
