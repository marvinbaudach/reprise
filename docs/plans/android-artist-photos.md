---
slug: android-artist-photos
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-14
strands: core,ui
merge_order: core,ui
---
# Android: Künstlerfotos in der Interpretenansicht — Mutterplan

Bindende Quelle ist
`docs/superpowers/specs/2026-08-14-android-artist-photos-design.md`. Dieser Plan
wiederholt sie nicht, er ergänzt sie um das *Wie*, die Reihenfolge und die
Abnahme. Wo Spec und Plan sich widersprechen, gilt die Spec.

Gelesen gegen `origin/dev` @ `5721ade95e`. Jede Zeilenangabe stammt aus diesem
Stand; wer sie nicht wiederfindet, hat eine andere Basis.

Diese Datei trägt nur den gemeinsamen Kontext. Die Aufgaben stehen in zwei
Strangdateien, und sie werden **nacheinander** gefahren:

| Strang | Datei | Inhalt | Aufgaben |
| --- | --- | --- | --- |
| `core` | `docs/plans/android-artist-photos-core.md` | PR 1, reines Rust — Kern und Brücke | 7 |
| `ui` | `docs/plans/android-artist-photos-ui.md` | PR 2, Kotlin (plus ein Rust-Enum) — Oberfläche | 10 |

## Regeln für den Umsetzer — zuerst lesen

**Pro Aufgabe zuerst der Test, dann der Code.** Jede Aufgabe nennt die
Testnamen. Ein Test, der beim ersten Lauf grün ist, hat nichts gemessen — er
muss rot sein, bevor der Code entsteht, und das Rot gehört ins Protokoll.

**Kein Gerät, kein `adb`, kein Emulator** — bis auf die letzte Aufgabe des
`ui`-Strangs. Alles andere läuft auf dem Host.

**`core` und `ui` laufen nie gleichzeitig.** Der Grund steht unter
[Parallelität](#parallelität). Wer zwei Worktrees nebeneinander aufmacht, hat
den Plan nicht gelesen.

**Harte Umgebungsfakten** (gemessen, nicht verhandelbar):

- Die Android-Suite braucht **JDK 21**. Der Systemstandard ist hier JDK 26, und
  JDK 26 killt Robolectric. Also vor *jedem* Gradle-Aufruf
  `JAVA_HOME=/usr/lib/jvm/java-21-openjdk` setzen.
- Die FFI-Tests hängen an der `readdir`-Reihenfolge. Die Suite läuft mit
  `TMPDIR=/tmp` grün.
- `BUILD SUCCESSFUL` ist kein Beweis: Gradle meldet `:app:testDebugUnitTest` als
  up-to-date, endet mit 0 und führt nichts aus. Das Urteil steht in
  `android/app/build/test-results/testDebugUnitTest/*.xml`, und
  `scripts/check-android-suite.sh:33-77` prüft die Frische dieser Dateien mit.
- `cargo test --exact` läuft leicht ins Leere. Ausgewertet wird über
  `grep -c '^test result: FAILED'` auf dem Logfile, nicht über den Blick auf die
  letzte Zeile.
- In `reprise-gnome` gibt es kein `--lib`; die Desktop-Tests laufen als
  `--bin reprise`.
- Lange Läufe in eine Datei umleiten, nicht auf die Konsole. Unten steht
  `$LOG` für ein Arbeitsverzeichnis außerhalb des Repos, etwa
  `/tmp/reprise-artist-photos`.

**Die Kotlin-Seite kompiliert nicht, bevor die Rust-Seite steht.** Die
UniFFI-Bindings unter `android/app/src/main/java/uniffi/` sind generiert und
gitignoriert; `scripts/check-android-suite.sh:146-151` löscht und erzeugt sie
neu aus `libreprise_android_ffi.so`. Nach jeder Änderung an
`crates/reprise-android-ffi/**` müssen sie neu erzeugt werden, sonst kennt
Kotlin die neuen Methoden nicht. Daraus folgt die Reihenfolge der beiden PRs,
und daraus folgt, dass `ui` mit einer Binding-Erzeugung beginnt.

## Entscheidungen, die die Spec offen lässt

Diese fünf Punkte sind vom Plan gefällt, nicht von der Spec. Wer sie anders
haben will, sagt es vor Aufgabe 3 des `core`-Strangs.

1. **Rückgabetypen.** `artist_portrait_cached` gibt `Option<String>` zurück,
   genau wie die Spec schreibt: sie fasst die Datenbank nicht an, also kann sie
   nur „kein Porträt" antworten. `artist_portrait_fetch` gibt dagegen
   `Result<Option<String>, LibraryError>` zurück, weil sie das Gate lesen muss
   und ein vergifteter Handle nicht als „kein Porträt" durchgehen darf — genau
   die Trennung, die `lib.rs:216-236` für `track_artwork` ausführlich begründet.
2. **Neue Dateien statt Wachstum in `lib.rs`.** Die Porträtmethoden kommen nach
   `crates/reprise-android-ffi/src/artist_portrait.rs`, der Schalter nach
   `crates/reprise-android-ffi/src/online_sources.rs`, beide mit eigenem
   `#[uniffi::export] impl MusicLibrary`-Block nach dem Muster von
   `appearance.rs:156-228`. `lib.rs` bekommt nur zwei `mod`-Zeilen. Das hält den
   Rebase klein (siehe unten) und `lib.rs` unter seiner Größe.
3. **Das Netz-Gate lebt genau einmal, in Rust.** Kotlin fragt nie selbst, ob es
   fragen darf. Dieselbe Entscheidung an zwei Stellen ist der Fehler, den dieses
   Projekt schon einmal bezahlt hat. Der Kotlin-Test „bei ausgeschaltetem
   Schalter fragt die Detailseite nicht" wird deshalb über einen
   `LibrarySessionPort`-Doppelgänger geführt, der den Vertrag der Brücke
   nachbildet; dass der Vertrag stimmt, beweist Aufgabe 5 des `core`-Strangs in
   Rust.
4. **Keine Merkliste für Porträtpfade in `LibrarySession`.** `artworkFor`
   (`LibrarySession.kt:196-214`) merkt sich aufgelöste Coverpfade, weil ein
   Tag-Lesen über SAF teuer ist. Ein Porträt kostet ein paar `stat`-Aufrufe im
   App-Cache; eine gemerkte `null` würde dagegen verhindern, dass ein gerade
   heruntergeladenes Porträt beim nächsten Blick auf die Liste auftaucht. Also
   durchreichen, nicht merken.
5. **Der Zwischenspeicher-Schlüssel bekommt eine Art, keine zweite Größenskala.**
   `AndroidArtworkSize` wird um `ArtistDetail` erweitert statt ein zweites
   Größen-Enum einzuführen — die Spec will ausdrücklich, dass `ArtworkCache` und
   `ArtworkRequestGate` mitbenutzt werden, und die tragen `AndroidArtworkSize`
   durch. Getrennt werden Track und Interpret über ein neues Feld `kind` im
   Schlüssel, nicht über die Größe. **Der neue Enum-Zweig landet in PR 2, nicht
   in PR 1** — die Begründung steht unter [Parallelität](#parallelität).

## Rebase gegen `feature/android-list-scroll-performance`

**Das ist das Kopfrisiko dieses Plans.** Basis bleibt `origin/dev`. Der offene
Branch steht mit 14 Commits darüber und fasst **mehr** an, als die Spec nennt —
gemessen an seiner Merge-Basis:

| Datei | dort | Kollisionsrisiko hier |
| --- | --- | --- |
| `crates/reprise-core/src/cover.rs` | +125, Hunks ab Zeile 347 (`resolution_index_path_in`, `thumbnail_for_track_with_source`) | **gering** — unsere Änderung sitzt bei 142-168 |
| `crates/reprise-android-ffi/src/lib.rs` | +21, ein Hunk in `track_artwork` (ab 239) | **gering**, solange die neuen Methoden in eigenen Dateien liegen (Entscheidung 2) |
| `crates/reprise-android-ffi/src/artwork_tests.rs` | +24 | gering |
| `android/.../TrackCover.kt` | +162 | **hoch** — unsere fünf Stellen sind klein, aber liegen mittendrin |
| `android/.../ArtworkCache.kt` | +121 | **hoch** — beide ändern die Schlüsseltypen |
| `android/.../BrowseTabs.kt` | +168 | **hoch** — beide ändern `ArtistRow`/`ArtistDetailSections` |
| `android/.../MainActivity.kt`, `MainActivitySurface.kt`, `BrowseScreen.kt` | +23/+1/+179 | mittel |
| Tests: `ArtworkCacheTest.kt`, `ArtworkCompositionTest.kt`, `TrackArtworkTest.kt`, `BrowseSurfaceTest.kt`, `MainActivityConfigurationTest.kt`, `ArtistSearchActivityTest.kt` | je +10…+106 | mittel |
| `ArtworkRequestGate.kt` | **nicht angefasst** | keins |

**Die Zwei-PR-Aufteilung entschärft dieses Risiko zur Hälfte: PR 1 fasst keine
der drei hoch-riskanten Dateien an** — er fasst `android/` überhaupt nicht an.
Das gesamte Kollisionsrisiko sitzt in PR 2, und PR 2 kann, wenn der andere
Branch zuerst landet, gegen den bereits gelandeten Stand geschrieben statt
nachträglich rebast werden.

Daraus folgt die Regel für PR 2: **`TrackCover.kt` bekommt einen zweiten
Auflösungsweg neben dem bestehenden, keinen Umbau.** Konkret bleiben es dort
fünf Eingriffe (zwei Konstruktorparameter mit Vorgabe, eine Verzweigung in
`resolveVisual`, drei `when`-Zweige). Alles Neue — `rememberArtistArtworkVisual`,
`ArtistAvatar`, `ArtistPortraitHeader` — steht in `ArtistCover.kt`, und
`ArtworkKind`/`ArtworkRequest` wachsen in `ArtworkRequestGate.kt`, das der andere
Branch nicht anfasst.

Wer zweiter landet, rebast. Erwartete Konfliktstellen dann: die drei
`when`-Blöcke in `TrackCover.kt`, die zwei Schlüsselklassen in
`ArtworkCache.kt`, `ArtistRow` und der `itemCount`-Ausdruck in `BrowseTabs.kt`,
und die Parameterlisten von `MainActivitySurfaceDependencies`. Alle vier sind
Textkonflikte an bekannten Stellen, keine Semantikkonflikte.

**Herabgestuft: das Perf-Risiko.** `thumbnail_with_source` liest die Quelldatei
ganz ein und hasht sie, bevor es den Cache-Treffer feststellt
(`cover.rs:222-228`, nachgelesen und bestätigt). Das läuft aber auf den eigenen
Einzelthread-Executoren `reprise-artwork-list` / `reprise-artwork-full`
(`TrackCover.kt:44-45`), nie auf dem Hauptthread. Es kann **keinen Frame
reißen**, nur seine eigene Lane verstopfen und das Bild verspätet liefern. Das
bleibt ein Messpunkt der Sichtprüfung, ist aber nicht das Kopfrisiko.

## Testkommandos

```sh
# Kern
cargo test --locked -p reprise-core > $LOG/core.log 2>&1
grep -c '^test result: FAILED' $LOG/core.log        # muss 0 sein

# Brücke
TMPDIR=/tmp cargo test --locked -p reprise-android-ffi > $LOG/ffi.log 2>&1
grep -c '^test result: FAILED' $LOG/ffi.log         # muss 0 sein

# Desktop-Regression (kein --lib in reprise-gnome)
cargo test --locked -p reprise-gnome --bin reprise > $LOG/gnome.log 2>&1
grep -c '^test result: FAILED' $LOG/gnome.log       # muss 0 sein

# Android, volles Gate (erzeugt die Bindings neu)
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/check-android-suite.sh > $LOG/android.log 2>&1
grep -E '^suites=' $LOG/android.log                  # failures=0 errors=0 verdict=fresh

# Android, enger Lauf während der Arbeit
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  android/gradlew --project-dir android :app:testDebugUnitTest \
  --tests 'de.reprise.spike.ArtistPortraitSurfaceTest' > $LOG/android-narrow.log 2>&1
ls -l android/app/build/test-results/testDebugUnitTest/    # frisch? sonst lief nichts
```

Die ersten drei Läufe sind die Abnahme von `core`, die letzten beiden gehören
`ui`. Formalitäten am Ende jedes Strangs: `cargo fmt --all`, `cargo clippy
--all-targets --all-features -- -D warnings`.

## Parallelität

Zwei Stränge, aber **kein paralleler Lauf**: `merge_order: core,ui`, und `ui`
darf erst begonnen werden, wenn `core` gelandet ist.

### Warum hier ausnahmsweise nicht parallel gefahren wird

Der Schnitt wurde ernsthaft versucht und trägt als *Reihenfolge*, nicht als
*Gleichzeitigkeit*. Die Kotlin-Seite kann nicht ohne die Rust-Seite übersetzen:
die UniFFI-Bindings unter `android/app/src/main/java/uniffi/` sind generiert und
gitignoriert (`scripts/check-android-suite.sh:146-151` löscht sie und erzeugt
sie neu). Alles Kotlin, das `artistPortraitCached`, `artistPortraitFetch`,
`onlineSourcesEnabled` oder `AndroidArtworkSize.ArtistDetail` nennt, ist ohne
den gelandeten Rust-Teil schlicht nicht übersetzbar — und ein zweiter Worktree
könnte es nicht einmal bemerken, weil er dieselben Bindings aus einem anderen
Rust-Stand erzeugt.

**Konkret: keine zwei Worktrees gleichzeitig aufmachen.** `ui` beginnt mit
`git fetch origin dev` auf dem Stand, der `core` bereits enthält, und mit dem
Neuerzeugen der Bindings. Vorher gibt es dort nichts zu tun.

### Warum PR 1 `android/` nicht anfasst

`AndroidArtworkSize::ArtistDetail` (Entwurfsaufgabe 3) ist in den `ui`-Strang
gewandert und ist dort die erste Aufgabe. Der Grund ist `dev`: ein neuer
Enum-Zweig macht die drei `when`-Blöcke in `TrackCover.kt` (`:72-75`, `:111-114`,
`:285-288`) unvollständig und damit den ganzen Android-Modul-Build rot. Seit
#471 sind die Android-Gates in CI scharf — `dev` wäre zwischen den beiden
Landungen rot, und zwar für jeden anderen Strang mit.

Deshalb gilt: **PR 1 fasst `android/` mit keiner einzigen Zeile an.** Was PR 1
an `reprise-android-ffi` tut, ist rein additiv (drei neue Methoden), und
additive UniFFI-Methoden lassen bestehendes Kotlin unverändert übersetzen. PR 2
trägt Enum-Zweig und `when`-Zweige zusammen in einem Commit-Bereich und ist
damit für sich genommen wieder grün.

Eine Folge dieser Verschiebung: der Test
`a_portrait_is_never_requested_at_the_now_playing_rung` (Entwurfsaufgabe 6)
benutzt `ArtistDetail` und kann deshalb in PR 1 nicht existieren. Er ist in
`ui`-Aufgabe 1 aufgeführt. Das ist kein vergessener Test, sondern eine
Verschiebung; beide Strangdateien sagen das an ihrer Stelle.

### Merge-Reihenfolge

1. **`core`** — sieben Aufgaben, reines Rust. Landet zuerst.
2. **Sperrpunkt:** die Android-Gates auf `dev` müssen nach dem Merge von `core`
   grün sein. Das ist der Beweis, dass die additive FFI-Erweiterung die
   Kotlin-Übersetzung nicht angefasst hat. Erst danach beginnt `ui`.
3. **`ui`** — zehn Aufgaben, Kotlin plus der eine Enum-Zweig. Landet zuletzt.

Ein dritter Strang wurde erwogen und verworfen: Aufgaben 4 und 5 von `ui` ändern
beide `BrowseTabs.kt`, Aufgaben 3 und 7 beide `MainActivity.kt` und
`MainActivitySurface.kt` — keine disjunkte Dateigruppe, und beide bräuchten
ohnehin erst `core`.

### Post-Merge-Cross-Checks

Die Regel bleibt: eine Prüfung, die eine Datei liest, die der Strang nicht
besitzt, gehört in diese Liste und **nicht** in die Abnahme des Strangs. Weil
die beiden Stränge hier sequenziell laufen und jeder den Baum für sich allein
hat, fällt die Liste kürzer aus als im Entwurf — die fünf Punkte von dort sind
alle noch da, aber vier sind in eine Strangabnahme gewandert:

1. **Die volle Android-Suite** (`scripts/check-android-suite.sh`) →
   **Abnahme von `ui`** (Aufgabe 9). Sie übersetzt und fährt alles, auch
   `BrowseTabs.kt`, `MainActivity*` und die Navigationstests — und `ui` besitzt
   diese Dateien alle. `core` fährt sie nicht und darf sie nicht fahren.
2. **`MainActivitySettingsNavigationTest.kt:52`** (`assertCountEquals(4)` wird
   durch die neue Einstellungsseite falsch) → **innerhalb von `ui`**, Aufgaben 6
   und 7 liegen jetzt im selben PR. Kein Cross-Check mehr.
3. **`ArtistDetailSurfaceTest.kt`, `BrowseSurfaceTest.kt`,
   `LibraryScreenStateTest.kt`, `MainActivityConfigurationTest.kt`,
   `ArtistSearchActivityTest.kt`** → **Abnahme von `ui`**, abgedeckt durch
   Punkt 1. Alles Android-Testdateien, alle im Besitz von `ui`.
4. **`cargo test --locked -p reprise-gnome --bin reprise`** → **Abnahme von
   `core`** (Aufgabe 7). `reprise-gnome` gehört keinem Strang, aber `core` läuft
   sequenziell allein im Baum und kann den Lauf deshalb fahren, ohne dass ihn
   ein gleichzeitiger zweiter Strang unter der Hand rot färbt — und er *muss*
   ihn fahren, weil der Desktop-Porträt-Worker
   (`crates/reprise-gnome/src/ui/now_playing/artist_portrait_worker.rs`)
   `load_or_fetch` ruft, das `core`-Aufgabe 2 umbaut. Ein Regressionsrisiko, das
   `core` erzeugt, darf nicht erst nach dem Merge auffallen.
5. **`cargo test --locked -p reprise-core`** als Ganzes → **Abnahme von `core`**
   (Aufgabe 7), aus demselben Grund.

Echt post-merge bleibt danach nur zweierlei:

- **Nach `core`:** der Sperrpunkt oben — Android-Gate auf `dev` grün, bevor `ui`
  beginnt.
- **Zwischen den beiden Landungen:** landet `feature/android-list-scroll-performance`
  in diesem Fenster, wird `ui` gegen den neuen Stand geschrieben und die volle
  Android-Suite danach noch einmal gefahren. Die Konfliktstellen stehen oben.
