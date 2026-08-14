---
slug: android-artist-photos-core
worktree: /home/marvin/Projects/reprise-android-artist-photos-core
branch: feature/android-artist-photos-core
phase: planned
codex_session:
created: 2026-08-14
---
# Android: Künstlerfotos — Strang `core` (PR 1): Kern und Brücke

Strangdatei zu `docs/plans/android-artist-photos.md`. Dort stehen die Bindung an
die Spec, die Regeln für den Umsetzer, die fünf selbstgefällten Entscheidungen,
die Testkommandos und die Merge-Ordnung. Diese Datei wiederholt sie nicht und
enthält nur die Aufgaben.

**Dieser Strang fasst `android/` mit keiner einzigen Zeile an.** Das ist keine
Bequemlichkeit, sondern die Bedingung dafür, dass `dev` zwischen den beiden
Landungen grün bleibt: seit #471 sind die Android-Gates in CI scharf, und ein
neuer `AndroidArtworkSize`-Zweig ohne die drei passenden `when`-Zweige in
`TrackCover.kt` macht den Android-Build rot. Der Enum-Zweig liegt deshalb in
`ui`-Aufgabe 1, zusammen mit den `when`-Zweigen. Alles, was dieser Strang an
`reprise-android-ffi` tut, ist additiv — drei neue Methoden, die bestehendes
Kotlin unverändert übersetzen lassen.

`$LOG` ist ein Arbeitsverzeichnis außerhalb des Repos, etwa
`/tmp/reprise-artist-photos`.

---

## Aufgabe 1 — `ThumbnailSize::MobilePortrait` mit 640 px

**Status:** complete in this task commit. Both named tests were compile-red
before the enum branch existed and passed in
`/tmp/reprise-artist-photos/core-cover.log` after implementation.

**Dateien:** `crates/reprise-core/src/cover.rs:142-168`,
`crates/reprise-core/src/cover_mobile_tests.rs`.

**Änderung:** Ein Zweig `MobilePortrait` im Enum (`cover.rs:143-154`) mit
Doc-Kommentar „A 210 dp Android artist portrait at the measured 3x density" und
`ThumbnailSize::MobilePortrait => 640` in `pixels()` (`cover.rs:157-168`).
`MobileFull` (1092) bleibt unberührt.

**Test zuerst** (`cover_mobile_tests.rs`, neben
`mobile_full_thumbnail_uses_the_measured_three_x_rung:35`):

- `mobile_portrait_thumbnail_is_the_measured_210_dp_rung` —
  `assert_eq!(ThumbnailSize::MobilePortrait.pixels(), 640)`.
- `mobile_portrait_thumbnails_land_in_the_platform_cache_root` — Kopie von
  `source_aware_mobile_thumbnail_uses_the_platform_cache_root:15-33` mit
  `MobilePortrait`; erwartet einen Dateinamen auf `-640.png` unterhalb von
  `<cache_root>/reprise/covers`.

**Fertig, wenn:**
`cargo test --locked -p reprise-core cover > $LOG/core-cover.log 2>&1` und
`grep -c '^test result: FAILED' $LOG/core-cover.log` gibt `0`, und beide neuen
Testnamen stehen im Log.

---

## Aufgabe 2 — `artist_portrait::load_or_fetch_in`

**Status:** complete in this task commit. Both named tests were compile-red
before the explicit-directory entry point existed; the focused portrait suite
then increased from 28 to 30 passing tests in
`/tmp/reprise-artist-photos/core-portrait.log`.

**Dateien:** `crates/reprise-core/src/artist_portrait/mod.rs:47-57`.

**Änderung:**

```rust
pub fn load_or_fetch_in(name: &str, dir: &Path) -> Result<PortraitOutcome, PortraitError>
```

enthält den heutigen Rumpf von `load_or_fetch` ohne die Zeile
`let dir = cache::cache_dir();`. `load_or_fetch` (`mod.rs:47`) schrumpft auf
`load_or_fetch_in(name, &cache::cache_dir())`. An `deezer.rs`, den Fristen
(`cache.rs:11-12`), der Auswahlregel und `MISSING_IMAGE_IDENTIFIERS` ändert sich
nichts.

**Test zuerst** (in `mod.rs`, im bestehenden `mod tests`). Beide Tests kommen
ohne Netz aus, weil sie die zwei Kurzschlüsse in `load_or_fetch_with:92-101`
treffen, und beide benutzen einen pro Lauf einmaligen Namen
(`format!("Reprise Fixture {}", fastrand::u64(..))`), damit die Aussage über den
XDG-Cache belastbar ist:

- `load_or_fetch_in_answers_a_fresh_portrait_from_the_given_directory` —
  `cache::store_image(&dir, &name, b"img", "jpg")`, dann `load_or_fetch_in`
  erwartet `Found(pfad)` mit `pfad.starts_with(&dir)`. Zusätzlich:
  `assert!(cache::portrait_path_in(&cache::cache_dir(), &name).is_none())` —
  das ist die Zusicherung „rührt `dirs::cache_dir()` nicht an".
- `load_or_fetch_in_honours_a_negative_marker_in_the_given_directory` —
  `cache::write_negative(&dir, &name)`, dann `load_or_fetch_in` erwartet
  `NotFound` und
  `assert!(!cache::negative_marker_path(&cache::cache_dir(), &name).exists())`.

**Regressionsschutz für den Desktop** ist, dass der bestehende Testblock
`mod.rs:182-342` **unverändert** bleibt und weiter grün ist — er fährt
`load_or_fetch_with` direkt, also den Rumpf, den beide Einstiegspunkte teilen —
zusammen mit `cache_dir_is_under_xdg_cache_reprise` (`cache.rs:113-115`).

**Fertig, wenn:**
`cargo test --locked -p reprise-core artist_portrait > $LOG/core-portrait.log 2>&1`
grün ist (`grep -c '^test result: FAILED'` gibt `0`) und die Zahl der
ausgeführten Tests in `artist_portrait` um genau 2 gestiegen ist (vorher/nachher
aus derselben Logzeile ablesen).

---

## Aufgabe 3 — Porträtverzeichnis und Beschaffer-Naht auf `MusicLibrary`

**Status:** complete in this task commit. The named path-ownership test was
compile-red on the missing `portrait_dir` seam, then passed in
`/tmp/reprise-artist-photos/ffi-portrait.log`; strict package Clippy also
passed after the shared fetcher type was named.

**Dateien:** `crates/reprise-android-ffi/src/library_types.rs:23-39`,
`crates/reprise-android-ffi/src/lib.rs:81-94`, neu
`crates/reprise-android-ffi/src/artist_portrait.rs`.

**Änderung:**

- `MusicLibrary` bekommt ein Feld
  `portrait_fetch: Arc<dyn Fn(&str, &Path) -> Result<PortraitOutcome, PortraitError> + Send + Sync>`.
  `open` (`lib.rs:81-94`) setzt es auf
  `Arc::new(reprise_core::artist_portrait::load_or_fetch_in)`.
- `#[cfg(test)] pub(crate) fn open_with_portrait_fetch(private, cache, fetch)` —
  derselbe Aufbau, anderer Beschaffer. Das ist dieselbe Naht, die der Desktop mit
  `ArtistPortraitRuntime::for_test`
  (`crates/reprise-gnome/src/ui/now_playing/artist_portrait_worker.rs:73-79`)
  benutzt; ohne sie ist „der Fake-Beschaffer zählt null Aufrufe" nicht messbar.
- `pub(crate) fn portrait_dir(&self) -> PathBuf { self.cache_root.join("artist-portraits") }`.

**Test zuerst:**

- `portraits_live_under_the_app_cache_root_not_the_xdg_cache` — `open` mit einem
  `tempfile::tempdir()`, `portrait_dir()` endet auf `artist-portraits` und liegt
  unter dem übergebenen Cache-Verzeichnis.

**Fertig, wenn:** `TMPDIR=/tmp cargo test --locked -p reprise-android-ffi
artist_portrait > $LOG/ffi-portrait.log 2>&1` grün.

---

## Aufgabe 4 — `artist_portrait_cached`

**Status:** complete in this task commit. All three named tests were
compile-red on the missing cached-only method, then all four focused FFI
portrait tests passed in `/tmp/reprise-artist-photos/ffi-portrait-task4.log`;
strict package Clippy passed as well.

**Dateien:** `crates/reprise-android-ffi/src/artist_portrait.rs`,
`crates/reprise-android-ffi/src/lib.rs` (nur `mod artist_portrait;` und
`pub use`).

**Änderung:**

```rust
pub fn artist_portrait_cached(&self, name: &str, size: AndroidArtworkSize) -> Option<String>
```

`load_cached_from(name, &self.portrait_dir())` → bei `Found(pfad)`
`cover::thumbnail_with_source(&UnixLibrarySource, &CoverSource::FolderImage(pfad),
size.thumbnail_size(), &self.cache_root)`; Fehler werden wie in
`lib.rs:263-272` protokolliert und als `None` beantwortet.

Zwei Punkte, die leicht falsch gemacht werden:

- Die Quelle ist `reprise_core::library::source::UnixLibrarySource`
  (`crates/reprise-core/src/library/source.rs:364`), **nicht** die
  `BridgedSource` des konfigurierten Baums. Porträts liegen im App-Cache, nicht
  im SAF-Baum. Deshalb braucht diese Methode auch keinen konfigurierten Baum und
  darf nie `TreeNotConfigured` melden.
- Sie fasst `self.state` nicht an. Das ist kein Zufall, sondern der Beweis, dass
  sie nichts ins Netz schicken kann: sie kennt das Gate gar nicht.

**Test zuerst:**

- `cached_portraits_never_call_the_fetcher_even_with_the_gate_open` — Gate über
  `online_sources::set_enabled(true)` + `modules::set_enabled(ARTWORK_MODULE,
  true)` öffnen, zählender Fake-Beschaffer, `artist_portrait_cached` aufrufen,
  Zähler `== 0`.
- `cached_portrait_returns_the_reduced_file_not_the_original` — ein echtes
  kleines PNG (wie `artwork_tests.rs:11-17`) über `cache::store_image` ins
  Porträtverzeichnis legen; der zurückgegebene Pfad liegt unter
  `<cache_root>/reprise/covers`, endet auf `-168.png` und ist **nicht** der
  abgelegte Pfad.
- `a_portrait_that_was_never_downloaded_is_none` — leeres Verzeichnis, `None`,
  kein Fehler.

**Fertig, wenn:** derselbe Befehl wie in Aufgabe 3, jetzt mit drei Tests mehr.

---

## Aufgabe 5 — `artist_portrait_fetch` mit dem NET-1a-Gate

**Status:** complete in this task commit. All three named tests were
compile-red on the missing fetch method, then the seven focused portrait tests
and the complete 152-test FFI suite passed in
`/tmp/reprise-artist-photos/ffi-portrait-task5.log` and `ffi-task5.log`; the
query-lock test proves the database guard is released before the fetcher runs.

**Dateien:** `crates/reprise-android-ffi/src/artist_portrait.rs`.

**Änderung:**

```rust
pub fn artist_portrait_fetch(&self, name: &str, size: AndroidArtworkSize)
    -> Result<Option<String>, LibraryError>
```

Ablauf, in dieser Reihenfolge:

1. `let allowed = { let state = self.lock()?;
   reprise_core::online_sources::network_allowed_or_off(&state.db,
   &reprise_core::modules::ARTWORK_MODULE) };` — **die Sperre wird hier wieder
   abgegeben.** Dieselbe Kombination benutzt der Desktop-Worker
   (`artist_portrait_worker.rs:33-36`).
2. `if !allowed { return Ok(None); }` — vor dem Beschaffer, vor jedem
   Dateizugriff.
3. `(self.portrait_fetch)(name, &self.portrait_dir())`, blockierend. Kotlin ruft
   das auf einem Worker-Thread; wer die Sperre hier noch hielte, würde jede
   Abfrage der Bibliothek hinter eine Netzanfrage stellen.
4. Bei `Found(pfad)` verkleinern wie in Aufgabe 4; bei `NotFound` oder `Err`
   `Ok(None)` mit `tracing::debug!`.

**Test zuerst.** Es sind hier **drei** Tests, nicht vier: der vierte Test des
Entwurfs, `a_portrait_is_never_requested_at_the_now_playing_rung`, benutzt
`AndroidArtworkSize::ArtistDetail` und kann in diesem PR nicht existieren, weil
der Enum-Zweig erst in PR 2 entsteht. Er steht in `ui`-Aufgabe 1 und ist dort
aufgeführt — er ist verschoben, nicht gestrichen. Die drei Tests hier fahren
deshalb ausschließlich auf den bestehenden Stufen `List` und `NowPlaying`:

- `net_1a_a_closed_gate_never_calls_the_fetcher_and_writes_no_file` — frische
  Datenbank (Gate ist dort per Voreinstellung zu, `online_sources.rs:25-28`),
  zählender Fake, Ergebnis `Ok(None)`, Zähler `== 0`, und
  `assert!(!library.portrait_dir().exists())`.
- `net_1a_an_open_gate_calls_the_fetcher_once_and_returns_the_reduced_file` —
  Gate öffnen, Fake legt ein PNG über `cache::store_image` ab und meldet
  `Found`; Zähler `== 1`, Pfad endet auf `-168.png`.
- `the_query_lock_is_free_while_a_portrait_is_being_fetched` — der Fake blockiert
  auf einem `mpsc`-Kanal; ein zweiter Thread (`std::thread::scope`) ruft
  `library.appearance_settings()` und muss innerhalb von 2 s antworten
  (`recv_timeout`), dann wird der Fake freigegeben. Ohne Punkt 1 oben hängt
  dieser Test.

**Fertig, wenn:** `TMPDIR=/tmp cargo test --locked -p reprise-android-ffi >
$LOG/ffi.log 2>&1` grün und alle drei Namen im Log stehen.

---

## Aufgabe 6 — `online_sources_enabled` / `set_online_sources_enabled`

**Status:** complete in this task commit. All four named tests were
compile-red on the missing switch methods, then passed in
`/tmp/reprise-artist-photos/ffi-online-sources.log`; the complete 156-test FFI
suite and strict package Clippy also passed. The global gate is deliberately
written before the Artwork module flag so first-enable seeding cannot undo it.

**Dateien:** neu `crates/reprise-android-ffi/src/online_sources.rs`,
`crates/reprise-android-ffi/src/lib.rs` (`mod` + `pub use`).

**Änderung:** Muster `appearance.rs:156-228`.

- `pub fn online_sources_enabled(&self) -> Result<bool, LibraryError>` →
  `online_sources::network_allowed(&state.db, &modules::ARTWORK_MODULE)`. Nicht
  die beiden Schlüssel einzeln lesen und selbst verunden — `network_allowed`
  ist die eine Autorität dafür (`online_sources.rs:86-100`).
- `pub fn set_online_sources_enabled(&self, value: bool) -> Result<(), LibraryError>`
  → **erst** `online_sources::set_enabled(&state.db, value)`, **dann**
  `modules::set_enabled(&state.db, &modules::ARTWORK_MODULE, value)`.

Die Reihenfolge ist die eigentliche Arbeit dieser Aufgabe. `set_enabled(true)`
fährt auf einer noch unentschiedenen Datenbank die Erstfreigabe und schreibt
dabei `module.artwork.enabled = false` (`online_sources.rs:73-83`). Wer den
Modulschlüssel zuerst setzt, sieht ihn danach wieder auf `false` — der Schalter
ließe sich nicht einschalten.

**Test zuerst:**

- `the_switch_is_off_on_a_fresh_database`
- `switching_on_survives_the_first_enable_seed` — auf frischer Datenbank
  `set_online_sources_enabled(true)`, dann `online_sources_enabled() == true`
  **und** beide Kernschlüssel einzeln geprüft (`online_sources::is_enabled`,
  `modules::is_enabled(ARTWORK_MODULE)`). Dieser Test ist rot, wenn die
  Reihenfolge falsch ist.
- `switching_off_closes_the_gate_for_fetches` — Fake-Beschaffer aus Aufgabe 5:
  an → `artist_portrait_fetch` → Zähler 1; aus → `artist_portrait_fetch` →
  Zähler weiterhin 1.
- `an_off_and_on_cycle_leaves_the_switch_on` — an, aus, an; danach `true`.

**Fertig, wenn:** `TMPDIR=/tmp cargo test --locked -p reprise-android-ffi` grün.

---

## Aufgabe 7 — Volles Rust-Gate

**Dateien:** keine.

**Fertig, wenn** diese drei Läufe grün sind und die Logs im Protokoll stehen:

```sh
cargo test --locked -p reprise-core > $LOG/core.log 2>&1
grep -c '^test result: FAILED' $LOG/core.log         # muss 0 sein

TMPDIR=/tmp cargo test --locked -p reprise-android-ffi > $LOG/ffi.log 2>&1
grep -c '^test result: FAILED' $LOG/ffi.log          # muss 0 sein

cargo test --locked -p reprise-gnome --bin reprise > $LOG/gnome.log 2>&1
grep -c '^test result: FAILED' $LOG/gnome.log        # muss 0 sein
```

Der dritte Lauf ist der wichtige und steht bewusst hier statt in der
Post-Merge-Liste: `reprise-gnome` gehört keinem Strang, aber dieser Strang läuft
sequenziell allein im Baum, kann den Lauf also fahren, ohne dass ihn ein
gleichzeitiger zweiter Strang unter der Hand rot färbt — und er muss ihn fahren,
weil der Desktop-Porträt-Worker
(`crates/reprise-gnome/src/ui/now_playing/artist_portrait_worker.rs`)
`load_or_fetch` ruft, das Aufgabe 2 umbaut. Eine Regression, die dieser Strang
erzeugt, darf nicht erst nach dem Merge auffallen.

Dazu die Formalitäten: `cargo fmt --all`, `cargo clippy --all-targets
--all-features -- -D warnings`.

**Nicht in diesem Strang:** kein Gradle-Lauf, keine Android-Suite, keine
Binding-Erzeugung. `android/` bleibt unberührt; dass der Android-Build danach
weiter grün ist, prüft der Sperrpunkt im Mutterplan nach dem Merge auf `dev`.
