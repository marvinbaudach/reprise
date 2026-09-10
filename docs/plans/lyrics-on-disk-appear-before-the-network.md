---
slug: lyrics-on-disk-appear-before-the-network
worktree: /home/marvin/Projects/reprise-lyrics-on-disk-appear-before-the-network
branch: feature/lyrics-on-disk-appear-before-the-network
phase: refactored
codex_session:
created: 2026-09-10
---
# Lyrics von der Platte erscheinen vor dem Netz

## Warum

Gemessen am 10.09.2026 gegen die echte Bibliothek (1941 Tracks, Cache
`~/.cache/reprise/lyrics` mit 997 Einträgen, 2009 `.lrc`-Sidecars neben 1941
Audiodateien):

```
1165  sofort: synced sidecar
 403  NETZ: plain sidecar, synced-retry fällig
 304  NETZ: NotFound-Eintrag älter als 7 Tage (TTL abgelaufen)
  35  sofort: NotFound frisch
  31  sofort: cache-treffer (kein sidecar)
   3  NETZ: kein sidecar, kein gültiger cache

710 von 1941 Tracks (36 %) gehen bei JEDEM Abspielen ins Netz.
```

Davon sind **233 Tracks mit plain Sidecar seit dem 09.07.2026 gespielt worden
und haben bis heute keinen Cache-Eintrag** — der Pfad heilt sich also nicht
selbst, er wiederholt sich. Von den 2009 Sidecar-Dateien sind **526 plain**, also ohne Zeitmarken; davon
lassen sich **403 einem lebenden Track zuordnen** (der Rest gehört zu Dateien,
die nicht mehr in der Bibliothek stehen). Die 403 sind die Gruppe, um die es
geht.

Sichtbar wird das als leerer Spinner „Loading lyrics…" über Text, den die App
bereits von der Platte gelesen hat. Der Lookup läuft nur bei offenem
Lyrics-Tab (`start_request` steigt bei `!tab_open` aus), trifft dich also genau
dann, wenn du hinschaust.

## Was heute wirklich passiert

Der zweistufige Ablauf ist bereits gebaut und im Kern korrekt:

1. `PlayerLyrics::set_track` → `start_request(intent, allow_network = false)`
   (`crates/reprise-gnome/src/ui/lyrics/player_lyrics.rs:106-121`).
2. `load_or_fetch_with_cache_context_at_from` liest lokal und fällt durch bis
   `if !options.allow_network { return local_plain.ok_or(Temporary) }`
   (`crates/reprise-core/src/lyrics/mod.rs:214`) — der Plain-Text kommt also
   **sofort** zurück, ohne Netz.
3. `apply_response` rendert ihn via `apply_hit` und startet dann
   `request_online(force)` für die Synced-Aufwertung
   (`player_lyrics.rs:220-227`).

`LyricsState::request_upgrade` löscht `hit` bewusst **nicht** (anders als
`retry()`), und der Test
`online_upgrade_keeps_a_local_plain_hit_visible_while_generation_advances`
(`crates/reprise-view/src/lyrics.rs`) hält genau das fest. Der Zustand ist für
sichtbaren Text während der Aufwertung vorbereitet. Nur die View hält sich
nicht daran.

### Defekt A — die Aufwertung löscht, was schon dasteht

`start_request` ruft **bedingungslos** `view.show_loading(...)`
(`player_lyrics.rs:168-170`), und `show_loading` macht `clear_lines()` plus
Wechsel auf `LOADING_PAGE` (`lyrics_view.rs:220-226`). Die zweite Stufe räumt
den eben gerenderten Text also wieder weg — für 8 s Timeout pro Provider
(`lrclib.rs:18`), lrclib und NetEase nacheinander.

Dahinter liegt ein zweiter, feinerer Fall: fällt die Aufwertung auf denselben
lokalen Treffer zurück — der Normalfall für alle 403 Tracks —, läuft `apply_hit`
trotzdem, und `show_result` baut mit `clear_lines()` denselben Text neu auf.
Das ist sichtbares Flackern und kostet die Scroll-Position. Ein unterdrückter
Spinner allein reicht also nicht.

### Defekt B — eine ergebnislose Netzrunde hinterlässt keine Spur

Im `Ok(hit)`-Arm von `load_or_fetch_with_cache_context_at_from`
(`mod.rs:222-232`):

```rust
if is_local(hit.source) {
    if report.network_consensus_not_found {
        cache::write_not_found(cache_dir, now, query);
    }
}   // sonst: gar nichts geschrieben
```

`run_chain` liefert bei vorhandenem `local_plain` **immer** `Ok(local_plain)`
(`chain.rs:36-41`), und `network_consensus_not_found` ist nur `true`, wenn
*alle* Provider sauber `NotFound` gemeldet haben. Ein Timeout, ein HTTP-Fehler
oder ein offener Breaker genügt, damit nichts geschrieben wird: kein Record,
also kein `synced_retry_at`, also klassifiziert `cache::classify` beim nächsten
Abspielen wieder `RetryForSynced` — endlos. Das sind die 233 Tracks.

Der `Err(error)`-Arm direkt darunter (`mod.rs:236-241`) macht es bereits
richtig und schreibt `write_found(..., synced_retry_attempted = true)`. Der
`Ok`-Arm hat den Fall nur nicht.

### Defekt C — der Massenlauf überspringt genau die Tracks, um die es geht

`BatchServices::production` (`batch.rs:138`):

```rust
local: Arc::new(move |path| super::local_hit_with_source(source, path).is_some()),
```

und in `run_batch_with_services` (`batch.rs:197-200`):

```rust
let decision = (!(services.local)(&track.path)).then(|| (services.needs)(&track.query));
let needs = decision.as_ref().map_or(NeedsFetch::Skip, CacheDecision::classification);
```

**Jeder** lokale Treffer lässt den Massenlauf den Track überspringen — plain
genauso wie synced. Der Lauf über die ganze Bibliothek hat die plain
Sidecars deshalb nie angefasst und konnte sie nie aufwerten. Er sah sie, hielt
sie für erledigt und zählte weiter. Das ist der Grund, warum heute alles am
Einzel-Lookup beim Abspielen hängt — dem Pfad mit den Defekten A und B.

`item_outcome` kennt `NeedsFetch::RetryForSynced` bereits und dokumentiert es
(`batch.rs:232-234`), der Massenlauf ist für diesen Fall also gebaut; nur der
`local`-Wächter davor lässt ihn nie eintreten.

## Entscheidungen aus dem Grillen

- **Die Synced-Aufwertung bleibt bestehen**, wird aber unsichtbar und schweigt
  nach einer beantworteten Runde 7 Tage. lrclib bekommt laufend synced
  Fassungen nachgereicht; wer den Text hat, will die Zeitmarken trotzdem.
- **Nur eine echte Antwort verdient die 7 Tage Ruhe.** `SourceOutcome::Skipped`
  (Breaker offen, Metadaten fehlen) und `SourceOutcome::Failed` (Timeout,
  HTTP-Fehler) stempeln nicht — ein Timeout ist kein Beweis, dass es keine
  synced Fassung gibt. Bei kaputtem Netz kostet das nichts: der Breaker macht
  nach 3 Fehlern für 5 Minuten zu (`breaker.rs:4-5`) und liefert danach
  `Skipped`.
- **Der Sprung an den Anfang beim Einwechseln synced Zeilen wird hingenommen.**
  Er passiert pro Track genau einmal — danach ist der Sidecar synced
  überschrieben. `lyrics_view.rs` bleibt unangetastet.
- **Der Massenlauf kommt mit** (Task 3). Es ist derselbe Denkfehler an der
  dritten Stelle: „lokal vorhanden" mit „fertig" zu verwechseln.

## Nicht Teil dieser Aufgabe

- Die 304 Tracks mit abgelaufener `NotFound`-TTL. Da gibt es nichts lokal
  anzuzeigen, der Spinner ist dort richtig, und die 7-Tage-Wiedervorlage ist
  gewollt (`NEGATIVE_TTL_SECONDS`, `cache.rs:8`).
- `CACHE_VERSION`-Sprünge. Betrifft 27 lebende Tracks, ist gewolltes Verhalten
  und kein Teil des Symptoms.
- `lyrics_view.rs` und jede Änderung am Scroll-Verhalten.

## Aufgaben

### Task 1 — Der Spinner erscheint nur, wenn es nichts zu zeigen gibt

`crates/reprise-gnome/src/ui/lyrics/player_lyrics.rs`

**1a.** In `start_request` das `show_loading` daran binden, ob der Zustand
bereits einen Treffer hält:

```rust
fn start_request(self: &Rc<Self>, intent: RequestIntent, allow_network: bool) {
    if !self.tab_open.get() {
        return;
    }
    if self.state.borrow().hit().is_none() {
        if let Some(view) = self.view() {
            view.show_loading(&intent.track.query.title, &intent.track.query.artist);
        }
    }
    …unverändert…
}
```

Das trifft genau den Aufwertungsfall und nichts sonst:

- `set_track` → `LyricsState::set_track` setzt `hit = None` → Spinner wie bisher.
- `retry()` (Nutzer drückt „Nochmal") setzt `hit = None` → Spinner wie bisher.
- `request_missing()` läuft nur, wenn `hit.is_none()` → Spinner wie bisher.
- `request_upgrade()` lässt `hit` stehen → **kein** Spinner.

Das `borrow()` endet vor `runtime.request(...)`; `apply_response` läuft ohnehin
erst im späteren `spawn_future_local`, also kein `RefCell`-Konflikt.

**1b.** In `apply_hit` einen Kurzschluss für den identischen Treffer, sonst
flackert die Aufwertung den Text neu auf. `LyricsHit` ist `PartialEq` —
`LyricsState` vergleicht ihn bereits:

```rust
fn apply_hit(self: &Rc<Self>, hit: &LyricsHit) {
    if self.state.borrow().hit() == Some(hit) {
        self.schedule_next_line();
        return;
    }
    …unverändert…
}
```

Der `schedule_next_line()`-Aufruf bleibt erhalten, damit der Zeilentakt nicht
davon abhängt, ob die Aufwertung etwas Neues brachte.

**Tests** in `player_lyrics_tests.rs`:

- Lookup-Double, das offline `LyricsBody::Plain` liefert und online blockiert:
  nach der Offline-Antwort steht die View auf der Content-Seite mit dem
  Plain-Text und wechselt **nicht** auf `LOADING_PAGE`, solange die
  Online-Anfrage läuft.
- Gegenprobe: ohne lokalen Treffer erscheint der Spinner unverändert.
- Fällt die Online-Antwort auf denselben Treffer zurück, wird `show_result`
  nicht erneut aufgerufen (Zähler im View-Double oder Vergleich der
  Zeilen-Identität). **Wichtig:** dieser Test muss den echten
  `apply_response`-Pfad fahren und das Lookup-Double genau das zurückgeben
  lassen, was `run_chain` in diesem Fall liefert — `Ok(local_plain)` mit
  `source: LyricsSource::Sidecar`, also denselben Wert wie die Offline-Stufe.
  Ein handgebauter `LyricsHit` als Fixture würde eine Gleichheit prüfen, die im
  echten Ablauf nie auftritt. `LyricsHit` vergleicht `body` **und** `source`;
  liefert irgendein Pfad denselben Text mit anderer Quelle, greift der
  Kurzschluss bewusst nicht.

### Task 2 — Eine beantwortete Netzrunde hinterlässt einen Stempel

`crates/reprise-core/src/lyrics/chain.rs`, `crates/reprise-core/src/lyrics/mod.rs`

`ChainReport` bekommt ein zweites Feld:

```rust
pub(super) struct ChainReport {
    pub(super) result: Result<LyricsHit, LyricsError>,
    pub(super) network_consensus_not_found: bool,
    pub(super) network_answered: bool,
}
```

`network_answered` ist `true`, sobald **mindestens ein** Netz-Provider
`SourceOutcome::NotFound` oder `SourceOutcome::Hit(_)` geliefert hat — also
tatsächlich geantwortet hat. `Skipped` und `Failed` zählen nicht. Alle drei
`ChainReport`-Rückgabestellen in `run_chain` setzen das Feld.

Im `Ok(hit)`-Arm von `load_or_fetch_with_cache_context_at_from`:

```rust
if is_local(hit.source) {
    if report.network_consensus_not_found {
        cache::write_not_found(cache_dir, now, query);
    } else if report.network_answered {
        cache::write_found(cache_dir, now, query, &hit, true);
    }
}
```

Damit trägt der Record `synced_retry_at = now`, `plain_retry_is_fresh` greift,
`classify` liefert `Skip`, und `skipped_result` gibt über `prefer_local_plain`
weiterhin den lokalen Text zurück — der Cache-Eintrag verdrängt den Sidecar
also nicht, er unterdrückt nur die Netzrunde für 7 Tage.

**Tests** in `chain_tests.rs`: `network_answered` ist `false`, wenn alle
Provider `Skipped` oder `Failed` liefern, und `true`, sobald einer `NotFound`
oder `Hit` liefert. In `mod_tests.rs`: bei vorhandenem lokalem Plain-Treffer
und einem Provider, der `NotFound` liefert, liegt nach dem Aufruf ein
`Found`-Record mit gesetztem `synced_retry_at` im Cache-Verzeichnis, und ein
zweiter Aufruf mit demselben `now` klassifiziert `Skip` und ruft keinen
Provider mehr auf (Aufrufzähler im Double). Gegenprobe: derselbe Ablauf mit
einem Provider, der `Failed` liefert, schreibt **keinen** Record und fragt beim
zweiten Aufruf erneut.

### Task 3 — Der Massenlauf überspringt nur fertige Tracks

`crates/reprise-core/src/lyrics/batch.rs`

Der `local`-Wächter bekommt dieselbe Regel wie `best_local` (`mod.rs:271-277`):
nur ein **synced** oder **instrumental** lokaler Treffer ist ein Grund zu
überspringen. Ein plain Sidecar ist es nicht.

```rust
local: Arc::new(move |path| {
    super::local_hit_with_source(source, path).is_some_and(|hit| {
        matches!(hit.body, LyricsBody::Synced(_) | LyricsBody::Instrumental)
    })
}),
```

Danach durchläuft ein plain-Sidecar-Track die normale Cache-Entscheidung,
landet auf `NeedsFetch::RetryForSynced` und wird von `item_outcome` bereits
korrekt gezählt („Plain text re-confirms the cache and is not a newly cached
track", `batch.rs:232-234`). Findet lrclib eine synced Fassung, überschreibt
`sidecar_write` die `.lrc` — der Massenlauf wertet also endlich auf. Der
Stempel aus Task 2 sorgt dafür, dass ein zweiter Lauf am selben Tag dieselben
Tracks nicht erneut abklappert.

**Folge für die Laufzeit, keine Regression.** Der Massenlauf meldet die
plain-lokalen Tracks bisher als `Skipped` und fasst das Netz für sie nie an.
Danach sind es echte Online-Lookups: bis zu 403 Tracks mal bis zu vier
HTTP-Anfragen, wo vorher null waren. Damit wird auch `progress.fail()` bei
`all_breakers_open()` nach einem `Failed`-Item auf einem Lauf erreichbar, der
vorher gar nicht ins Netz ging. Ein erster Lauf nach dem Fix dauert also
spürbar länger und kann mit `BatchRunStatus::Finished` nach einem
Breaker-Abbruch enden — beides ist gewollt und darf nicht als Rückschritt
gelesen werden.

**Tests** in `batch_tests.rs`:

- ein Track mit lokalem **plain** Treffer wird nicht mehr übersprungen, sondern
  erreicht den Online-Lookup;
- ein Track mit lokalem **synced** Treffer wird weiterhin übersprungen,
  Instrumental ebenso;
- ein **zweiter Lauf innerhalb der TTL überspringt einen gestempelten
  plain-Track**. Das ist die sichtbare Folge von Task 2: wer den Massenlauf
  startet, nichts findet und ihn am selben Tag erneut startet, bekommt für alle
  403 Tracks `Skipped`. Das ist korrekt und gehört als Test festgenagelt, damit
  es Spezifikation ist und keine Überraschung.

## Verifikation

Alles im eigenen Worktree, ohne Netz und ohne die echte Bibliothek:

```
cargo test -p reprise-core lyrics
cargo test -p reprise-view lyrics
cargo test -p reprise-gnome ui::lyrics
scripts/check-display-tests.sh --rule-named
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

After the review, an incomplete provider round no longer earns the full
seven-day suppression from Task 2. Its first partial answer is deferred only
for the five-minute breaker window; a second incomplete attempt earns the
seven-day suppression so that a permanently unavailable provider cannot make
every playback repeat the lookup forever.

Kein Test darf `~/.cache/reprise/lyrics` oder `~/Music` anfassen — die
vorhandenen Tests arbeiten bereits mit `cache_dir`-Parameter und `tempfile`,
das bleibt so.

Die Beweise pro Defekt:

- **A** — der neue Test in `player_lyrics_tests.rs`: die Content-Seite bleibt
  während der Aufwertung stehen, und ein identischer Treffer rendert nicht neu.
- **B** — die neuen Tests in `mod_tests.rs`: eine unvollständige Runde wartet
  zunächst nur das Breaker-Fenster ab; bleibt ein Provider beim zweiten Versuch
  unerreichbar, wird der wiederholte Lauf sieben Tage gedrosselt. Eine reine
  `Failed`-Runde wird weiterhin erneut versucht.
- **C** — der neue Test in `batch_tests.rs`: plain-lokal erreicht den
  Online-Lookup, synced-lokal nicht.

**Kontrollarm nach dem Landen** (nicht Teil der Abnahme im Worktree): das
Messskript aus der Diagnose liest DB, Sidecars und Cache und gibt die Tabelle
von oben aus. Nach einem normalen Build und ein paar gespielten Tracks muss die
Gruppe „NETZ: plain sidecar, synced-retry fällig" von 403 aus schrumpfen, und
nach einem Massenlauf muss die Zahl der plain Sidecars unter 403 fallen. Das
Skript bleibt im Scratchpad, nur die Zahlen kommen ins Repo.

The rule-named display suite executes the three ignored Task 1 tests under the
repository's isolated Xvfb convention.

## Parallelität

**Kein Schnitt.** Die drei Tasks umfassen zusammen rund 40 Zeilen
Produktivcode. Die Dateigruppen wären zwar disjunkt
(`crates/reprise-gnome/src/ui/lyrics/**` gegen `crates/reprise-core/src/lyrics/**`),
aber zwei parallele `cargo`-Builds für eine Handvoll Zeilen kosten mehr
Wall-Clock als sie sparen, und Task 3 setzt Task 2 inhaltlich voraus: ohne den
Stempel würde ein Massenlauf, der plain Sidecars nicht mehr überspringt, bei
jedem Start dieselben 403 Tracks erneut abklappern. Ein Strang, ein Worktree.

### Dateibesitz (Startpunkt, kein Zaun)

```
crates/reprise-gnome/src/ui/lyrics/player_lyrics.rs
crates/reprise-gnome/src/ui/lyrics/player_lyrics_tests.rs
crates/reprise-core/src/lyrics/chain.rs
crates/reprise-core/src/lyrics/chain_tests.rs
crates/reprise-core/src/lyrics/mod.rs
crates/reprise-core/src/lyrics/mod_tests.rs
crates/reprise-core/src/lyrics/batch.rs
crates/reprise-core/src/lyrics/batch_tests.rs
```

`ChainReport` ist `pub(super)`, lebt in `chain.rs`, wird nur dort konstruiert
und nur in `mod.rs` und `chain_tests.rs` gelesen. `BatchServices` lebt in
`batch.rs`. Die Liste hält damit den ganzen Vertrag. Sollte sie sich als zu eng
erweisen, ist das ein Grund weiterzugehen, kein Grund anzuhalten; anhalten nur,
wenn der Vertrag selbst falsch ist.
