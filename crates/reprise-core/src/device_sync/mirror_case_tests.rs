use super::*;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

#[derive(Clone, Default)]
struct CapturedCaseWarnings(Arc<Mutex<Vec<CapturedCaseWarning>>>);

#[derive(Default)]
struct CapturedCaseWarning {
    track_id: Option<i64>,
    first_spelling: Option<String>,
    second_spelling: Option<String>,
}

impl Visit for CapturedCaseWarning {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "track_id" {
            self.track_id = Some(value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "first_spelling" => self.first_spelling = Some(value.to_owned()),
            "second_spelling" => self.second_spelling = Some(value.to_owned()),
            _ => {}
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

impl Subscriber for CapturedCaseWarnings {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut warning = CapturedCaseWarning::default();
        event.record(&mut warning);
        if *event.metadata().level() == tracing::Level::WARN && warning.track_id.is_some() {
            self.0.lock().unwrap().push(warning);
        }
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

fn case_track(id: i64, album_artist: &str, album: &str, title: &str) -> SyncTrack {
    let mut value = track(id, &format!("/music/{id}.mp3"), Some(192), 10_000, 240_000);
    value.album_artist = album_artist.into();
    value.album = album.into();
    value.title = title.into();
    value
}

fn selected_input(track: SyncTrack) -> MirrorInput {
    let source = SelectionSource::Playlist(10);
    input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Case",
            vec![MirrorTrack::Available(track)],
        )],
    )
}

fn unavailable(track_id: i64) -> MirrorTrack {
    MirrorTrack::Unavailable(UnavailableTrack {
        track_id,
        title: format!("Track {track_id}"),
        artist: "Artist".into(),
        duration_ms: 10_000,
    })
}

fn managed(path: &str) -> ManagedDeviceFile {
    ManagedDeviceFile {
        relative_path: path.into(),
        size_bytes: 240_000,
    }
}

#[test]
fn resident_case_spelling_prevents_transfer_and_owns_the_analysis_path() {
    let wanted = case_track(1, "Bring Me the Horizon", "Sempiternal", "Track 1");
    let resident = "Bring Me The Horizon/Sempiternal/01 Track 1.mp3";
    let mut mirror_input = selected_input(wanted.clone());
    mirror_input
        .inventory
        .push(inventory(&wanted, resident, "copy-original-v1"));
    mirror_input.managed_files.push(managed(resident));
    mirror_input.desktop_analyses.push(DesktopAnalysis {
        track_id: wanted.id,
        size_bytes: 123,
    });

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert_eq!(plan.desired_files[0].device_path, resident);
    assert_eq!(plan.analysis_writes.len(), 1);
    assert_eq!(
        plan.analysis_writes[0].device_path,
        "Bring Me The Horizon/Sempiternal/01 Track 1.reprise-analysis"
    );
}

#[test]
fn resident_non_ascii_case_spelling_is_adopted() {
    let mut wanted = case_track(12, "Artist", "Album", "Angriff der Dönerteller");
    wanted.source_path = "/music/twelve.flac".into();
    wanted.original_name = "twelve.flac".into();
    wanted.bitrate_kbps = None;
    wanted.size_bytes = 1_000_000;
    let resident = "Artist/Album/12 Angriff der DÖNERTELLER.opus";
    let mut mirror_input = selected_input(wanted.clone());
    mirror_input.profile = TransferProfile::Opus160;
    mirror_input
        .inventory
        .push(inventory(&wanted, resident, "opus-vbr-160-v1"));
    mirror_input.managed_files.push(managed(resident));
    mirror_input.desktop_analyses.push(DesktopAnalysis {
        track_id: wanted.id,
        size_bytes: 123,
    });

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert_eq!(plan.desired_files[0].device_path, resident);
    assert_eq!(
        plan.analysis_writes[0].device_path,
        "Artist/Album/12 Angriff der DÖNERTELLER.reprise-analysis"
    );
}

#[test]
fn authoritative_scan_accepts_the_resident_case_variant_of_the_inventory_path() {
    let wanted = case_track(1, "Carnifex", "Graveside Confessions", "Track 1");
    let inventory_path = "Carnifex/Graveside Confessions/01 Track 1.mp3";
    let resident_path = "Carnifex/GRAVESIDE CONFESSIONS/01 Track 1.mp3";
    let mut mirror_input = selected_input(wanted.clone());
    mirror_input
        .inventory
        .push(inventory(&wanted, inventory_path, "copy-original-v1"));
    mirror_input.managed_files.push(managed(resident_path));
    mirror_input.managed_files_scanned = true;

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
}

#[test]
fn new_track_uses_the_majority_resident_directory_spelling() {
    let wanted = case_track(9, "Current Artist", "Current Album", "Track 9");
    let mut mirror_input = selected_input(wanted);
    mirror_input.managed_files = vec![
        managed("CURRENT ARTIST/Current Album/01 One.mp3"),
        managed("CURRENT ARTIST/Current Album/02 Two.mp3"),
        managed("CURRENT ARTIST/Current Album/03 Three.mp3"),
        managed("Current Artist/Current Album/04 Four.mp3"),
    ];

    let plan = plan_mirror(mirror_input);

    assert_eq!(plan.copy.len(), 1);
    assert_eq!(
        plan.copy[0].device_path,
        "CURRENT ARTIST/Current Album/09 Track 9.mp3"
    );
}

#[test]
fn equal_directory_counts_plan_neither_arrival_analysis_nor_removal() {
    let wanted = case_track(9, "Tie Artist", "Tie Album", "Track 9");
    let source = SelectionSource::Playlist(10);
    let mut entries = vec![MirrorTrack::Available(wanted)];
    entries.extend((1..=8).map(unavailable));
    let mut mirror_input = input(vec![source.clone()], vec![playlist(source, "Tie", entries)]);
    for id in 1..=8 {
        let spelling = if id <= 4 {
            "TIE ARTIST/Tie Album"
        } else {
            "Tie Artist/TIE ALBUM"
        };
        let path = format!("{spelling}/{id:02} Resident {id}.mp3");
        let resident = case_track(id, spelling, "Unused", &format!("Resident {id}"));
        mirror_input
            .inventory
            .push(inventory(&resident, &path, "copy-original-v1"));
        mirror_input.managed_files.push(managed(&path));
    }
    mirror_input.desktop_analyses.push(DesktopAnalysis {
        track_id: 9,
        size_bytes: 123,
    });

    let captured = CapturedCaseWarnings::default();
    let plan = tracing::subscriber::with_default(captured.clone(), || plan_mirror(mirror_input));

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert!(plan.analysis_writes.is_empty());
    assert!(plan.remove.is_empty());
    assert!(!plan.desired_files.iter().any(|file| file.track.id == 9));
    let warnings = captured.0.lock().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].track_id, Some(9));
    assert_eq!(
        warnings[0].first_spelling.as_deref(),
        Some("TIE ARTIST/Tie Album")
    );
    assert_eq!(
        warnings[0].second_spelling.as_deref(),
        Some("Tie Artist/TIE ALBUM")
    );
}

#[test]
fn unavailable_track_keeps_its_minority_inventory_spelling() {
    let source = SelectionSource::Playlist(10);
    let mut entries = vec![unavailable(9)];
    entries.extend((1..=4).map(unavailable));
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(source, "Retained", entries)],
    );
    let minority_track = case_track(9, "Minority Artist", "Album", "Track 9");
    let minority_path = "Minority Artist/Album/09 Track 9.mp3";
    let minority = inventory(&minority_track, minority_path, "copy-original-v1");
    mirror_input.inventory.push(minority.clone());
    mirror_input.managed_files.push(managed(minority_path));
    for id in 1..=4 {
        let path = format!("MINORITY ARTIST/Album/{id:02} Resident {id}.mp3");
        let resident = case_track(id, "MINORITY ARTIST", "Album", &format!("Resident {id}"));
        mirror_input
            .inventory
            .push(inventory(&resident, &path, "copy-original-v1"));
        mirror_input.managed_files.push(managed(&path));
    }

    let plan = plan_mirror(mirror_input);

    assert!(plan.retained_unavailable.contains(&minority));
    assert!(!plan.remove.iter().any(|removal| match removal {
        ManagedRemoval::Inventory(file) => file.device_path == minority_path,
        ManagedRemoval::Orphan(file) => file.relative_path == minority_path,
    }));
}

#[test]
fn own_inventory_path_beats_the_directory_majority() {
    let wanted = case_track(9, "Minority Artist", "Album", "Track 9");
    let minority_path = "Minority Artist/Album/09 Track 9.mp3";
    let mut mirror_input = selected_input(wanted.clone());
    mirror_input
        .inventory
        .push(inventory(&wanted, minority_path, "copy-original-v1"));
    mirror_input.managed_files = vec![
        managed(minority_path),
        managed("MINORITY ARTIST/Album/01 Other.mp3"),
        managed("MINORITY ARTIST/Album/02 Other.mp3"),
        managed("MINORITY ARTIST/Album/03 Other.mp3"),
    ];

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert_eq!(plan.desired_files[0].device_path, minority_path);
}

#[test]
fn case_variant_managed_sibling_is_not_an_orphan_removal() {
    let wanted = case_track(1, "Bring Me the Horizon", "Album", "Track 1");
    let known = "Bring Me the Horizon/Album/01 Track 1.mp3";
    let resident = "Bring Me The Horizon/Album/01 Track 1.mp3";
    let mut mirror_input = selected_input(wanted.clone());
    mirror_input
        .inventory
        .push(inventory(&wanted, known, "copy-original-v1"));
    mirror_input.managed_files.push(managed(resident));

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert!(plan.remove.is_empty());
}
