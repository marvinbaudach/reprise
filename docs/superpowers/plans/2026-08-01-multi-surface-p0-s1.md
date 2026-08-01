# P0 (Vorlauf) und S1 (Android-Spike) — Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Voraussetzungen für die Android-App schaffen — eine leere,
mechanisch abgesicherte `reprise-view`-Crate, geklärter Dateibesitz, und ein
schriftlicher Befund, ob der Android-Weg (Rust-NDK, F-Droid, UniFFI, Media3,
SAF) trägt.

**Architecture:** P0 ist Repo-Hygiene und liefert echten, getesteten Code.
S1 ist ein **Spike**: ein Wegwerf-Prototyp, dessen Deliverable ein
schriftlicher Befund ist, kein übernommener Code. Die S1-Tasks folgen
deshalb bewusst **nicht** dem TDD-Zyklus — sie haben Abbruchkriterien statt
Testerwartungen. Wo unten keine exakte Befehlszeile steht, ist genau das der
Untersuchungsgegenstand der Aufgabe.

**Tech Stack:** Rust 1.92 (workspace `rust-version`), Cargo-Workspace,
`cargo-ndk`, Android NDK, UniFFI, Jetpack Compose, Media3/ExoPlayer,
`fdroidserver`.

**Spec:** `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`

**Worktree:** `.worktrees/multi-surface`, Branch `feature/multi-surface-frontends`

## Global Constraints

- **Gates vor jedem Commit** (aus `AGENTS.md`, Repo-Wurzel):
  `cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436` (`paste`, über
  `lofty`). Ein NEUES Advisory heißt STOP.
- **`cargo test` ohne `--workspace` läuft nur das gnome-default-member** —
  immer `--workspace` verwenden.
- **Dateigrößenregel:** jede erstellte oder wesentlich bearbeitete Codedatei
  endet **< 800 Zeilen**. Markdown ist ausgenommen.
- **Test-Baseline:** wird in Task 1 gemessen und in jedem folgenden Task als
  Referenz genannt. Nicht aus `AGENTS.md` übernehmen — die dortige Zahl
  gehört zu einem anderen Plan.
- **`reprise-view` ist MIT** (Spec B4). Der Workspace-Default ist bereits
  `license.workspace = true` → MIT; nicht überschreiben.
- **`reprise-view` darf niemals** `gtk4`, `libadwaita`, `glib`, `gstreamer`
  oder `zbus` in den Abhängigkeitsbaum ziehen (Spec §5).
- **Commit-Format:** `<type>: <description>`, englisch (Typen: feat, fix,
  refactor, docs, test, chore, perf, ci).

---

# Teil A — P0 (Vorlauf)

### Task 1: `reprise-view` als leere Crate anlegen

**Files:**
- Create: `crates/reprise-view/Cargo.toml`
- Create: `crates/reprise-view/src/lib.rs`
- Modify: `Cargo.toml` (Workspace-`members`)

**Interfaces:**
- Consumes: nichts.
- Produces: die Crate `reprise-view` als Workspace-Mitglied. Task 2 prüft
  ihren Abhängigkeitsbaum; P1a füllt sie.

- [ ] **Step 1: Test-Baseline messen und notieren**

Run vom Repo-Wurzelverzeichnis des Worktrees:

```bash
cargo test --workspace 2>&1 | tail -20
```

Notiere die Gesamtzahl („N passed; M ignored") hier im Plan unter dieser
Zeile. Diese Zahl ist ab jetzt die Referenz für jeden Task dieses Plans.

Baseline: `________ passed; ________ ignored`

- [ ] **Step 2: Die Crate anlegen**

`crates/reprise-view/Cargo.toml`:

```toml
[package]
name = "reprise-view"
description = "Toolkit-freie Präsentationsschicht: ViewModels, Formatierung, Filterung, Sortierung und Zustandsmaschinen, geteilt von allen Reprise-Oberflächen"
version.workspace = true
authors.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
reprise-core = { path = "../reprise-core" }

[lints]
workspace = true
```

`crates/reprise-view/src/lib.rs`:

```rust
//! Die toolkit-freie Präsentationsschicht von Reprise.
//!
//! Diese Crate hält alles, was zwischen Kern und Oberfläche steht und kein
//! Toolkit braucht: ViewModels, Formatierung, Filterung, Sortierung,
//! Zustandsmaschinen, Navigationshistorie und übersetzbare Texte. Die
//! GTK-, Compose- und Web-Oberflächen konsumieren dieselben Werte, damit
//! diese Logik genau einmal existiert.
//!
//! Verbindliche Grenze: hier darf niemals `gtk4`, `libadwaita`, `glib`,
//! `gstreamer` oder `zbus` hineinlinken. `scripts/check-architecture.sh`
//! erzwingt das mechanisch.
//!
//! Die Crate ist beim Anlegen leer. `docs/superpowers/specs/
//! 2026-08-01-multi-surface-frontends-design.md` §4 (P1a) beschreibt, was
//! zuerst hier einzieht.
```

- [ ] **Step 3: Workspace-Mitgliedschaft eintragen**

In `Cargo.toml` (Repo-Wurzel) die Zeile `"crates/reprise-view",` in
`members` einfügen — alphabetisch nach `"crates/reprise-stems",` und vor
dem schließenden `]`. Die `default-members`-Zeile bleibt unverändert.

- [ ] **Step 4: Bauen und prüfen, dass die Crate leer, aber gültig ist**

```bash
cargo build -p reprise-view
```

Erwartet: `Finished`. `reprise-core` ist bewusst schon eingetragen, obwohl
die Crate noch leer ist, weil P1a sofort darauf zugreift — das löst keine
Warnung aus (`unused_crate_dependencies` ist allow-by-default) und bricht
daher auch das `-D warnings`-Gate in Step 5 nicht.

- [ ] **Step 5: Volle Gates fahren**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
```

Erwartet: alle grün, Testzahl **unverändert** gegenüber der Baseline aus
Step 1 (eine leere Crate bringt keine Tests mit).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/reprise-view/
git commit -m "feat: add empty reprise-view crate for the shared presentation layer"
```

---

### Task 2: Das `cargo tree`-Gate für `reprise-view`

**Files:**
- Modify: `scripts/check-architecture.sh`

**Interfaces:**
- Consumes: die Crate aus Task 1; die dort bereits definierte
  Shell-Funktion `run_dependency_probe` und die Variable
  `banned_dependency_families`.
- Produces: ein Tor, das jeden künftigen Verstoß in P1a/P1b sofort rot
  macht.

- [ ] **Step 1: Den fehlschlagenden Zustand herstellen**

Trage in `crates/reprise-view/Cargo.toml` unter `[dependencies]`
vorübergehend ein:

```toml
zbus = "5"
```

Das ist der Verstoß, den das Gate fangen muss. Wird in Step 5 wieder
entfernt.

- [ ] **Step 2: Das Gate schreiben**

In `scripts/check-architecture.sh`, direkt **nach** dem
`stray_runtime_edge`-Block (der mit
`echo "reprise-runtime may depend on reprise-core and reprise-runtime-protocol only; found:" >&2`
beginnt und mit dessen `fi` endet), einfügen:

```bash
# The shared presentation layer is consumed by GTK, by a Compose app on
# Android and by a Tauri app on the desktop (multi-surface spec §1). A
# toolkit or bus edge here would silently re-couple every surface to the
# GNOME process — the exact failure this crate exists to prevent. It may
# depend on the engine and on nothing else in the workspace.
view_tree=$(run_dependency_probe "reprise-view all features" \
  -p reprise-view --all-features -e normal --prefix none --target all) || exit 1
if printf '%s\n' "$view_tree" | rg --quiet "$banned_dependency_families"; then
  echo "reprise-view must not depend on GTK, libadwaita, GLib, GStreamer, or zbus" >&2
  printf '%s\n' "$view_tree" | rg "$banned_dependency_families" >&2
  exit 1
fi
stray_view_edge=$(printf '%s\n' "$view_tree" \
  | rg '^reprise-[a-z-]+ ' \
  | rg -v '^(reprise-core|reprise-view) ' \
  | sort -u || true)
if [[ -n "$stray_view_edge" ]]; then
  echo "reprise-view may depend on reprise-core only; found:" >&2
  echo "$stray_view_edge" >&2
  exit 1
fi
```

- [ ] **Step 3: Prüfen, dass das Gate den Verstoß fängt**

```bash
./scripts/check-architecture.sh
```

Erwartet: **FEHLSCHLAG** mit der Zeile
`reprise-view must not depend on GTK, libadwaita, GLib, GStreamer, or zbus`
und darunter der `zbus`-Zeile aus dem Baum. Schlägt es **nicht** fehl, ist
das Gate wirkungslos — dann stimmt die Einfügestelle oder die Variable
`banned_dependency_families` nicht.

- [ ] **Step 4: Prüfen, dass auch die Fremdkanten-Probe greift**

Ersetze in `crates/reprise-view/Cargo.toml` die `zbus`-Zeile durch:

```toml
reprise-runtime-protocol = { path = "../reprise-runtime-protocol" }
```

Dann erneut:

```bash
./scripts/check-architecture.sh
```

Erwartet: **FEHLSCHLAG** mit `reprise-view may depend on reprise-core only; found:`
und `reprise-runtime-protocol` darunter. (Diese Kante ist nicht per se
falsch — sie kann in P1a bewusst zugelassen werden. Das Gate soll nur
beweisen, dass sie nicht unbemerkt entsteht.)

- [ ] **Step 5: Verstoß zurücknehmen und Gate grün sehen**

Entferne die in Step 4 eingefügte Zeile wieder, sodass `[dependencies]` nur
noch `reprise-core` enthält (Stand aus Task 1).

```bash
./scripts/check-architecture.sh
```

Erwartet: **ERFOLG**, mit `== Multi-frontend core boundaries ==` und ohne
Fehlerzeile.

- [ ] **Step 6: Volle Gates fahren**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
```

Erwartet: alle grün, Testzahl unverändert gegenüber der Baseline.

- [ ] **Step 7: Commit**

```bash
git add scripts/check-architecture.sh crates/reprise-view/Cargo.toml
git commit -m "ci: gate reprise-view against toolkit and workspace edges"
```

---

### Task 3: Dateibesitz und Planstände in `AGENTS.md` verankern

**Files:**
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: nichts.
- Produces: die verbindliche Besitzregelung, auf die sich jede P1a-Welle
  beruft. Ohne sie kollidieren parallele Agenten über 64k LOC.

- [ ] **Step 1: Die Sektion schreiben**

Am Ende von `AGENTS.md` anhängen (das Format folgt der bestehenden Sektion
„Completed file ownership — episodes as queue citizens"):

```markdown
## Active file ownership — multi-surface frontends

Spec: `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
Branch: `feature/multi-surface-frontends`

This ownership is ACTIVE. A sibling branch that edits an owned path must
rebase onto this branch first, not merge past it.

### P0 — groundwork (this plan)

| Owner | Files |
| --- | --- |
| multi-surface-frontends | `crates/reprise-view/**`, the `members` list in the workspace `Cargo.toml`, `scripts/check-architecture.sh` |
| multi-surface-frontends | `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`, `docs/superpowers/plans/2026-08-01-multi-surface-p0-s1.md`, this section of `AGENTS.md` |
| sibling branches — excluded | everything under `crates/reprise-gnome/**` until P1a opens |

### P1a — the mobile slice of reprise-view (not yet open)

Package boundaries are drawn when P1a is planned, after the S1 findings
land. Until then no `reprise-gnome` path is owned by this branch.

### Plans parked for P0

These carried an unfinished phase when P0 started (2026-08-01). Each must
land or be explicitly parked before P1a opens, because P1a moves files they
touch:

| Plan | Phase at P0 start |
| --- | --- |
| `docs/plans/podcasts-radio.md` | planned |
| `docs/plans/motion-player.md` | planned |
| `docs/plans/audio-character-mcp.md` | ready-for-review |
| `docs/plans/list-views-fixes.md` | refactored |
| `docs/plans/ux-rules-motion.md` | reviewed |
| `docs/plans/podcast-row-click-selection.md` | coded |
```

- [ ] **Step 2: Prüfen, dass die Datei konsistent bleibt**

```bash
rg -n '^## ' AGENTS.md | tail -5
```

Erwartet: `## Active file ownership — multi-surface frontends` als letzte
oder vorletzte Überschrift, und die bestehende Sektion `## Completed file
ownership — episodes as queue citizens` unverändert darüber.

- [ ] **Step 3: Volle Gates fahren**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
```

Erwartet: alle grün, Testzahl unverändert. (Reine Markdown-Änderung — die
Gates laufen trotzdem, weil `AGENTS.md` sie für **jeden** Commit vorschreibt.)

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md
git commit -m "docs: record active file ownership for the multi-surface work"
```

---

# Teil B — S1 (Android-Spike)

> **Andere Regeln als Teil A.** Dies ist ein Spike. Der Prototyp wird
> **nicht** übernommen; er lebt unter `spikes/android/` und wird am Ende
> gelöscht oder als Referenz markiert, niemals gemergt. Jeder Task endet mit
> einem **schriftlichen Befund**, nicht mit einem grünen Test. Ein Task,
> dessen Befund „geht nicht" lautet, ist trotzdem **erfolgreich** — genau
> dafür existiert der Spike.
>
> **Abbruchregel:** Task 5 (F-Droid) kann das gesamte Vorhaben kippen. Wenn
> sein Befund negativ ist, werden Tasks 6–9 **nicht** ausgeführt; stattdessen
> geht es direkt zu Task 10, und die Spec braucht eine Revision von B11.

**Befund-Datei für alle S1-Tasks:** `docs/research/android-spike-2026-08.md`
Jeder Task hängt einen eigenen Abschnitt an; kein Task überschreibt einen
fremden.

---

### Task 4: Rust nach Android bauen

**Files:**
- Create: `docs/research/android-spike-2026-08.md`
- Create: `spikes/android/` (Arbeitsverzeichnis, nicht committet außer der
  Befund)
- Modify: `.gitignore` (Eintrag `spikes/`)

**Interfaces:**
- Consumes: `reprise-core` und `reprise-view` (leer) aus Task 1.
- Produces: die Gewissheit, dass der Rust-Baum überhaupt für Android baut —
  Vorbedingung für Task 5, 6, 7, 8, 9.

- [ ] **Step 1: Toolchain einrichten**

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk
```

Android-NDK installieren (über `sdkmanager` oder Android Studio) und
`ANDROID_NDK_HOME` setzen. Notiere die verwendete NDK-Version im Befund —
sie ist für Task 5 relevant.

- [ ] **Step 2: `reprise-core` für arm64 bauen**

```bash
cargo ndk -t arm64-v8a build -p reprise-core --release
```

Der interessante Teil ist `rusqlite` mit `bundled` — es kompiliert SQLite
aus C-Quellen und braucht daher einen funktionierenden NDK-Cross-Compiler.
Schlägt es fehl, ist die Fehlermeldung der eigentliche Befund.

- [ ] **Step 3: Die übrigen ABIs prüfen**

```bash
cargo ndk -t armeabi-v7a -t x86_64 build -p reprise-core --release
```

- [ ] **Step 4: `reprise-runtime` prüfen**

```bash
cargo ndk -t arm64-v8a build -p reprise-runtime --release
```

Das ist der Beweis für Spec §2.1 („die Runtime ist transportfrei") auf einem
echten fremden Target. Zieht sie wider Erwarten etwas Linux-Spezifisches,
zeigt es sich hier.

- [ ] **Step 5: Befund schreiben**

Lege `docs/research/android-spike-2026-08.md` an mit Kopf und erstem
Abschnitt:

```markdown
# Android-Spike — Befunde (2026-08)

Spec: `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
Plan: `docs/superpowers/plans/2026-08-01-multi-surface-p0-s1.md`

Dieser Bericht beantwortet die fünf Fragen aus Spec §4/S1. Jeder Abschnitt
endet mit einem Urteil: TRÄGT / TRÄGT MIT AUFLAGEN / TRÄGT NICHT.

## Frage 0 — Baut der Rust-Baum überhaupt für Android?

NDK-Version:
Getestete ABIs:
`reprise-core`:
`reprise-runtime`:
Aufgetretene Hürden:

**Urteil:**
```

Fülle jedes Feld aus. Leere Felder sind ein unfertiger Task.

- [ ] **Step 6: `spikes/` von der Versionskontrolle ausschließen**

In `.gitignore` anhängen:

```gitignore
# Wegwerf-Prototypen (siehe docs/superpowers/plans/2026-08-01-multi-surface-p0-s1.md, Teil B)
spikes/
```

- [ ] **Step 7: Commit** (nur Befund und `.gitignore`, kein Prototyp-Code)

```bash
git add docs/research/android-spike-2026-08.md .gitignore
git commit -m "docs: record android spike finding on cross-compiling the rust tree"
```

---

### Task 5: F-Droid-Baubarkeit — die Abbruchfrage

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`

**Interfaces:**
- Consumes: die funktionierende NDK-Toolchain und die NDK-Version aus Task 4.
- Produces: das Urteil, das über Spec B11 entscheidet. Negativ ⇒ Tasks 6–9
  entfallen.

> Dies ist die Frage mit Show-Stopper-Charakter (Spec §7). Sie steht so früh
> wie möglich — direkt hinter dem lokalen Build, weil man ohne funktionierenden
> Build nichts über die Baubarkeit bei F-Droid sagen kann.

- [ ] **Step 1: Präzedenzfälle sammeln**

Suche in `fdroiddata` (`https://gitlab.com/fdroid/fdroiddata`) nach
Rezepten von Apps, die Rust über das NDK bauen. Suchansätze: `rustup` und
`cargo-ndk` in `metadata/*.yml`. Notiere mindestens zwei konkrete
App-IDs samt der Art, wie ihr `Builds:`-Block die Toolchain bereitstellt
(`sudo:`, `init:`, `prebuild:`).

Findest du keine, ist das selbst ein wichtiger Befund — dann ist das Risiko
deutlich höher als angenommen.

- [ ] **Step 2: Die F-Droid-Buildumgebung gegen die eigenen Anforderungen prüfen**

Klären und im Befund festhalten:

- Welche NDK-Versionen stellt der Buildserver bereit, und passt die aus
  Task 4 dazu?
- Darf das Rezept `rustup` aufrufen, oder muss die Toolchain aus Paketen
  kommen?
- Wie lange darf ein Build laufen? Ein vollständiger Release-Build von
  `reprise-core` inklusive gebündeltem SQLite über drei ABIs ist nicht
  billig — miss die Dauer aus Task 4 und halte sie hier fest.
- Erzwingt F-Droid Netzfreiheit während des Builds? Wenn ja: können alle
  Cargo-Abhängigkeiten vorab vendored werden (`cargo vendor`)?

- [ ] **Step 3: Ein Rezept probeweise formulieren**

Schreibe einen `Builds:`-Eintrag, wie er für Reprise aussehen müsste, und
lege ihn im Befund ab. Er muss noch nicht laufen — er muss zeigen, dass die
nötigen Schritte in den erlaubten Rahmen passen.

- [ ] **Step 4: Wenn möglich, lokal bauen**

`fdroidserver` installieren und den Build gegen einen Klon von `fdroiddata`
versuchen. Gelingt das nicht im verfügbaren Zeitrahmen, notiere das als
offene Restunsicherheit statt es zu erzwingen — ein ehrliches „nicht
verifiziert" ist brauchbarer als ein geratenes „geht".

- [ ] **Step 5: Befund und Urteil schreiben**

Abschnitt anhängen:

```markdown
## Frage 5 — Ist die Rust-NDK-Toolchain im F-Droid-Buildserver baubar?

Präzedenzfälle (App-ID + Vorgehen):
NDK-Verfügbarkeit auf dem Buildserver:
rustup erlaubt / Toolchain-Quelle:
Build-Dauer lokal (drei ABIs, release):
Netzfreiheit / vendoring nötig:
Entwurf des Builds-Eintrags:
Lokal verifiziert (ja/nein/teilweise):

**Urteil:**
```

- [ ] **Step 6: Bei negativem Urteil — Abbruch protokollieren**

Lautet das Urteil TRÄGT NICHT, hänge an:

```markdown
### Konsequenz

Tasks 6–9 dieses Plans entfallen. Spec B11 braucht eine Revision: entweder
ein anderer Veröffentlichungsweg (eigenes Repository, direkte APKs,
Play-only) oder ein anderer technischer Ansatz. Bis dahin wird an der
Android-App kein weiterer Aufwand betrieben.
```

- [ ] **Step 7: Commit**

```bash
git add docs/research/android-spike-2026-08.md
git commit -m "docs: record android spike finding on f-droid buildability"
```

---

### Task 6: Trägt UniFFI die ViewModel-Typen?

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`

**Interfaces:**
- Consumes: die Toolchain aus Task 4, das positive Urteil aus Task 5.
- Produces: die Antwort, die den **Schnitt** von `reprise-view` in P1a
  bestimmt — deshalb steht der Spike vor P1a und nicht danach.

- [ ] **Step 1: Einen repräsentativen Typ auswählen**

Nimm einen echten, nicht erfundenen Typ aus dem heutigen Frontend, der in
P1a nach `reprise-view` wandert. Kandidat mit der höchsten Aussagekraft:
`crates/reprise-gnome/src/ui/podcasts/podcasts_presentation.rs` — er
enthält verschachtelte Structs, ein `Copy`-Enum mit `&'static str`-Feldern
und `BTreeMap`, also genau die Formen, an denen eine FFI-Grenze scheitert.

Lies die Datei und übertrage zwei bis drei ihrer Typen als
Prototyp-Definition nach `spikes/android/`.

- [ ] **Step 2: UniFFI daran ansetzen**

Definiere die Typen mit `uniffi`-Attributen, generiere Kotlin-Bindings und
rufe sie aus einem minimalen Kotlin-Testprogramm auf.

- [ ] **Step 3: Die Grenzen ausmessen**

Halte fest, was **nicht** geht oder Umschreiben erzwingt. Erfahrungsgemäß
kritisch: `&'static str` in Feldern, Lebenszeit-Parameter, Traits als
Rückgabetypen, `BTreeMap`, Enums mit Daten, Callbacks von Kotlin nach Rust.

- [ ] **Step 4: Kosten einer langen Liste messen**

Baue eine Liste von 10.000 Zeilen-ViewModels und miss die Zeit für die
Überquerung der Grenze. Das ist die Zahl, die entscheidet, ob die
Android-App ViewModels **seitenweise** holen muss statt am Stück — und
das wiederum prägt die API, die P1a bauen soll.

- [ ] **Step 5: Befund und Urteil schreiben**

```markdown
## Frage 3 — Trägt UniFFI die Typen von reprise-view?

Getestete Typen (Herkunftsdatei):
Was ohne Änderung trägt:
Was umgeschrieben werden muss:
Was gar nicht geht:
Kosten für 10.000 Zeilen (ms):
Folgerung für den API-Schnitt in P1a (am Stück / seitenweise / anders):

**Urteil:**
```

- [ ] **Step 6: Commit**

```bash
git add docs/research/android-spike-2026-08.md
git commit -m "docs: record android spike finding on uniffi viewmodel bindings"
```

---

### Task 7: Media3 gegen den `playback`-Vertrag

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`

**Interfaces:**
- Consumes: Toolchain aus Task 4.
- Produces: die Antwort, ob P4a den bestehenden Core-Vertrag erfüllen kann
  oder ihn ändern muss.

- [ ] **Step 1: Den Vertrag lesen**

```bash
rg -n 'pub trait|pub fn|pub enum|pub struct' crates/reprise-core/src/playback.rs
```

Notiere die vollständige Signatur jeder Methode, die ein Backend erfüllen
muss. Das ist die Prüfliste für Step 3 — keine Zusammenfassung, die
tatsächlichen Signaturen.

- [ ] **Step 2: Ein Minimal-Backend bauen**

In `spikes/android/`: eine Media3/ExoPlayer-Instanz, angesteuert aus Rust
über JNI. Ziel ist ausschließlich: Datei laden, abspielen, pausieren,
suchen, Position lesen, Ende erkennen.

- [ ] **Step 3: Gegen die Prüfliste abgleichen**

Gehe die Liste aus Step 1 Zeile für Zeile durch und markiere je Methode:
erfüllbar / erfüllbar mit Abweichung / nicht erfüllbar. Für jede Abweichung
notiere die konkrete Ursache.

Besonders zu prüfen, weil es im GTK-Pfad existiert und leicht übersehen
wird: Gapless-Wiedergabe und Crossfade. Halte fest, was Media3 davon
nativ kann.

- [ ] **Step 4: Befund und Urteil schreiben**

```markdown
## Frage 1 — Erfüllt Media3 den playback-Vertrag?

Methoden der Prüfliste (Signatur → Urteil):
Gapless:
Crossfade:
Nötige Vertragsänderungen in reprise-core:

**Urteil:**
```

- [ ] **Step 5: Commit**

```bash
git add docs/research/android-spike-2026-08.md
git commit -m "docs: record android spike finding on media3 against the playback contract"
```

---

### Task 8: Der `MediaSessionService` als Runtime-Wirt

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`

**Interfaces:**
- Consumes: das Media3-Backend aus Task 7.
- Produces: die Bestätigung oder Widerlegung von Spec B7.

- [ ] **Step 1: Den Service bauen**

Ein `MediaSessionService` in `spikes/android/`, der beim Start eine
eingebettete `reprise-runtime`-Instanz erzeugt und hält.

- [ ] **Step 2: Den Lebenszyklus durchspielen**

Prüfe und protokolliere je Szenario, ob Wiedergabe und Runtime überleben:

- App in den Hintergrund, Bildschirm aus, 10 Minuten
- App aus der Übersicht („Recents") gewischt
- Systemseitiger Speicherdruck
- Doze-Modus

- [ ] **Step 3: Die Lease-Frage klären**

Spec §9.3 des Multi-Frontend-Plans verlangt eine Single-Owner-Lease über
eine Dateisperre unter `XDG_RUNTIME_DIR`. Auf Android gibt es das nicht.
Halte fest, was an dessen Stelle tritt — und ob überhaupt etwas nötig ist,
wenn der Service per Konstruktion der einzige Wirt ist.

- [ ] **Step 4: Befund und Urteil schreiben**

```markdown
## Frage 2 — Kann ein MediaSessionService die Runtime beherbergen?

Hintergrund + Bildschirm aus:
Aus Recents gewischt:
Speicherdruck:
Doze:
Ersatz für die Single-Owner-Lease:

**Urteil:**
```

- [ ] **Step 5: Commit**

```bash
git add docs/research/android-spike-2026-08.md
git commit -m "docs: record android spike finding on hosting the runtime in a foreground service"
```

---

### Task 9: SAF gegen den pfadbasierten Scanner

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`

**Interfaces:**
- Consumes: Toolchain aus Task 4.
- Produces: die Antwort auf Spec B11.5 — und damit darauf, ob der Play-Kanal
  überhaupt offensteht und was er `reprise-core` kostet.

- [ ] **Step 1: Den heutigen Zugriffspfad kartieren**

```bash
rg -n 'std::fs|Path::new|PathBuf|read_dir|File::open' crates/reprise-core/src/library/ | wc -l
rg -ln 'std::fs|read_dir' crates/reprise-core/src/library/
```

Notiere die Dateien und die Gesamtzahl der Zugriffsstellen. Das ist die
Größe des möglichen Umbaus.

- [ ] **Step 2: Die vier Anforderungen einzeln prüfen**

Über einen per `ACTION_OPEN_DOCUMENT_TREE` gewählten Baum mit persistierter
Berechtigung, je einzeln:

1. Audiodateien rekursiv aufzählen und Tags lesen (`lofty`)
2. Geschwisterdateien lesen: `cover.jpg`, `.lrc`-Sidecars, `.m3u`
3. **Schreiben** in eine Audiodatei (Tag-Writeback)
4. Änderungen am Baum bemerken — der Ersatz für `notify` (Spec O3)

- [ ] **Step 3: Den Übergabemechanismus klären**

SAF liefert `content://`-URIs, kein Dateisystempfad. Halte fest, welcher
Weg trägt: Dateideskriptoren nach Rust reichen, ein Pfad-Abstraktionstrait
in `reprise-core`, oder etwas anderes. Schätze für den gewählten Weg die
Zahl der betroffenen Zugriffsstellen aus Step 1.

- [ ] **Step 4: Befund und Urteil schreiben**

```markdown
## Frage 4 — Trägt SAF den Scanner?

Zugriffsstellen in reprise-core/src/library (Zahl + Dateien):
1. Aufzählen und Tags lesen:
2. Geschwisterdateien:
3. Tag-Writeback:
4. Änderungserkennung (Ersatz für notify):
Gewählter Übergabemechanismus:
Geschätzte Zahl betroffener Zugriffsstellen:

**Urteil:**
```

- [ ] **Step 5: Commit**

```bash
git add docs/research/android-spike-2026-08.md
git commit -m "docs: record android spike finding on saf against the path-based scanner"
```

---

### Task 10: Gesamturteil und Spec-Rückwirkung

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`
- Modify: `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`

**Interfaces:**
- Consumes: alle Befunde aus Tasks 4–9.
- Produces: die Freigabe (oder Absage) für P1a, und eine Spec, die dem
  Gemessenen entspricht statt dem Vermuteten.

- [ ] **Step 1: Das Gesamturteil schreiben**

```markdown
## Gesamturteil

| Frage | Urteil |
| --- | --- |
| 0 — Rust baut für Android | |
| 5 — F-Droid-Baubarkeit | |
| 3 — UniFFI trägt die Typen | |
| 1 — Media3 erfüllt den Vertrag | |
| 2 — Service beherbergt die Runtime | |
| 4 — SAF trägt den Scanner | |

**Empfehlung:** P1a öffnen / P1a mit Auflagen öffnen / Spec revidieren

**Begründung:**
```

- [ ] **Step 2: Die Spec gegen die Befunde prüfen**

Gehe die Spec Abschnitt für Abschnitt durch und markiere jede Aussage, die
ein Befund widerlegt oder einschränkt. Erwartungsgemäß betroffen: B6
(UniFFI-Annahme), B7 (Service als Wirt), B11.2 und B11.5 (F-Droid, SAF),
P1a (Schnitt), O3 (Änderungserkennung), O4 (APK-Größe).

- [ ] **Step 3: Die Spec anpassen**

Jede widerlegte Annahme wird korrigiert — nicht relativiert. Ergänze bei
jeder Änderung einen Verweis auf den Befund, wie es B5 mit seinem
Revisionsvermerk vorführt. Vermutungen, die sich bestätigt haben, bekommen
ebenfalls einen Verweis; damit ist später erkennbar, was gemessen und was
angenommen ist.

- [ ] **Step 4: Den Prototyp entsorgen**

```bash
rm -rf spikes/android
```

Der Prototyp hat seinen Zweck erfüllt. Was aus ihm gebraucht wird, steht im
Befund; Code, der „vielleicht noch nützlich" ist, wird zur stillen
Altlast.

- [ ] **Step 5: Volle Gates fahren**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
```

Erwartet: alle grün, Testzahl unverändert gegenüber der Baseline aus
Task 1 Step 1.

- [ ] **Step 6: Commit**

```bash
git add docs/research/android-spike-2026-08.md docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md
git commit -m "docs: conclude the android spike and reconcile the spec with its findings"
```

---

## Nach diesem Plan

P1a wird **erst geplant, wenn Task 10 abgeschlossen ist** — der
UniFFI-Befund (Task 6) bestimmt den API-Schnitt, und der SAF-Befund
(Task 9) kann `reprise-core` betreffen. Beides vorher zu planen hieße, auf
Vermutungen zu bauen.
