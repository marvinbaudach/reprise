# Rhythmbox Import Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current push-NavigationPage import flow with a three-state dialog (Selection → Progress → Complete) matching design mock frame 12, including prescan metadata, live progress, and undo.

**Architecture:** Backend adds `prescan_rhythmdb()` (read-only library scan), merges the `play_counts` + `last_played_at` choices into one, adds a progress callback and rollback support to `merge_stats()`. Frontend replaces the push page with an `adw::Dialog` hosting a `gtk4::Stack` of three states. Prescan runs on dialog open; import runs with progress updates; completion shows detailed summary with undo.

**Tech Stack:** Rust, gtk4-rs, libadwaita, rusqlite 0.32, quick_xml, glib async

## Global Constraints

- Edition 2021, workspace lints enforced (`clippy -D warnings`)
- Uninlined format args: use `format!("{x}")` not `format!("{}", x)`
- Semicolons on unit-returning expressions
- Immutable data: never mutate arguments, return new values
- No `std::process::Command` in reprise-core
- Test pattern: `tempfile::tempdir()`, in-memory DB via `crate::db::migrate`
- Existing test convention: tests in same file with `#[cfg(test)] mod tests`
- Styling: CSS via `ui/style` — no per-widget `CssProvider`
- Never open a window on the real desktop — headless tests only (Xvfb)

## Parallelism

```
Task 1 (prescan + choices refactor) → Task 2 (progress + rollback) → Task 3 (frontend dialog) → Task 4 (smoke + call-site wiring)
```

Sequential — each task consumes the prior task's types.

---

### Task 1: Prescan API + RhythmboxImportChoices refactor

**Files:**
- Modify: `crates/reprise-core/src/library/rhythmbox_import.rs`

**Interfaces:**
- Produces: `RhythmboxPrescanResult`, `prescan_rhythmdb()`, updated `RhythmboxImportChoices` (3 fields: `ratings`, `play_counts_and_last_played`, `added_at`) — consumed by Tasks 2, 3, 4

- [ ] **Step 1: Write prescan test**

Add to the `#[cfg(test)] mod tests` block in `rhythmbox_import.rs`:

```rust
#[test]
fn prescan_counts_entries_and_classifies_skips() {
    let dir = tempdir().unwrap();
    let music_dir = dir.path().join("music");
    fs::create_dir_all(&music_dir).unwrap();
    let existing = music_dir.join("song.ogg");
    fs::write(&existing, b"fake").unwrap();
    let existing_uri = url::Url::from_file_path(&existing).unwrap();
    let missing_uri =
        url::Url::from_file_path(music_dir.join("gone.ogg")).unwrap();
    let outside_uri =
        url::Url::from_file_path(dir.path().join("elsewhere.ogg")).unwrap();
    let xml = format!(
        r#"<?xml version="1.0"?>
<rhythmdb version="2.0">
  <entry type="song"><location>{existing_uri}</location><rating>4</rating><play-count>10</play-count><first-seen>1700000000</first-seen><last-played>1700000500</last-played></entry>
  <entry type="song"><location>{missing_uri}</location><rating>3</rating></entry>
  <entry type="song"><location>{outside_uri}</location><play-count>5</play-count></entry>
  <entry type="podcast-post"><location>file:///podcast.ogg</location><rating>5</rating></entry>
</rhythmdb>"#
    );
    let rhythmdb = dir.path().join("rhythmdb.xml");
    fs::write(&rhythmdb, xml).unwrap();
    let playlists_path = dir.path().join("playlists.xml");
    fs::write(
        &playlists_path,
        r#"<?xml version="1.0"?>
<rhythmdb-playlists>
  <playlist name="Gym" type="static">
    <location>file:///a.ogg</location>
    <location>file:///b.ogg</location>
  </playlist>
</rhythmdb-playlists>"#,
    )
    .unwrap();

    let conn = Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
        [existing.to_string_lossy()],
    )
    .unwrap();

    let library_root = music_dir.to_string_lossy().to_string();
    let result = prescan_rhythmdb(
        &rhythmdb,
        &playlists_path,
        &conn,
        Some(&library_root),
    )
    .unwrap();

    assert_eq!(result.total_entries, 4);
    assert_eq!(result.song_entries, 3);
    assert_eq!(result.non_song_entries, 1);
    assert_eq!(result.rated_tracks, 2);
    assert_eq!(result.tracks_with_history, 2);
    assert_eq!(result.tracks_with_date_added, 1);
    assert_eq!(result.matched, 1);
    assert_eq!(result.outside_library, 1);
    assert_eq!(result.missing_on_disk, 1);
    assert_eq!(result.playlist_count, 1);
    assert_eq!(result.playlist_track_count, 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core rhythmbox_import::tests::prescan_counts -- --nocapture 2>&1 | tail -10`
Expected: FAIL — `prescan_rhythmdb` not found

- [ ] **Step 3: Add `RhythmboxPrescanResult` struct and `prescan_rhythmdb()` function**

Add at the top of `rhythmbox_import.rs`, after the existing structs:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RhythmboxPrescanResult {
    pub total_entries: usize,
    pub song_entries: usize,
    pub non_song_entries: usize,
    pub rated_tracks: usize,
    pub tracks_with_history: usize,
    pub tracks_with_date_added: usize,
    pub matched: usize,
    pub outside_library: usize,
    pub missing_on_disk: usize,
    pub playlist_count: usize,
    pub playlist_track_count: usize,
    pub last_modified: Option<std::time::SystemTime>,
}

pub fn prescan_rhythmdb(
    rhythmdb_path: &Path,
    playlists_path: &Path,
    conn: &Connection,
    library_root: Option<&str>,
) -> Result<RhythmboxPrescanResult, RhythmboxImportError> {
    let last_modified = std::fs::metadata(rhythmdb_path)
        .and_then(|m| m.modified())
        .ok();

    // Parse all entries (not just songs) to count totals
    let file = File::open(rhythmdb_path)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = true;
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut result = RhythmboxPrescanResult {
        last_modified,
        ..RhythmboxPrescanResult::default()
    };

    // Track current entry state
    let mut in_song = false;
    let mut in_non_song_entry = false;
    let mut entry_builder: Option<EntryBuilder> = None;
    let mut field: Option<Field> = None;

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(element) => {
                depth += 1;
                if element.name().as_ref() == b"entry" {
                    let entry_type = element.attributes().flatten().find_map(|attr| {
                        (attr.key.as_ref() == b"type").then(|| {
                            attr.decoded_and_normalized_value(
                                quick_xml::XmlVersion::Implicit1_0,
                                reader.decoder(),
                            )
                            .ok()
                            .map(|v| v.into_owned())
                        })
                    }).flatten();
                    result.total_entries += 1;
                    match entry_type.as_deref() {
                        Some("song") => {
                            result.song_entries += 1;
                            in_song = true;
                            in_non_song_entry = false;
                            entry_builder = Some(EntryBuilder::default());
                        }
                        _ => {
                            result.non_song_entries += 1;
                            in_song = false;
                            in_non_song_entry = true;
                            entry_builder = None;
                        }
                    }
                    field = None;
                } else if entry_builder.is_some() {
                    field = field_for(element.name().as_ref());
                }
            }
            Event::Text(text) => {
                if let (Some(builder), Some(f)) = (&mut entry_builder, field) {
                    let decoded = text.decode()?;
                    builder.push(f, &decoded);
                }
            }
            Event::CData(text) => {
                if let (Some(builder), Some(f)) = (&mut entry_builder, field) {
                    let decoded = text.decode()?;
                    builder.push(f, &decoded);
                }
            }
            Event::GeneralRef(reference) => {
                if let (Some(builder), Some(f)) = (&mut entry_builder, field) {
                    let reference = reference.decode()?;
                    let escaped = format!("&{reference};");
                    let decoded = quick_xml::escape::unescape(&escaped)?;
                    builder.push(f, &decoded);
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                if element.name().as_ref() == b"entry" {
                    if let Some(builder) = entry_builder.take() {
                        if let Some(track) = builder.finish() {
                            if track.rating.is_some() {
                                result.rated_tracks += 1;
                            }
                            if track.play_count.unwrap_or(0) > 0
                                || track.last_played_at.is_some()
                            {
                                result.tracks_with_history += 1;
                            }
                            if track.added_at.is_some() {
                                result.tracks_with_date_added += 1;
                            }
                            // Classify against library
                            let path_str = track.path.to_string_lossy();
                            let in_db = conn
                                .query_row(
                                    "SELECT 1 FROM tracks WHERE path = ?1",
                                    [&path_str],
                                    |_| Ok(()),
                                )
                                .optional()
                                .unwrap_or(None)
                                .is_some();
                            if in_db {
                                result.matched += 1;
                            } else {
                                let under_root = library_root
                                    .is_some_and(|root| path_str.starts_with(root));
                                if !under_root {
                                    result.outside_library += 1;
                                } else if !track.path.exists() {
                                    result.missing_on_disk += 1;
                                } else {
                                    // File exists under root but not in DB
                                    // (not yet scanned) — count as outside_library
                                    result.outside_library += 1;
                                }
                            }
                        }
                        // EntryBuilder::finish returned None — entry had no
                        // useful stats (no rating, no play_count, no dates).
                        // These are still counted in song_entries above.
                    }
                    in_song = false;
                    in_non_song_entry = false;
                    field = None;
                } else if field_for(element.name().as_ref()).is_some() {
                    field = None;
                }
            }
            Event::Eof => {
                if depth != 0 {
                    return Err(RhythmboxImportError::UnexpectedEof);
                }
                break;
            }
            _ => {}
        }
        buffer.clear();
    }

    // Count playlists
    if playlists_path.is_file() {
        if let Ok(playlists) = parse_playlists(playlists_path) {
            result.playlist_count = playlists.len();
            result.playlist_track_count =
                playlists.iter().map(|p| p.paths.len()).sum();
        }
    }

    Ok(result)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p reprise-core rhythmbox_import::tests::prescan_counts -- --nocapture 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Refactor `RhythmboxImportChoices` — merge `play_counts` + `last_played_at` into `play_counts_and_last_played`**

In `rhythmbox_import.rs`, change the struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhythmboxImportChoices {
    pub ratings: bool,
    pub play_counts_and_last_played: bool,
    pub added_at: bool,
}
```

In `merge_stats()`, update the two references:
- Line 265: change `if choices.play_counts {` → `if choices.play_counts_and_last_played {`
- Line 283: change `if choices.last_played_at {` → `if choices.play_counts_and_last_played {`

Update ALL existing test instances of `RhythmboxImportChoices` in the same file. Each currently has 4 fields — replace with 3. Examples:

```rust
// was: RhythmboxImportChoices { ratings: true, play_counts: true, added_at: false, last_played_at: false }
// now:
RhythmboxImportChoices { ratings: true, play_counts_and_last_played: true, added_at: false }

// was: RhythmboxImportChoices { ratings: false, play_counts: true, added_at: false, last_played_at: false }
// now:
RhythmboxImportChoices { ratings: false, play_counts_and_last_played: true, added_at: false }

// was: RhythmboxImportChoices { ratings: false, play_counts: false, added_at: true, last_played_at: false }
// now:
RhythmboxImportChoices { ratings: false, play_counts_and_last_played: false, added_at: true }

// was: RhythmboxImportChoices { ratings: false, play_counts: false, added_at: false, last_played_at: true }
// now:
RhythmboxImportChoices { ratings: false, play_counts_and_last_played: true, added_at: false }
```

Note for the `last_played_at`-only test (`merge_imports_only_a_newer_positive_last_played_idempotently`): since last_played is now merged with play_counts, set `play_counts_and_last_played: true`. The test still validates last_played behavior because the fixture data only has last_played values.

- [ ] **Step 6: Update call sites in `preference_rhythmbox.rs`**

In `preference_rhythmbox.rs`, the `RhythmboxOption` enum loses `PlayCounts` and `LastPlayed` as separate variants. Replace with `PlayCountsAndLastPlayed`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RhythmboxOption {
    ColumnLayout,
    Ratings,
    PlayCountsAndLastPlayed,
    DateAdded,
    Playlists,
}
```

Update `import_option_specs()` to return 5 items:

```rust
fn import_option_specs() -> [ImportOptionSpec; 5] {
    [
        ImportOptionSpec { id: RhythmboxOption::ColumnLayout, selected: false },
        ImportOptionSpec { id: RhythmboxOption::Ratings, selected: true },
        ImportOptionSpec { id: RhythmboxOption::PlayCountsAndLastPlayed, selected: true },
        ImportOptionSpec { id: RhythmboxOption::DateAdded, selected: true },
        ImportOptionSpec { id: RhythmboxOption::Playlists, selected: true },
    ]
}
```

Update `option_title()`:

```rust
fn option_title(option: RhythmboxOption) -> String {
    strings::text(match option {
        RhythmboxOption::ColumnLayout => strings::ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT,
        RhythmboxOption::Ratings => strings::RHYTHMBOX_IMPORT_RATINGS,
        RhythmboxOption::PlayCountsAndLastPlayed => strings::RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED,
        RhythmboxOption::DateAdded => strings::RHYTHMBOX_IMPORT_DATE_ADDED,
        RhythmboxOption::Playlists => strings::RHYTHMBOX_IMPORT_PLAYLISTS,
    })
}
```

Update `push_import_page()` import button callback — now has 5 rows mapping to 3-field choices:

```rust
surface.import_button.connect_clicked(move |button| {
    let Some(context) = weak.upgrade() else { return };
    context.start_rhythmbox_import(
        button,
        rows[0].is_active(),  // column_layout
        RhythmboxImportChoices {
            ratings: rows[1].is_active(),
            play_counts_and_last_played: rows[2].is_active(),
            added_at: rows[3].is_active(),
        },
        rows[4].is_active(),  // playlists
    );
});
```

Update the smoke block in `add_rhythmbox_import_row()`:

```rust
RhythmboxImportChoices {
    ratings: true,
    play_counts_and_last_played: true,
    added_at: true,
},
```

- [ ] **Step 7: Verify `first_run.rs` is unaffected**

`first_run.rs` has its OWN local `RhythmboxImportChoices` struct (line 31, only `column_layout: bool`). It does NOT use the core `RhythmboxImportChoices` — it activates `ACTION_IMPORT_RHYTHMBOX_COLUMNS` which only handles column layout. No changes needed.

Run: `cargo check -p reprise-gnome 2>&1 | grep first_run`
Expected: no errors mentioning first_run

- [ ] **Step 8: Add new string constant**

In `strings_rhythmbox.rs`, add:

```rust
pub const RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED: &str = N_!("Play counts & last played");
```

- [ ] **Step 9: Update test assertions in `preference_rhythmbox.rs`**

Update the `statistics_are_selected_but_column_layout_requires_opt_in` test:

```rust
#[test]
fn statistics_are_selected_but_column_layout_requires_opt_in() {
    let options = import_option_specs();
    assert_eq!(options.len(), 5);
    assert_eq!(options[0].id, RhythmboxOption::ColumnLayout);
    assert!(!options[0].selected);
    assert_eq!(options[1].id, RhythmboxOption::Ratings);
    assert!(options[1].selected);
    assert_eq!(options[2].id, RhythmboxOption::PlayCountsAndLastPlayed);
    assert!(options[2].selected);
    assert_eq!(options[3].id, RhythmboxOption::DateAdded);
    assert!(options[3].selected);
    assert_eq!(options[4].id, RhythmboxOption::Playlists);
    assert!(options[4].selected);
}
```

Update the display test `import_page_exposes_all_six_explicit_choices` — rename to `import_page_exposes_all_five_explicit_choices` and update:

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn import_page_exposes_all_five_explicit_choices() {
    gtk4::init().unwrap();
    let surface = build_import_page();
    assert_eq!(surface.page.title(), "Import from Rhythmbox");
    assert!(surface.page.can_pop());
    assert!(surface.page.child().is_some_and(|child| child.is::<adw::ToolbarView>()));
    assert_eq!(surface.import_button.label().as_deref(), Some("Import"));
    let root = adw::NavigationPage::with_tag(
        &gtk4::Box::new(gtk4::Orientation::Vertical, 0),
        "Preferences",
        "preferences",
    );
    let navigation = adw::NavigationView::new();
    navigation.add(&root);
    navigation.push(&surface.page);
    assert_eq!(navigation.visible_page().as_ref(), Some(&surface.page));
    assert!(navigation.pop());
    assert_eq!(navigation.visible_page().as_ref(), Some(&root));
    assert_eq!(surface.rows.len(), 5);
    assert_eq!(surface.rows[0].title(), "Column layout");
    assert_eq!(surface.rows[1].title(), "Ratings");
    assert_eq!(surface.rows[2].title(), "Play counts & last played");
    assert_eq!(surface.rows[3].title(), "Date added");
    assert_eq!(surface.rows[4].title(), "Playlists");
    assert!(!surface.rows[0].is_active());
    assert!(surface.rows[1].is_active());
    assert!(surface.rows[2].is_active());
    assert!(surface.rows[3].is_active());
    assert!(surface.rows[4].is_active());
}
```

- [ ] **Step 10: Run all affected tests**

Run: `cargo test -p reprise-core rhythmbox_import -- --nocapture 2>&1 | tail -20`
Expected: ALL PASS

Run: `cargo test -p reprise-gnome preference_rhythmbox -- --nocapture 2>&1 | tail -10`
Expected: ALL PASS (non-display tests)

- [ ] **Step 11: Run full workspace check**

Run: `cargo check --workspace 2>&1 | tail -10`
Expected: no errors (all call sites updated)

- [ ] **Step 12: Commit**

```bash
git add crates/reprise-core/src/library/rhythmbox_import.rs \
       crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs \
       crates/reprise-gnome/src/ui/strings_rhythmbox.rs
git commit -m "feat(import): prescan API + merge play_counts/last_played choices"
```

---

### Task 2: Progress callback + rollback support in `merge_stats`

**Files:**
- Modify: `crates/reprise-core/src/library/rhythmbox_import.rs`

**Interfaces:**
- Consumes: `RhythmboxImportChoices` (3-field version from Task 1)
- Produces: `RhythmboxRollbackEntry`, `RhythmboxRollback`, `undo_rhythmbox_import()`, updated `merge_stats()` signature with `on_progress` + `RhythmboxRollback` return — consumed by Tasks 3, 4

- [ ] **Step 1: Write rollback round-trip test**

```rust
#[test]
fn merge_returns_rollback_and_undo_restores_original_values() {
    let path = PathBuf::from("/music/song.ogg");
    let mut conn = database(&path, 3, 5);
    conn.execute("UPDATE tracks SET added_at = 100, last_played_at = 200", [])
        .unwrap();

    let (summary, rollback) = merge_stats(
        &mut conn,
        &[RhythmboxTrackStats {
            path: path.clone(),
            rating: Some(5),
            play_count: Some(20),
            added_at: Some(50),
            last_played_at: Some(300),
        }],
        RhythmboxImportChoices {
            ratings: true,
            play_counts_and_last_played: true,
            added_at: true,
        },
        None,
    )
    .unwrap();

    // Verify import took effect
    assert_eq!(summary.play_counts_raised, 1);
    assert_eq!(summary.dates_imported, 1);
    assert_eq!(summary.last_played_imported, 1);
    assert_eq!(values(&conn), (3, 20)); // rating unchanged (was already set)

    // Undo
    let restored = undo_rhythmbox_import(&mut conn, &rollback).unwrap();
    assert_eq!(restored, 1);
    assert_eq!(values(&conn), (3, 5));
    let (added_at, last_played) = conn
        .query_row("SELECT added_at, last_played_at FROM tracks", [], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .unwrap();
    assert_eq!(added_at, 100);
    assert_eq!(last_played, Some(200));
}
```

- [ ] **Step 2: Write progress callback test**

```rust
#[test]
fn merge_calls_progress_for_each_track() {
    let path1 = PathBuf::from("/music/a.ogg");
    let path2 = PathBuf::from("/music/b.ogg");
    let conn_raw = Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn_raw).unwrap();
    conn_raw
        .execute(
            "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
            [path1.to_string_lossy()],
        )
        .unwrap();
    conn_raw
        .execute(
            "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
            [path2.to_string_lossy()],
        )
        .unwrap();
    let mut conn = conn_raw;

    let progress = std::cell::Cell::new(0usize);
    let (_, _) = merge_stats(
        &mut conn,
        &[
            RhythmboxTrackStats {
                path: path1,
                rating: Some(4),
                play_count: None,
                added_at: None,
                last_played_at: None,
            },
            RhythmboxTrackStats {
                path: path2,
                rating: Some(3),
                play_count: None,
                added_at: None,
                last_played_at: None,
            },
        ],
        RhythmboxImportChoices {
            ratings: true,
            play_counts_and_last_played: false,
            added_at: false,
        },
        Some(&|n| { progress.set(n); }),
    )
    .unwrap();

    assert_eq!(progress.get(), 2);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p reprise-core rhythmbox_import::tests::merge_returns_rollback -- --nocapture 2>&1 | tail -5`
Expected: FAIL — signature mismatch

- [ ] **Step 4: Add rollback types and update `merge_stats` signature**

Add structs after `RhythmboxImportSummary`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmboxRollbackEntry {
    pub path: String,
    pub rating: i32,
    pub play_count: i64,
    pub added_at: i64,
    pub last_played_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RhythmboxRollback {
    pub entries: Vec<RhythmboxRollbackEntry>,
}
```

Update `merge_stats` signature and body:

```rust
pub fn merge_stats(
    conn: &mut Connection,
    tracks: &[RhythmboxTrackStats],
    choices: RhythmboxImportChoices,
    on_progress: Option<&dyn Fn(usize)>,
) -> Result<(RhythmboxImportSummary, RhythmboxRollback), RhythmboxImportError> {
    let transaction = conn.transaction()?;
    let mut summary = RhythmboxImportSummary {
        parsed: tracks.len(),
        ..RhythmboxImportSummary::default()
    };
    let mut rollback = RhythmboxRollback::default();

    for (index, track) in tracks.iter().enumerate() {
        let path = track.path.to_string_lossy();
        let current = transaction
            .query_row(
                "SELECT rating, play_count, added_at, last_played_at FROM tracks WHERE path = ?1",
                [&path],
                |row| Ok((
                    row.get::<_, i32>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                )),
            )
            .optional()?;
        let Some((current_rating, current_play_count, current_added_at, current_last_played)) = current else {
            summary.skipped += 1;
            if let Some(cb) = on_progress { cb(index + 1); }
            continue;
        };
        summary.matched += 1;

        let next_rating = if choices.ratings && current_rating == 0 {
            track.rating.unwrap_or(current_rating)
        } else {
            current_rating
        };
        let next_play_count = if choices.play_counts_and_last_played {
            track.play_count.map_or(current_play_count, |imported| current_play_count.max(imported))
        } else {
            current_play_count
        };
        let next_added_at = if choices.added_at {
            track.added_at.map_or(current_added_at, |imported| {
                if current_added_at > 0 { current_added_at.min(imported) } else { imported }
            })
        } else {
            current_added_at
        };
        let next_last_played = if choices.play_counts_and_last_played {
            match (current_last_played, track.last_played_at) {
                (Some(current), Some(imported)) => Some(current.max(imported)),
                (None, Some(imported)) => Some(imported),
                (current, None) => current,
            }
        } else {
            current_last_played
        };

        summary.ratings_imported += usize::from(next_rating != current_rating);
        summary.play_counts_raised += usize::from(next_play_count != current_play_count);
        summary.dates_imported += usize::from(next_added_at != current_added_at);
        summary.last_played_imported += usize::from(next_last_played != current_last_played);

        if next_rating != current_rating
            || next_play_count != current_play_count
            || next_added_at != current_added_at
            || next_last_played != current_last_played
        {
            rollback.entries.push(RhythmboxRollbackEntry {
                path: path.to_string(),
                rating: current_rating,
                play_count: current_play_count,
                added_at: current_added_at,
                last_played_at: current_last_played,
            });
            transaction.execute(
                "UPDATE tracks SET rating = ?1, play_count = ?2, added_at = ?3, last_played_at = ?4 WHERE path = ?5",
                rusqlite::params![next_rating, next_play_count, next_added_at, next_last_played, path],
            )?;
        }
        if let Some(cb) = on_progress { cb(index + 1); }
    }

    transaction.commit()?;
    Ok((summary, rollback))
}
```

- [ ] **Step 5: Add `undo_rhythmbox_import` function**

```rust
pub fn undo_rhythmbox_import(
    conn: &mut Connection,
    rollback: &RhythmboxRollback,
) -> Result<usize, RhythmboxImportError> {
    let transaction = conn.transaction()?;
    let mut restored = 0usize;
    for entry in &rollback.entries {
        let affected = transaction.execute(
            "UPDATE tracks SET rating = ?1, play_count = ?2, added_at = ?3, last_played_at = ?4 WHERE path = ?5",
            rusqlite::params![
                entry.rating,
                entry.play_count,
                entry.added_at,
                entry.last_played_at,
                entry.path,
            ],
        )?;
        restored += affected;
    }
    transaction.commit()?;
    Ok(restored)
}
```

- [ ] **Step 6: Update all existing tests that call `merge_stats`**

Every existing test calls `merge_stats(conn, tracks, choices)` → change to `merge_stats(conn, tracks, choices, None)` and destructure the result as `(summary, _rollback)` or just `let (summary, _) = merge_stats(…).unwrap();`.

There are 6 existing tests that call `merge_stats`:
- `merge_preserves_local_rating_and_never_decreases_play_count`
- `merge_imports_missing_rating_and_higher_count_idempotently`
- `merge_respects_choices_and_counts_unmatched_entries`
- `merge_imports_only_an_older_positive_date_added_idempotently`
- `merge_imports_only_a_newer_positive_last_played_idempotently`

For each, change:
```rust
// was:
let summary = merge_stats(&mut conn, &imported, choices).unwrap();
// now:
let (summary, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
```

- [ ] **Step 7: Update call site in `preference_rhythmbox.rs`**

In `PreferencesContext::start_rhythmbox_import`, the `merge_stats` call:

```rust
// was:
rhythmbox_import::merge_stats(&mut conn, &tracks, choices)
// now:
rhythmbox_import::merge_stats(&mut conn, &tracks, choices, None)
    .map(|(summary, _rollback)| summary)
```

Note: we discard the rollback here for now — Task 3 will wire it up in the new dialog. For this task, just make it compile.

- [ ] **Step 8: Run all tests**

Run: `cargo test -p reprise-core rhythmbox_import -- --nocapture 2>&1 | tail -20`
Expected: ALL PASS

Run: `cargo check --workspace 2>&1 | tail -5`
Expected: no errors

- [ ] **Step 9: Commit**

```bash
git add crates/reprise-core/src/library/rhythmbox_import.rs \
       crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs
git commit -m "feat(import): progress callback + rollback support in merge_stats"
```

---

### Task 3: Three-state import dialog (frontend)

**Files:**
- Modify: `crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs`
- Modify: `crates/reprise-gnome/src/ui/strings_rhythmbox.rs`
- Modify: `crates/reprise-gnome/src/ui/preferences/preferences.rs` (update `open_rhythmbox_import`)

**Interfaces:**
- Consumes: `RhythmboxPrescanResult`, `prescan_rhythmdb()`, updated `merge_stats()`, `RhythmboxRollback`, `undo_rhythmbox_import()` from Tasks 1–2
- Produces: `build_import_dialog()`, `open_rhythmbox_import()` (replaces push_import_page) — consumed by Task 4

- [ ] **Step 1: Add all new string constants**

In `strings_rhythmbox.rs`, add:

```rust
pub const RHYTHMBOX_PRESCAN_SCANNING: &str = N_!("Scanning Rhythmbox library…");
pub const RHYTHMBOX_LIBRARY_FOUND: &str = N_!("Rhythmbox library found");
pub const RHYTHMBOX_IMPORT_BODY_RICH: &str =
    N_!("Choose what to copy into Reprise. Rhythmbox and your audio files remain unchanged — you can undo the whole operation.");
pub const RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED: &str = N_!("Play counts & last played");
pub const RHYTHMBOX_IMPORT_COMPLETE_HEADING: &str = N_!("Import complete");
pub const RHYTHMBOX_IMPORTING: &str = N_!("Importing from Rhythmbox…");
pub const RHYTHMBOX_UNDO_IMPORT: &str = N_!("Undo import");
pub const RHYTHMBOX_SKIP_OUTSIDE_LIBRARY: &str = N_!("Files outside your library folder");
pub const RHYTHMBOX_SKIP_MISSING_ON_DISK: &str = N_!("Files no longer on disk");
pub const RHYTHMBOX_SKIP_NON_SONG: &str = N_!("Podcasts & radio streams");
pub const RHYTHMBOX_DONE: &str = N_!("Done");
pub const RHYTHMBOX_CANCEL: &str = N_!("Cancel");

pub fn rhythmbox_entries_matched(matched: usize, total: usize) -> String {
    let matched = matched.to_string();
    let total = total.to_string();
    formatted(
        N_!("{matched} of {total} Rhythmbox entries matched your library"),
        &[("matched", &matched), ("total", &total)],
    )
}

pub fn rhythmbox_entries_skipped(count: usize) -> String {
    let count = count.to_string();
    formatted(
        N_!("{count} entries skipped"),
        &[("count", &count)],
    )
}

pub fn rhythmbox_prescan_info(entries: usize, last_used_days: Option<u64>) -> String {
    let entries = entries.to_string();
    match last_used_days {
        Some(days) => {
            let days = days.to_string();
            formatted(
                N_!("{entries} entries · last used {days} days ago"),
                &[("entries", &entries), ("days", &days)],
            )
        }
        None => formatted(
            N_!("{entries} entries"),
            &[("entries", &entries)],
        ),
    }
}

pub fn rhythmbox_match_count(matched: usize) -> String {
    let matched = matched.to_string();
    formatted(
        N_!("{matched} match your library"),
        &[("matched", &matched)],
    )
}

pub fn rhythmbox_rated_subtitle(count: usize) -> String {
    let count = count.to_string();
    formatted(
        N_!("{count} rated tracks found"),
        &[("count", &count)],
    )
}

pub fn rhythmbox_history_subtitle(count: usize) -> String {
    let count = count.to_string();
    formatted(
        N_!("{count} tracks with history"),
        &[("count", &count)],
    )
}

pub fn rhythmbox_date_added_subtitle() -> String {
    super::text(N_!("Original "added to library" timeline"))
}

pub fn rhythmbox_playlists_subtitle(playlists: usize, tracks: usize) -> String {
    let playlists = playlists.to_string();
    let tracks = tracks.to_string();
    formatted(
        N_!("{playlists} playlists · {tracks} tracks"),
        &[("playlists", &playlists), ("tracks", &tracks)],
    )
}

pub fn rhythmbox_progress_count(done: usize, total: usize) -> String {
    let done = done.to_string();
    let total = total.to_string();
    formatted(
        N_!("{done} of {total} tracks"),
        &[("done", &done), ("total", &total)],
    )
}

pub fn rhythmbox_result_ratings(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} imported"), &[("count", &count)])
}

pub fn rhythmbox_result_play_counts(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} raised"), &[("count", &count)])
}

pub fn rhythmbox_result_dates(dates: usize, last_played: usize) -> String {
    let dates = dates.to_string();
    let last_played = last_played.to_string();
    formatted(
        N_!("{dates} · {last_played} restored"),
        &[("dates", &dates), ("last_played", &last_played)],
    )
}

pub fn rhythmbox_result_playlists(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} created"), &[("count", &count)])
}

pub fn rhythmbox_skipped_warning(count: usize) -> String {
    let count = count.to_string();
    formatted(
        N_!("{count} Rhythmbox entries point to files outside your library folder — they will be skipped."),
        &[("count", &count)],
    )
}
```

- [ ] **Step 2: Run string compilation check**

Run: `cargo check -p reprise-gnome 2>&1 | tail -5`
Expected: compiles (new strings are unused but that's fine — dead_code warning only)

- [ ] **Step 3: Rewrite `preference_rhythmbox.rs` — build three-state dialog**

Replace the entire `build_import_page` function and related types. Keep `build_import_row`, `add_rhythmbox_import_row`, `default_rhythmdb_path`, `default_playlists_path`, `rhythmbox_data_available`, and the test infrastructure. Replace everything related to the old NavigationPage flow.

The new dialog builder:

```rust
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::rhythmbox_import::{
    self, RhythmboxImportChoices, RhythmboxImportSummary, RhythmboxPlaylist,
    RhythmboxPlaylistSummary, RhythmboxPrescanResult, RhythmboxRollback,
    RhythmboxTrackStats,
};

use super::preferences::PreferencesContext;
use super::strings;

// ... keep existing constants RHYTHMDB_PATH_ENV, PLAYLISTS_PATH_ENV, SMOKE_IMPORT_ENV ...
// ... keep existing RhythmboxOption, ImportOptionSpec, import_option_specs, option_title ...
// ... keep existing build_import_row, add_rhythmbox_import_row, default_rhythmdb_path, etc. ...

struct ImportDialogWidgets {
    dialog: adw::Dialog,
    stack: gtk4::Stack,
    // Selection state
    info_subtitle: gtk4::Label,
    match_label: gtk4::Label,
    warning_row: adw::ActionRow,
    import_button: gtk4::Button,
    rows: Vec<adw::SwitchRow>,
    // Progress state
    progress_bar: adw::ProgressBar,    // Note: use gtk4::ProgressBar if adw doesn't exist
    progress_label: gtk4::Label,
    // Complete state
    complete_subtitle: gtk4::Label,
    ratings_result: adw::ActionRow,
    play_counts_result: adw::ActionRow,
    dates_result: adw::ActionRow,
    playlists_result: adw::ActionRow,
    skipped_expander: adw::ExpanderRow,
    skip_outside: adw::ActionRow,
    skip_missing: adw::ActionRow,
    skip_non_song: adw::ActionRow,
    undo_button: gtk4::Button,
    done_button: gtk4::Button,
}

fn build_import_dialog() -> ImportDialogWidgets {
    // === Selection state ===
    let info_icon = gtk4::Image::from_icon_name("emblem-ok-symbolic");
    info_icon.set_pixel_size(24);
    let info_title = gtk4::Label::new(Some(&strings::text(strings::RHYTHMBOX_LIBRARY_FOUND)));
    info_title.add_css_class("heading");
    let info_subtitle = gtk4::Label::new(None);
    info_subtitle.add_css_class("dim-label");
    info_subtitle.set_wrap(true);
    info_subtitle.set_xalign(0.0);
    let match_label = gtk4::Label::new(None);
    match_label.add_css_class("dim-label");
    match_label.set_xalign(0.0);

    let info_text = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    info_text.append(&info_title);
    info_text.append(&info_subtitle);
    info_text.append(&match_label);
    let info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    info_box.append(&info_icon);
    info_box.append(&info_text);
    info_box.set_margin_bottom(12);

    let body_label = gtk4::Label::new(Some(&strings::text(strings::RHYTHMBOX_IMPORT_BODY_RICH)));
    body_label.set_wrap(true);
    body_label.set_xalign(0.0);
    body_label.add_css_class("dim-label");
    body_label.set_margin_bottom(12);

    let options_group = adw::PreferencesGroup::new();
    let specs = import_option_specs();
    let rows: Vec<adw::SwitchRow> = specs
        .into_iter()
        .map(|spec| {
            let row = adw::SwitchRow::builder()
                .title(option_title(spec.id))
                .active(spec.selected)
                .build();
            options_group.add(&row);
            row
        })
        .collect();

    let warning_row = adw::ActionRow::builder()
        .title("")
        .build();
    warning_row.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
    warning_row.set_visible(false);

    let selection_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    selection_box.set_margin_top(18);
    selection_box.set_margin_bottom(18);
    selection_box.set_margin_start(18);
    selection_box.set_margin_end(18);
    selection_box.append(&info_box);
    selection_box.append(&body_label);
    selection_box.append(&options_group);
    selection_box.append(&warning_row);

    // === Progress state ===
    let progress_title = gtk4::Label::new(Some(&strings::text(strings::RHYTHMBOX_IMPORTING)));
    progress_title.add_css_class("title-3");
    let progress_bar = adw::ProgressBar::new();
    let progress_label = gtk4::Label::new(None);
    progress_label.add_css_class("dim-label");

    let progress_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    progress_box.set_margin_top(48);
    progress_box.set_margin_bottom(48);
    progress_box.set_margin_start(24);
    progress_box.set_margin_end(24);
    progress_box.set_valign(gtk4::Align::Center);
    progress_box.append(&progress_title);
    progress_box.append(&progress_bar);
    progress_box.append(&progress_label);

    // === Complete state ===
    let complete_icon = gtk4::Image::from_icon_name("emblem-ok-symbolic");
    complete_icon.set_pixel_size(48);
    complete_icon.set_halign(gtk4::Align::Center);
    complete_icon.set_margin_bottom(12);
    let complete_heading = gtk4::Label::new(Some(
        &strings::text(strings::RHYTHMBOX_IMPORT_COMPLETE_HEADING),
    ));
    complete_heading.add_css_class("title-2");
    complete_heading.set_halign(gtk4::Align::Center);
    let complete_subtitle = gtk4::Label::new(None);
    complete_subtitle.add_css_class("dim-label");
    complete_subtitle.set_halign(gtk4::Align::Center);
    complete_subtitle.set_margin_bottom(18);

    let results_group = adw::PreferencesGroup::new();
    let ratings_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_IMPORT_RATINGS))
        .build();
    let play_counts_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED))
        .build();
    let dates_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_IMPORT_DATE_ADDED))
        .build();
    let playlists_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_IMPORT_PLAYLISTS))
        .build();
    results_group.add(&ratings_result);
    results_group.add(&play_counts_result);
    results_group.add(&dates_result);
    results_group.add(&playlists_result);

    let skipped_expander = adw::ExpanderRow::builder()
        .title("")
        .show_enable_switch(false)
        .build();
    skipped_expander.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
    let skip_outside = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_SKIP_OUTSIDE_LIBRARY))
        .build();
    let skip_missing = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_SKIP_MISSING_ON_DISK))
        .build();
    let skip_non_song = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_SKIP_NON_SONG))
        .build();
    skipped_expander.add_row(&skip_outside);
    skipped_expander.add_row(&skip_missing);
    skipped_expander.add_row(&skip_non_song);
    let skipped_group = adw::PreferencesGroup::new();
    skipped_group.add(&skipped_expander);

    let undo_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_UNDO_IMPORT));
    undo_button.add_css_class("flat");
    undo_button.add_css_class("destructive-action");
    let done_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_DONE));
    done_button.add_css_class("suggested-action");
    let button_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    button_bar.set_halign(gtk4::Align::End);
    button_bar.set_margin_top(18);
    button_bar.append(&undo_button);
    button_bar.append(&done_button);

    let complete_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    complete_box.set_margin_top(24);
    complete_box.set_margin_bottom(18);
    complete_box.set_margin_start(18);
    complete_box.set_margin_end(18);
    complete_box.append(&complete_icon);
    complete_box.append(&complete_heading);
    complete_box.append(&complete_subtitle);
    complete_box.append(&results_group);
    complete_box.append(&skipped_group);
    complete_box.append(&button_bar);

    // === Stack ===
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideLeft);
    stack.add_named(&selection_box, Some("selection"));
    stack.add_named(&progress_box, Some("progress"));
    stack.add_named(&complete_box, Some("complete"));

    // === Dialog ===
    let cancel_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_CANCEL));
    let import_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_IMPORT_START));
    import_button.add_css_class("suggested-action");
    import_button.set_sensitive(false); // enabled after prescan

    let header = adw::HeaderBar::new();
    header.pack_start(&cancel_button);
    header.pack_end(&import_button);
    header.set_show_end_title_buttons(false);
    header.set_show_start_title_buttons(false);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX),
        "",
    )));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(560)
        .build();

    cancel_button.connect_clicked({
        let dialog = dialog.clone();
        move |_| dialog.close()
    });

    ImportDialogWidgets {
        dialog,
        stack,
        info_subtitle,
        match_label,
        warning_row,
        import_button,
        rows,
        progress_bar,
        progress_label,
        complete_subtitle,
        ratings_result,
        play_counts_result,
        dates_result,
        playlists_result,
        skipped_expander,
        skip_outside,
        skip_missing,
        skip_non_song,
        undo_button,
        done_button,
    }
}
```

- [ ] **Step 4: Wire up the prescan + import + complete flow in `PreferencesContext`**

Replace `open_rhythmbox_import` and `start_rhythmbox_import` in `PreferencesContext`:

```rust
impl PreferencesContext {
    pub(super) fn open_rhythmbox_import(self: &Rc<Self>) {
        let widgets = build_import_dialog();
        let rhythmdb_path = default_rhythmdb_path();
        let playlists_path = default_playlists_path(&rhythmdb_path);
        let library_root = {
            let conn = self.conn.borrow();
            reprise_core::library::settings::get_library_root(&conn)
                .ok()
                .flatten()
        };

        // Prescan in background
        let root_clone = library_root.clone();
        let rhythmdb_clone = rhythmdb_path.clone();
        let playlists_clone = playlists_path.clone();
        let conn_for_prescan = {
            // We need a separate read-only connection for prescan since it runs off-main
            // Actually, prescan needs DB access — we pass the conn via the blocking spawn
            let conn = self.conn.borrow();
            let db_path = conn.path().map(|p| p.to_owned());
            db_path
        };

        // For prescan, we open a temporary read-only connection
        let weak = Rc::downgrade(self);
        let info_subtitle = widgets.info_subtitle.clone();
        let match_label = widgets.match_label.clone();
        let warning_row = widgets.warning_row.clone();
        let import_button = widgets.import_button.clone();
        let rows = widgets.rows.clone();
        let prescan_result: Rc<RefCell<Option<RhythmboxPrescanResult>>> =
            Rc::new(RefCell::new(None));
        let prescan_for_import = prescan_result.clone();

        glib::spawn_future_local({
            let prescan_result = prescan_result.clone();
            async move {
                let result = gio::spawn_blocking(move || {
                    // Open a read-only connection for prescan
                    let conn_path = conn_for_prescan.unwrap_or_default();
                    let conn = if conn_path.is_empty() {
                        return Err("no database path".to_string());
                    } else {
                        rusqlite::Connection::open_with_flags(
                            &conn_path,
                            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                        )
                        .map_err(|e| e.to_string())?
                    };
                    rhythmbox_import::prescan_rhythmdb(
                        &rhythmdb_clone,
                        &playlists_clone,
                        &conn,
                        root_clone.as_deref(),
                    )
                    .map_err(|e| e.to_string())
                })
                .await;
                let result = match result {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) | Err(_) => {
                        tracing::warn!(error = %e.unwrap_or_default(), "prescan failed");
                        import_button.set_sensitive(false);
                        return;
                    }
                };

                // Fill in selection state
                let days_ago = result.last_modified.and_then(|m| {
                    m.elapsed().ok().map(|d| d.as_secs() / 86400)
                });
                info_subtitle.set_label(&strings::rhythmbox_prescan_info(
                    result.song_entries,
                    days_ago,
                ));
                match_label.set_label(&strings::rhythmbox_match_count(result.matched));

                // Set subtitles on rows based on prescan
                if rows.len() >= 5 {
                    // Ratings subtitle
                    rows[1].set_subtitle(&strings::rhythmbox_rated_subtitle(
                        result.rated_tracks,
                    ));
                    // Play counts & last played subtitle
                    rows[2].set_subtitle(&strings::rhythmbox_history_subtitle(
                        result.tracks_with_history,
                    ));
                    // Date added subtitle
                    rows[3].set_subtitle(&strings::rhythmbox_date_added_subtitle());
                    // Playlists subtitle
                    rows[4].set_subtitle(&strings::rhythmbox_playlists_subtitle(
                        result.playlist_count,
                        result.playlist_track_count,
                    ));
                }

                let total_skipped =
                    result.outside_library + result.missing_on_disk + result.non_song_entries;
                if total_skipped > 0 {
                    warning_row.set_title(&strings::rhythmbox_skipped_warning(total_skipped));
                    warning_row.set_visible(true);
                }

                import_button.set_sensitive(true);
                *prescan_result.borrow_mut() = Some(result);
                tracing::info!("Rhythmbox prescan complete, dialog ready");
            }
        });

        // Wire import button
        let weak_import = Rc::downgrade(self);
        let stack = widgets.stack.clone();
        let progress_bar = widgets.progress_bar.clone();
        let progress_label = widgets.progress_label.clone();
        let complete_subtitle = widgets.complete_subtitle.clone();
        let ratings_result = widgets.ratings_result.clone();
        let play_counts_result = widgets.play_counts_result.clone();
        let dates_result = widgets.dates_result.clone();
        let playlists_result = widgets.playlists_result.clone();
        let skipped_expander = widgets.skipped_expander.clone();
        let skip_outside = widgets.skip_outside.clone();
        let skip_missing = widgets.skip_missing.clone();
        let skip_non_song = widgets.skip_non_song.clone();
        let undo_button = widgets.undo_button.clone();
        let done_button = widgets.done_button.clone();
        let dialog_for_done = widgets.dialog.clone();
        let rows_for_import = widgets.rows.clone();
        let rollback_holder: Rc<RefCell<Option<RhythmboxRollback>>> =
            Rc::new(RefCell::new(None));

        widgets.import_button.connect_clicked(move |button| {
            let Some(context) = weak_import.upgrade() else { return };
            button.set_sensitive(false);
            stack.set_visible_child_name("progress");

            let column_layout = rows_for_import[0].is_active();
            let choices = RhythmboxImportChoices {
                ratings: rows_for_import[1].is_active(),
                play_counts_and_last_played: rows_for_import[2].is_active(),
                added_at: rows_for_import[3].is_active(),
            };
            let import_playlists = rows_for_import[4].is_active();

            let rhythmdb_path = default_rhythmdb_path();
            let playlists_path = default_playlists_path(&rhythmdb_path);
            let any_stats = choices.ratings
                || choices.play_counts_and_last_played
                || choices.added_at;

            let total_tracks = prescan_for_import
                .borrow()
                .as_ref()
                .map_or(0usize, |r| r.song_entries);

            // Progress update via channel
            let (sender, receiver) = glib::MainContext::channel(glib::Priority::DEFAULT_IDLE);
            let progress_bar_c = progress_bar.clone();
            let progress_label_c = progress_label.clone();
            receiver.attach(None, move |done: usize| {
                let fraction = if total_tracks > 0 {
                    done as f64 / total_tracks as f64
                } else {
                    0.0
                };
                progress_bar_c.set_fraction(fraction.min(1.0));
                progress_label_c.set_label(&strings::rhythmbox_progress_count(
                    done,
                    total_tracks,
                ));
                glib::ControlFlow::Continue
            });

            let stack_c = stack.clone();
            let complete_subtitle_c = complete_subtitle.clone();
            let ratings_result_c = ratings_result.clone();
            let play_counts_result_c = play_counts_result.clone();
            let dates_result_c = dates_result.clone();
            let playlists_result_c = playlists_result.clone();
            let skipped_expander_c = skipped_expander.clone();
            let skip_outside_c = skip_outside.clone();
            let skip_missing_c = skip_missing.clone();
            let skip_non_song_c = skip_non_song.clone();
            let rollback_c = rollback_holder.clone();
            let prescan_for_complete = prescan_for_import.clone();
            let context_c = context.clone();

            glib::spawn_future_local(async move {
                let result = gio::spawn_blocking(move || -> Result<ImportResult, String> {
                    let rhythmdb_path = default_rhythmdb_path();
                    let playlists_path = default_playlists_path(&rhythmdb_path);
                    let tracks = if any_stats {
                        Some(
                            rhythmbox_import::parse_rhythmdb(&rhythmdb_path)
                                .map_err(|e| e.to_string())?,
                        )
                    } else {
                        None
                    };
                    let playlists = import_playlists.then(|| {
                        rhythmbox_import::parse_playlists(&playlists_path)
                            .map_err(|e| e.to_string())
                    });

                    // We need the conn for merge — but we're off-main-thread.
                    // Return parsed data and merge on main thread.
                    Ok(ImportResult {
                        tracks,
                        playlists,
                    })
                })
                .await;

                let parsed = match result {
                    Ok(Ok(parsed)) => parsed,
                    _ => {
                        tracing::warn!("Rhythmbox import parse failed");
                        stack_c.set_visible_child_name("selection");
                        return;
                    }
                };

                // Merge on main thread (we need conn)
                let mut conn = context_c.conn.borrow_mut();
                let stats = parsed.tracks.map(|tracks| {
                    // Progress callback via sender
                    let batch_sender = sender.clone();
                    let batch_counter = Cell::new(0usize);
                    rhythmbox_import::merge_stats(
                        &mut conn,
                        &tracks,
                        choices,
                        Some(&|done| {
                            let prev = batch_counter.get();
                            if done - prev >= 50 || done == tracks.len() {
                                batch_counter.set(done);
                                let _ = batch_sender.send(done);
                            }
                        }),
                    )
                });
                let (summary, rollback) = match stats {
                    Some(Ok((s, r))) => (Some(s), Some(r)),
                    Some(Err(e)) => {
                        tracing::warn!(%e, "merge_stats failed");
                        (None, None)
                    }
                    None => (None, None),
                };

                let (playlist_summary, playlist_error) = match parsed.playlists {
                    Some(Ok(playlists)) => (
                        Some(
                            rhythmbox_import::merge_playlists(&mut conn, &playlists)
                                .map_err(|e| e.to_string()),
                        ),
                        None,
                    ),
                    Some(Err(e)) => (None, Some(e)),
                    None => (None, None),
                };
                drop(conn);

                if column_layout {
                    context_c.import_rhythmbox_column_layout();
                }
                if summary.is_some() {
                    context_c.track_list.reload();
                }
                if playlist_summary
                    .as_ref()
                    .is_some_and(|r| r.is_ok())
                {
                    context_c.sidebar.refresh("Rhythmbox playlist import");
                }

                // Store rollback
                *rollback_c.borrow_mut() = rollback;

                // Fill complete state
                let prescan = prescan_for_complete.borrow();
                let matched = summary.map_or(0, |s| s.matched);
                let total = prescan.as_ref().map_or(0, |p| p.song_entries);
                complete_subtitle_c.set_label(&strings::rhythmbox_entries_matched(matched, total));

                if let Some(s) = &summary {
                    ratings_result_c.set_subtitle(&strings::rhythmbox_result_ratings(s.ratings_imported));
                    play_counts_result_c.set_subtitle(&strings::rhythmbox_result_play_counts(s.play_counts_raised));
                    dates_result_c.set_subtitle(&strings::rhythmbox_result_dates(
                        s.dates_imported,
                        s.last_played_imported,
                    ));
                }
                if let Some(Ok(ps)) = &playlist_summary {
                    playlists_result_c.set_subtitle(&strings::rhythmbox_result_playlists(ps.imported));
                }

                // Skipped breakdown
                let outside = prescan.as_ref().map_or(0, |p| p.outside_library);
                let missing = prescan.as_ref().map_or(0, |p| p.missing_on_disk);
                let non_song = prescan.as_ref().map_or(0, |p| p.non_song_entries);
                let total_skipped = outside + missing + non_song;
                if total_skipped > 0 {
                    skipped_expander_c.set_title(&strings::rhythmbox_entries_skipped(total_skipped));
                    skip_outside_c.set_subtitle(&outside.to_string());
                    skip_missing_c.set_subtitle(&missing.to_string());
                    skip_non_song_c.set_subtitle(&non_song.to_string());
                    skipped_expander_c.set_visible(true);
                } else {
                    skipped_expander_c.set_visible(false);
                }

                tracing::info!(
                    matched,
                    ratings = summary.map_or(0, |s| s.ratings_imported),
                    play_counts = summary.map_or(0, |s| s.play_counts_raised),
                    dates = summary.map_or(0, |s| s.dates_imported),
                    last_played = summary.map_or(0, |s| s.last_played_imported),
                    "Rhythmbox import finished"
                );

                stack_c.set_visible_child_name("complete");
            });
        });

        // Wire undo button
        let rollback_for_undo = rollback_holder.clone();
        let weak_undo = Rc::downgrade(self);
        let dialog_for_undo = widgets.dialog.clone();
        widgets.undo_button.connect_clicked(move |_| {
            let Some(context) = weak_undo.upgrade() else { return };
            let rollback = rollback_for_undo.borrow_mut().take();
            if let Some(rollback) = rollback {
                let mut conn = context.conn.borrow_mut();
                match rhythmbox_import::undo_rhythmbox_import(&mut conn, &rollback) {
                    Ok(restored) => {
                        tracing::info!(restored, "Rhythmbox import undone");
                        drop(conn);
                        context.track_list.reload();
                    }
                    Err(e) => tracing::warn!(%e, "could not undo Rhythmbox import"),
                }
            }
            dialog_for_undo.close();
        });

        // Wire done button
        widgets.done_button.connect_clicked(move |_| {
            dialog_for_done.close();
        });

        // Present
        let parent = self.preferences_parent();
        widgets.dialog.present(Some(&parent));
    }
}
```

- [ ] **Step 5: Update `PreferencesContext::open_rhythmbox_import` in `preferences.rs`**

In `preferences.rs`, the method `open_rhythmbox_import` currently pushes a navigation page:

```rust
pub(super) fn open_rhythmbox_import(self: &Rc<Self>) {
    let navigation = self.preferences_navigation.borrow().upgrade();
    let Some(navigation) = navigation else {
        tracing::warn!("Rhythmbox import requested without preferences navigation");
        return;
    };
    super::preference_rhythmbox::push_import_page(self, &navigation);
}
```

Replace with a direct call to the new dialog builder:

```rust
pub(super) fn open_rhythmbox_import(self: &Rc<Self>) {
    super::preference_rhythmbox::open_import_dialog(self);
}
```

And in `preference_rhythmbox.rs`, expose the entry point:

```rust
pub(super) fn open_import_dialog(context: &Rc<PreferencesContext>) {
    context.open_rhythmbox_import();
}
```

Wait — that's circular. Instead, move the `open_rhythmbox_import` implementation entirely into `preference_rhythmbox.rs` as a free function and call it from `preferences.rs`. Or simply make the dialog-building method on `PreferencesContext` in the `preference_rhythmbox.rs` impl block (which already exists). The current `open_rhythmbox_import` in `preferences.rs` should just call the method on self:

Actually, looking at the existing code, `open_rhythmbox_import` is already defined in `preferences.rs` and calls `push_import_page`. Simply change it to call the new dialog method defined in `preference_rhythmbox.rs`'s impl block. Since the new `open_rhythmbox_import` in `preference_rhythmbox.rs` is defined as `impl PreferencesContext`, it can be called as `self.open_rhythmbox_import_dialog()`. Rename to avoid collision:

In `preference_rhythmbox.rs`, rename the method to `present_rhythmbox_import_dialog`.

In `preferences.rs`:
```rust
pub(super) fn open_rhythmbox_import(self: &Rc<Self>) {
    self.present_rhythmbox_import_dialog();
}
```

- [ ] **Step 6: Remove old `push_import_page` function and `ImportPageSurface` struct**

Delete `push_import_page`, `build_import_page`, `ImportPageSurface`, and the old `start_rhythmbox_import`/`show_rhythmbox_result` methods. Keep `build_import_row`, `add_rhythmbox_import_row`, `import_rhythmbox_column_layout`, and the tests (update them next).

Keep the `ParsedImport` struct (renamed if desired) — it's used internally in `present_rhythmbox_import_dialog` to shuttle parsed data from the blocking thread. The `ImportResult` struct from the old code can be removed.

- [ ] **Step 7: Update tests**

Update `import_row_requires_a_detected_rhythmdb_file` — unchanged, keep as is.

Update `detected_rhythmdb_builds_the_import_row` — unchanged, keep as is.

Update `statistics_are_selected_but_column_layout_requires_opt_in` — already updated in Task 1.

Replace `import_page_exposes_all_five_explicit_choices` with a dialog test:

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn import_dialog_has_three_stack_states_and_five_option_rows() {
    gtk4::init().unwrap();
    let widgets = build_import_dialog();

    // Stack has three children
    assert!(widgets.stack.child_by_name("selection").is_some());
    assert!(widgets.stack.child_by_name("progress").is_some());
    assert!(widgets.stack.child_by_name("complete").is_some());

    // Five option rows with correct titles and defaults
    assert_eq!(widgets.rows.len(), 5);
    assert_eq!(widgets.rows[0].title(), "Column layout");
    assert_eq!(widgets.rows[1].title(), "Ratings");
    assert_eq!(widgets.rows[2].title(), "Play counts & last played");
    assert_eq!(widgets.rows[3].title(), "Date added");
    assert_eq!(widgets.rows[4].title(), "Playlists");
    assert!(!widgets.rows[0].is_active());
    assert!(widgets.rows[1].is_active());
    assert!(widgets.rows[2].is_active());
    assert!(widgets.rows[3].is_active());
    assert!(widgets.rows[4].is_active());

    // Import button starts insensitive (needs prescan)
    assert!(!widgets.import_button.is_sensitive());
}
```

- [ ] **Step 8: Run all tests**

Run: `cargo test -p reprise-gnome preference_rhythmbox -- --nocapture 2>&1 | tail -10`
Expected: non-display tests PASS

Run: `cargo check --workspace 2>&1 | tail -5`
Expected: no errors

- [ ] **Step 9: Commit**

```bash
git add crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs \
       crates/reprise-gnome/src/ui/preferences/preferences.rs \
       crates/reprise-gnome/src/ui/strings_rhythmbox.rs
git commit -m "feat(import): three-state dialog replacing push navigation page"
```

---

### Task 4: Smoke test + cleanup + wiring

**Files:**
- Modify: `crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs` (smoke env handling)
- Modify: `crates/reprise-gnome/src/ui/strings.rs` (remove unused old string if any)

**Interfaces:**
- Consumes: all from Tasks 1–3
- Produces: working end-to-end import dialog accessible from Settings → Library

- [ ] **Step 1: Update smoke test handling**

The existing `SMOKE_IMPORT_ENV` block in `add_rhythmbox_import_row` auto-triggers the import. Update it to open the new dialog instead:

```rust
if std::env::var(SMOKE_IMPORT_ENV).is_ok() {
    let weak = Rc::downgrade(context);
    glib::idle_add_local_once(move || {
        if let Some(context) = weak.upgrade() {
            context.present_rhythmbox_import_dialog();
        }
    });
}
```

- [ ] **Step 2: Update the smoke block in `preferences.rs`**

The existing `smoke.as_deref() == Some("rhythmbox")` switches to the library page. Keep that — it just shows the right page; the user (or smoke env) then clicks the row to open the dialog.

No change needed here.

- [ ] **Step 3: Remove `RHYTHMBOX_IMPORT_LAST_PLAYED` and `RHYTHMBOX_IMPORT_PLAY_COUNTS` from strings if now unused**

Check if `RHYTHMBOX_IMPORT_PLAY_COUNTS` and `RHYTHMBOX_IMPORT_LAST_PLAYED` are still referenced anywhere. If not, remove them from `strings_rhythmbox.rs`. The first-run wizard doesn't use these — it uses `ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT` only. The old import page used them but is now gone.

In `strings_rhythmbox.rs`, remove:
```rust
// Remove these lines:
pub const RHYTHMBOX_IMPORT_PLAY_COUNTS: &str = N_!("Play counts");
pub const RHYTHMBOX_IMPORT_LAST_PLAYED: &str = N_!("Last played");
```

Keep `RHYTHMBOX_IMPORT_RATINGS`, `RHYTHMBOX_IMPORT_DATE_ADDED`, `RHYTHMBOX_IMPORT_PLAYLISTS` — still used in the new dialog result rows.

- [ ] **Step 4: Run full test suite**

Run: `cargo test -p reprise-core rhythmbox_import -- --nocapture 2>&1 | tail -20`
Expected: ALL PASS

Run: `cargo test -p reprise-gnome preference_rhythmbox -- --nocapture 2>&1 | tail -10`
Expected: ALL PASS

Run: `cargo check --workspace 2>&1 | tail -5`
Expected: no errors

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -10`
Expected: no warnings

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs \
       crates/reprise-gnome/src/ui/strings_rhythmbox.rs
git commit -m "feat(import): smoke test update + cleanup unused strings"
```
