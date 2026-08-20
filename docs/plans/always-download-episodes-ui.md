---
slug: always-download-episodes-ui
worktree: /home/marvin/Projects/reprise-always-download-episodes-ui
branch: feature/always-download-episodes-ui
phase: planned
codex_session:
created: 2026-08-20
---
# Episoden immer herunterladen statt streamen — Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine YouTube-Episode wird nie mehr gestreamt, sondern vor der Wiedergabe heruntergeladen, und jedes Abo hält seine neuesten `keep_downloaded` (Standard 10) Folgen auf der Platte.

**Architecture:** Die vorhandene Zahl `keep_downloaded` wird beidseitig: ein neuer Auffüller in `podcasts::pipeline` lädt, was unter den neuesten N fehlt, während `cleanup_candidates` weiter löscht, was darüber hinausgeht — beide über *eine* gemeinsame Sortiervorschrift, aber über verschiedene Grundmengen. Der Auffüller läuft als eigene Worker-Operation nach dem Refresh, nicht in ihm. Der Wiedergabepfad ersetzt `resolve_youtube` (Stream-Proxy) durch einen Download auf einem `one_shot_task`, an dessen Abschluss die lokale Datei abgespielt wird.

**Tech Stack:** Rust, rusqlite/SQLite, gtk4-rs/libadwaita, glib-Mainloop, `async_channel`.

Bindende Quelle ist
`docs/superpowers/specs/2026-08-20-always-download-episodes-design.md`. Dieser
Plan wiederholt sie nicht, er ergänzt sie um das *Wie*, die Reihenfolge und die
Abnahme. Wo Spec und Plan sich widersprechen, gilt die Spec.

Gelesen gegen `origin/dev` @ `afb839069e`. Jede Zeilenangabe stammt aus diesem
Stand; wer sie nicht wiederfindet, hat eine andere Basis.

## Global Constraints

- `DEFAULT_KEEP_DOWNLOADED` ist nach diesem Plan `10` (vorher `5`).
- `0` bedeutet bei jeder numerischen Mengeneinstellung **unbegrenzt** (`E-9`) —
  beim Auffüller heißt das: alle Episoden des Abos, nicht „keine".
- Die Sortiervorschrift für „neueste Episode zuerst" existiert genau einmal als
  Konstante und wird von Auffüller und Aufräumer benutzt.
- Kein zweiter Download-Executor: jeder Downloadweg geht durch
  `podcasts::pipeline::download_episode_in`.
- Dateien bleiben unter 800 Zeilen (`check-architecture.sh` erzwingt das).
- Chat/Antworten deutsch, alles im Repo (Code, Kommentare, Commit-Botschaften,
  Testnamen) englisch.
- Commit-Format: `<type>: <description>`, Typen `feat|fix|refactor|docs|test|chore|perf|ci`.

## Regeln für den Umsetzer — zuerst lesen

- **Die `Files:`-Liste je Aufgabe ist ein Startpunkt, kein Zaun.** Wenn der
  Vertrag einer Aufgabe eine hier nicht genannte Datei braucht, fass sie an und
  notier es. Halte nur an, wenn der *Vertrag selbst* falsch ist.
- **Jede Aufgabe endet grün und committet.** `cargo test -p <crate>` für den
  berührten Crate, `cargo clippy --all-targets -- -D warnings`.
- **TDD ist bindend:** erst der Test, dann der Lauf, der ihn scheitern sieht,
  dann die Implementierung. Ein Test, der ohne die Implementierung besteht,
  misst nichts und ist zu verwerfen.

---

## Datei-Eigentum dieses Strangs

Dieser Strang gehört `crates/reprise-gnome/**`, `docs/ux-rules.md`, dem
Mutterplan `docs/plans/always-download-episodes.md` und dieser Plandatei.

**Der Strang sitzt auf `feature/always-download-episodes-core` auf**, nicht auf
`dev`. Die Aufgaben 1–6 (Strang `core`) sind in dieser Historie bereits erledigt:
`podcasts::fill_downloads::fill_downloads`, `FillSummary`,
`podcasts::pipeline::download_episode`, die Download-Claims und die
fehlerspezifischen yt-dlp-Meldungen existieren. Lies sie im Quelltext nach,
statt sie neu zu erfinden, und **ändere nichts unter `crates/reprise-core/**`** —
was dort steht, ist die abgenommene Grundlage dieses Strangs. Fällt dort ein
echter Fehler auf, melde ihn im Abschlussbericht, statt ihn hier zu reparieren.

## Was der `core`-Strang hinterlassen hat

- `crates/reprise-gnome` kompiliert weiterhin; es gibt keine gebrochene
  Signatur. Was es gibt, ist totes und falsches Verhalten:
- `podcasts_worker.rs:257` gibt dem Refresh weiter einen
  Download-Fortschritts-Callback mit, obwohl der Refresh nichts mehr lädt. Der
  Kommentar bei `:328` über Auto-Download ist veraltet.
- `add_dialog_tests.rs:655` — `pod_13_preview_error_never_forwards_a_leaking_payload`
  erwartet die alte generische yt-dlp-Meldung und **schlägt fehl**, bis dieser
  Strang ihn auf die neue, reparaturspezifische Meldung zieht.
- Tote Auto-Download-Bedienelemente: `preference_podcasts.rs:114`,
  `add_dialog_rows.rs:42`, `add_dialog_subscription.rs:27` und die
  Weiterreichung in `add_dialog.rs:365`. Der Kern liest das gespeicherte
  Abo-Flag nicht mehr. Task 9 räumt genau das ab.

## Strang `ui` — `reprise-gnome`

### Task 7: Der Auffüller als Worker-Operation

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs:10-35, 92-102, 250-302`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs` (Anstoß nach dem Refresh)
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_worker_tests.rs`

**Interfaces:**
- Consumes: `podcasts::fill_downloads::fill_downloads` (Task 4).
- Produces: `PodcastsOperation::FillDownloads`, `PodcastsWorkerResult::Filled(FillSummary)`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_fill_downloads_request_does_not_cancel_a_running_refresh() {
    // Same non-cancelling treatment `Download` has: the fill-up runs for
    // minutes and must never invalidate a refresh, nor be invalidated by one.
    let current = 7;
    assert_eq!(
        request_generation(current, PodcastsOperation::FillDownloads),
        current
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-gnome a_fill_downloads_request_does_not_cancel_a_running_refresh`
Expected: FAIL, `no variant named `FillDownloads``

- [ ] **Step 3: Add the operation**

In `podcasts_worker.rs`:

```rust
pub(in crate::ui) enum PodcastsOperation {
    Refresh {
        policy: podcasts::refresh::RefreshPolicy,
        kind: Option<podcasts::PodcastKind>,
    },
    LoadMore {
        subscription_id: i64,
        end: usize,
    },
    Download {
        episode_id: i64,
    },
    /// Brings every subscription up to its `keep_downloaded` target. Runs
    /// after a refresh rather than inside it: the first run after this feature
    /// lands has a whole library's backlog to fetch, and a refresh that blocks
    /// for that long looks hung.
    FillDownloads,
}
```

im `request_generation`-`match`:

```rust
        PodcastsOperation::Download { .. } | PodcastsOperation::FillDownloads => current,
```

im `PodcastsWorkerResult`:

```rust
    Filled(podcasts::fill_downloads::FillSummary),
```

in `send_response`s `terminal`-Berechnung wird `Filled` wie `Refreshed`
behandelt:

```rust
    let terminal = match &result {
        Err(_)
        | Ok(
            PodcastsWorkerResult::Refreshed(_)
            | PodcastsWorkerResult::LoadedMore { .. }
            | PodcastsWorkerResult::Filled(_),
        ) => true,
```

und im Operations-`match`:

```rust
        PodcastsOperation::FillDownloads => {
            let result = podcasts::config::load(conn)
                .map_err(|error| error.to_string())
                .and_then(|config| {
                    let ytdlp = podcasts::ytdlp::YtDlp::discover_with_browser(
                        config.ytdlp_path.as_deref(),
                        config.youtube_browser,
                    );
                    podcasts::fill_downloads::fill_downloads(
                        conn,
                        &podcasts::pipeline::HttpFeedFetcher,
                        &ytdlp,
                        &podcasts::downloads::default_download_root(),
                        &mut |episode_id, state| {
                            send_response(
                                request,
                                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
                            );
                        },
                    )
                    .map(PodcastsWorkerResult::Filled)
                    .map_err(|error| error.to_string())
                });
            send_response(request, result);
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p reprise-gnome a_fill_downloads_request_does_not_cancel_a_running_refresh`
Expected: PASS

- [ ] **Step 5: Dispatch it after a refresh**

In `podcasts_view.rs` dort, wo `PodcastsWorkerResult::Refreshed(_)` verarbeitet
wird, im Anschluss `PodcastsOperation::FillDownloads` in Auftrag geben — über
denselben Weg, den `dispatch_download` benutzt (`podcasts_view.rs:544`). Die
eintreffenden `DownloadState`-Antworten fließen in dieselbe
`download_states`-Karte wie beim Knopf-Download, damit die Zeilen ihren
Fortschritt zeigen.

Ein zweiter Auffüller darf nicht neben einem laufenden starten: dieselbe
`Cell<bool>`-Wache, die die Ansicht für andere laufende Operationen benutzt,
oder eine neue mit demselben Muster.

- [ ] **Step 6: Run the crate's tests**

Run: `cargo test -p reprise-gnome podcasts`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs \
        crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs \
        crates/reprise-gnome/src/ui/podcasts/podcasts_worker_tests.rs
git commit -m "feat: run the download fill-up after every refresh"
```

---

### Task 8: Wiedergabe lädt herunter statt zu streamen

**Files:**
- Modify: `crates/reprise-gnome/src/ui/playback/external_media.rs:280-376`
- Modify: `crates/reprise-gnome/src/ui/playback/external_media_state.rs:36-42`
- Test: `crates/reprise-gnome/src/ui/playback/external_media_state_tests.rs`

**Interfaces:**
- Consumes: `podcasts::pipeline::download_episode` (Task 3),
  `one_shot_task::spawn_with_progress`, `store::episode`.
- Produces: `resolve_youtube` heißt `fetch_youtube` und spielt eine lokale Datei
  ab; `stream_proxy` wird vom Wiedergabepfad nicht mehr betreten. Neu und rein:
  ```rust
  pub(super) enum FetchOutcome { Play(String), Fail(String) }
  pub(super) fn fetch_outcome(
      result: Result<(), String>,
      downloaded_path: Option<String>,
  ) -> FetchOutcome
  ```

**Zur Testbarkeit — vorher lesen.** `reprise-gnome` hat **keinen** Fake-Player
und keinen `PlayerController`-Test; die vorhandenen Wiedergabetests
(`external_media_state_tests.rs`, `external_media_state_queue_tests.rs`) prüfen
reine Zustandsfunktionen. Einen Controller-Prüfstand zu bauen ist ein eigener
Umbau und gehört nicht in diese Aufgabe. Deshalb wird die **Entscheidung** in
eine reine Funktion gezogen und dort getestet; dass die glib-Verdrahtung darum
herum stimmt, belegt Task 10 an der laufenden Anwendung. Wer hier einen
Controller-Test erzwingt, baut das größere Ding — halt an und sag Bescheid.

- [ ] **Step 1: Write the failing test**

In `external_media_state_tests.rs`:

```rust
use super::external_media_state::{fetch_outcome, FetchOutcome};

#[test]
fn a_finished_fetch_plays_the_file_the_download_wrote() {
    let outcome = fetch_outcome(Ok(()), Some("/music/episode.opus".into()));
    assert_eq!(outcome, FetchOutcome::Play("/music/episode.opus".into()));
}

#[test]
fn a_failed_fetch_carries_the_downloads_own_message() {
    // Task 5 makes this message specific; the playback path must not replace
    // it with one of its own, because it is now the reason nothing plays.
    let outcome = fetch_outcome(
        Err("YouTube changed its response — update yt-dlp and try again".into()),
        None,
    );
    assert_eq!(
        outcome,
        FetchOutcome::Fail("YouTube changed its response — update yt-dlp and try again".into())
    );
}

#[test]
fn a_finished_fetch_without_a_file_fails_rather_than_playing_nothing() {
    // `persist_download` writing the row is what makes an episode locally
    // available. A success with no row is a bug, not a playable state.
    let outcome = fetch_outcome(Ok(()), None);
    assert!(matches!(outcome, FetchOutcome::Fail(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-gnome fetch_outcome`
Expected: FAIL, `cannot find function `fetch_outcome``

- [ ] **Step 2b: Write the pure decision**

In `external_media_state.rs`, neben `podcast_source_requires_resolution`:

```rust
/// What to do once a fetch finishes.
///
/// Pure on purpose: the glib wiring around it cannot be unit-tested in this
/// crate (no fake player exists), so the decision it makes is kept where a
/// test can reach it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FetchOutcome {
    Play(String),
    Fail(String),
}

pub(super) fn fetch_outcome(
    result: Result<(), String>,
    downloaded_path: Option<String>,
) -> FetchOutcome {
    match (result, downloaded_path) {
        (Ok(()), Some(path)) => FetchOutcome::Play(path),
        (Ok(()), None) => {
            FetchOutcome::Fail("the episode reported a finished download but no file".into())
        }
        (Err(message), _) => FetchOutcome::Fail(message),
    }
}
```

Run: `cargo test -p reprise-gnome fetch_outcome`
Expected: PASS, 3 Tests.

- [ ] **Step 3: Replace the resolve with a fetch**

In `external_media.rs` `resolve_youtube` (Zeile 284) durch `fetch_youtube`
ersetzen. Der Rumpf, an derselben Stelle in `begin_podcast` aufgerufen:

```rust
    /// Fetches the episode, then plays it from disk.
    ///
    /// Replaces the streaming path: an episode is played from a local file or
    /// not at all. The download runs on a named background thread the same way
    /// the resolve used to, and its progress drives the session's phase, so the
    /// player bar shows "fetching" rather than a dead zero.
    fn fetch_youtube(
        self: &Rc<Self>,
        generation: u64,
        episode_id: i64,
        resume_ms: i64,
    ) {
        let db = self.conn.clone();
        let task = crate::ui::one_shot_task::spawn_with_progress(
            "reprise-youtube-fetch",
            move |publish| {
                let config = reprise_core::podcasts::config::load(&db)
                    .map_err(|error| error.to_string())?;
                let ytdlp = reprise_core::podcasts::ytdlp::YtDlp::discover_with_browser(
                    config.ytdlp_path.as_deref(),
                    config.youtube_browser,
                );
                reprise_core::podcasts::pipeline::download_episode(
                    &db,
                    &reprise_core::podcasts::pipeline::HttpFeedFetcher,
                    &ytdlp,
                    &reprise_core::podcasts::downloads::default_download_root(),
                    episode_id,
                    &mut |state| publish(state),
                )
                .map_err(|error| error.to_string())
            },
        );
        let (progress, result) = match task {
            Ok(pair) => pair,
            Err(error) => {
                self.fail_podcast(generation, &error.to_string());
                return;
            }
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            while let Ok(state) = progress.recv().await {
                let Some(controller) = weak.upgrade() else {
                    return;
                };
                if !controller.external_generation_matches_podcast(generation) {
                    return;
                }
                controller.update_podcast_fetch_progress(generation, &state);
            }
        });
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let Ok(result) = result.recv().await else {
                return;
            };
            let Some(controller) = weak.upgrade() else {
                return;
            };
            if !controller.external_generation_matches_podcast(generation) {
                return;
            }
            // The path comes from the row the download just wrote, not from the
            // download's return value: `persist_download` is what makes an
            // episode locally available, so reading the row is what proves it.
            let path = reprise_core::podcasts::store::episode(&controller.conn, episode_id)
                .ok()
                .flatten()
                .and_then(|episode| episode.downloaded_path);
            match fetch_outcome(result.map(|_| ()), path) {
                FetchOutcome::Play(path) => {
                    let _ = controller.start_podcast_source(
                        generation,
                        episode_id,
                        EpisodeSource::File(path),
                        resume_ms,
                    );
                }
                FetchOutcome::Fail(message) => controller.fail_podcast(generation, &message),
            }
        });
    }
```

`update_podcast_fetch_progress` ist neu und klein: es hält die Sitzung in
`PodcastPhase::Resolving` und reicht `DownloadState::Downloading { received, total }`
an die vorhandene Fortschrittsanzeige weiter. Wenn die Wiedergabeleiste heute
keinen Ladefortschritt kennt, genügt für diese Aufgabe, dass die Phase steht —
dann ist die Funktion ein `tracing::debug!` und ein `notify_external_changed()`.

In `begin_podcast` (Zeile 275) den Aufruf ersetzen:

```rust
        if needs_ytdlp {
            self.fetch_youtube(generation, episode_id, resume_ms);
            return Ok(());
        }
```

Das `source`-Argument entfällt: der Download braucht die `episode_id`, nicht die
Video-URL. Die `use`-Zeilen für `stream_proxy` und `YtDlp` in dieser Datei
fallen weg, sofern sie nichts anderes mehr benutzt — der Compiler sagt es.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p reprise-gnome playback`
Expected: PASS

- [ ] **Step 5: Prove the tests are load-bearing**

Dreh in `fetch_outcome` den `(Ok(()), None)`-Arm auf
`FetchOutcome::Play(String::new())`, lauf
`a_finished_fetch_without_a_file_fails_rather_than_playing_nothing`, sieh ihn
scheitern, mach es rückgängig.

- [ ] **Step 5b: Prove the streaming path is gone**

Run: `grep -n "stream_proxy" crates/reprise-gnome/src/ui/playback/external_media.rs`
Expected: keine Treffer. Solange dort noch einer steht, kann die Wiedergabe
weiterhin streamen, und kein Test in diesem Crate würde es merken.

- [ ] **Step 6: Check the file's length**

Run: `wc -l crates/reprise-gnome/src/ui/playback/external_media.rs`
Expected: unter 800. Wenn nicht, wandert `fetch_youtube` in ein neues
`external_media_fetch.rs` — die Datei ist ohnehin die richtige Grenze für diese
Verantwortung.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/playback/
git commit -m "feat: download a youtube episode before playing it"
```

---

### Task 9: Die Schalter, die nichts mehr tun, verschwinden

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/add_dialog.rs` (Auto-Download-Zeile)
- Modify: `crates/reprise-gnome/src/ui/podcasts/add_dialog_subscription.rs:27-30`
- Modify: `crates/reprise-gnome/src/ui/preferences/preference_podcasts.rs`
- Modify: `docs/ux-rules.md`
- Test: `crates/reprise-gnome/src/ui/podcasts/add_dialog_tests.rs`

**Interfaces:**
- Consumes: nichts.
- Produces: keine UI mehr für `auto_download` / `auto_download_default`.

Die Spalte `podcast_subscriptions.auto_download` **bleibt**; sie wird nur von
niemandem mehr gelesen. Das Fallenlassen der Spalte ist laut Spec ausdrücklich
außerhalb dieses Plans.

- [ ] **Step 1: Find every switch**

Run: `grep -rn "auto_download" crates/reprise-gnome/src --include='*.rs' | grep -v _tests`
Expected: die Zeilen im Abo-Dialog und in den Einstellungen. Jede davon ist eine
Bedienfläche, die nach diesem Plan nichts mehr bewirkt.

- [ ] **Step 2: Write the failing test**

In `add_dialog_tests.rs`:

```rust
#[test]
fn the_add_dialog_no_longer_offers_an_auto_download_switch() {
    let rows = add_dialog_row_titles();
    assert!(
        !rows.iter().any(|title| title.contains("utomatisch")
            || title.to_lowercase().contains("download")),
        "a switch that changes nothing must not be shown: {rows:?}"
    );
}
```

Die vorhandene Abfrage der Dialogzeilen benutzen:
`grep -n "fn add_dialog_row_titles\|row_titles" crates/reprise-gnome/src/ui/podcasts/add_dialog_tests.rs`.
Gibt es sie nicht, prüf stattdessen, dass die Konstruktionsfunktion des Dialogs
keine `adw::SwitchRow` für Auto-Download mehr baut.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p reprise-gnome the_add_dialog_no_longer_offers_an_auto_download_switch`
Expected: FAIL

- [ ] **Step 4: Remove the switches**

Die Zeile aus dem Abo-Dialog und aus den Podcast-Einstellungen entfernen, samt
`configured_auto_download_default()` und dem Schreiben von
`AUTO_DOWNLOAD_DEFAULT_KEY`. Neue Abos setzen die Spalte auf ihren
Vorgabewert; nichts liest ihn.

- [ ] **Step 5: Update the UX rules**

`docs/ux-rules.md` nach Auto-Download durchsuchen und die verwaiste Regel
streichen oder auf die neue Regel umschreiben: „Die neuesten `keep_downloaded`
Folgen jedes Abos liegen auf der Platte; Wiedergabe erfolgt immer lokal."

Run: `grep -rn -i "auto.download\|automatisch herunterladen" docs/ux-rules.md`
Expected: nach der Änderung keine Treffer, die den entfernten Schalter meinen.

- [ ] **Step 6: Run the full suite**

Run: `cargo test -p reprise-gnome && cargo clippy --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/ \
        crates/reprise-gnome/src/ui/preferences/preference_podcasts.rs \
        docs/ux-rules.md
git commit -m "refactor: drop the auto-download switches the pipeline no longer reads"
```

---

### Task 10: Abnahme am laufenden Programm

**Files:**
- Modify: keine — dies ist der Nachweis, nicht der Umbau.

**Interfaces:**
- Consumes: alles Vorstehende.
- Produces: das Protokoll, das an den Plan gehängt wird.

Ein grüner Testlauf belegt hier nicht genug: die tragende Behauptung ist, dass
Auffüller und Aufräumer **gemeinsam** konvergieren, und dass eine Episode
tatsächlich von der Platte spielt.

- [ ] **Step 1: Run the whole suite**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: PASS. Ausgabe nach `$SCRATCH/suite.log` umleiten und nur die
Zusammenfassung lesen.

- [ ] **Step 2: Verify convergence against a real database**

Auf einer **Kopie** der echten Datenbank (samt `-wal`, sonst fehlen die
jüngsten Schreibvorgänge) zweimal hintereinander auffüllen und aufräumen. Beim
zweiten Lauf muss `FillSummary::default()` herauskommen und
`cleanup_candidates` leer sein.

- [ ] **Step 3: Verify playback comes from disk**

Die Anwendung starten, eine YouTube-Episode ohne Datei abspielen. Erwartet:
die Zeile zeigt Ladefortschritt, danach beginnt die Wiedergabe, und
`podcast_episodes.downloaded_path` ist für diese Episode gesetzt. Belegen mit
der DB-Abfrage, nicht mit einem Screenshot.

- [ ] **Step 4: Verify the failure message is specific**

`REPRISE_YTDLP_BIN` auf ein veraltetes yt-dlp zeigen lassen und dieselbe
Episode abspielen. Erwartet: die Zeile nennt die Reparatur („update yt-dlp"),
nicht mehr „YouTube source could not be read with yt-dlp".

- [ ] **Step 5: Record the evidence**

Die vier Ergebnisse als Abschnitt „Abnahme" an diese Plandatei anhängen, mit
den tatsächlichen Zahlen und Abfragen. `phase:` im Frontmatter auf `verified`.

- [ ] **Step 6: Commit**

```bash
git add docs/plans/always-download-episodes.md
git commit -m "docs: record the acceptance run for always-download episodes"
```
