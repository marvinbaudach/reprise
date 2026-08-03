use std::sync::{Arc, Mutex};

use reprise_core::playback::{PlaybackState, PlayerEvent, StreamEvent, StreamGeneration};

use crate::playback::{AndroidPlaybackState, AndroidPlayerEvent, PlaybackEventBridge};

#[test]
fn playback_event_bridge_delivers_ordered_core_events_with_production_generations() {
    let received = Arc::new(Mutex::new(Vec::<StreamEvent>::new()));
    let recorded = Arc::clone(&received);
    let bridge = PlaybackEventBridge::new(Box::new(move |event| {
        recorded.lock().unwrap().push(event);
    }));

    bridge.emit(
        7,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Playing,
        },
    );
    bridge.emit(
        7,
        AndroidPlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000,
        },
    );
    bridge.emit(8, AndroidPlayerEvent::AdvancedToNext);
    bridge.emit(8, AndroidPlayerEvent::TrackFinished);
    bridge.emit(
        8,
        AndroidPlayerEvent::Error {
            message: "decoder failed".to_owned(),
        },
    );

    let events = received.lock().unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(events[0].generation, StreamGeneration::from(7));
    assert!(matches!(
        events[0].event,
        PlayerEvent::StateChanged(PlaybackState::Playing)
    ));
    assert_eq!(events[1].generation, StreamGeneration::from(7));
    assert!(matches!(
        events[1].event,
        PlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000
        }
    ));
    assert_eq!(events[2].generation, StreamGeneration::from(8));
    assert!(matches!(events[2].event, PlayerEvent::AdvancedToNext));
    assert_eq!(events[3].generation, StreamGeneration::from(8));
    assert!(matches!(events[3].event, PlayerEvent::TrackFinished));
    assert_eq!(events[4].generation, StreamGeneration::from(8));
    assert!(matches!(
        &events[4].event,
        PlayerEvent::Error(message) if message == "decoder failed"
    ));
}
