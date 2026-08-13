---
slug: track-list-selection-anchor
worktree: ~/Projects/reprise-track-list-selection-anchor
branch: feature/track-list-selection-anchor
phase: planned
codex_session:
created: 2026-08-12
spec: docs/superpowers/specs/2026-08-12-track-list-selection-anchor-design.md
---
# Der Selektionsanker der Track-Liste — Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> or `superpowers:executing-plans` and work task by task. Steps are checkboxes.

**Goal:** Eine Shift-Auswahl in der Track-Liste geht vom laufenden Song aus,
wenn der Nutzer selbst noch keinen Anker gesetzt hat — und markiert genau eine
Zeile, wenn es weder Anker noch sichtbaren laufenden Song gibt, statt ab Zeile 0
über die halbe Bibliothek zu ziehen.

**Architecture:** Die Track-Liste bekommt einen eigenen Selektionsanker, den
heute allein GTKs `GtkListBase` intern führt. Die Entscheidungslogik wird eine
reine Funktion (`resolve`), die einen `SelectionOp` zurückgibt; zwei dünne
Seams — ein Zellen-`GestureClick` in der Capture-Phase und ein
`EventControllerKey` auf der `ColumnView` — übersetzen Eingaben in diesen Aufruf
und das Ergebnis in `MultiSelection`-Aufrufe. Der laufende Song wird nie in den
Zustand geschrieben, sondern erst zur Eingabezeit als Rückfall aufgelöst.

**Tech Stack:** Rust, gtk4-rs, libadwaita. Keine neuen Abhängigkeiten.

**Baseline:** `origin/dev` @ `0b65af7035`. Jede Zeilenangabe unten ist gegen
diesen Stand geprüft. Der Haupt-Checkout hängt 220 Commits zurück — im Worktree
von `origin/dev` abzweigen, nicht von einem lokalen Branch.

## Global Constraints

- **Regel-IDs in `docs/ux-rules.md` sind append-only.** Die neue Regel ist
  **NAV-17** (höchste vergebene NAV-ID ist NAV-16). Eine neue `[active]`-Regel
  und der sie umsetzende Code landen im **selben Commit**, und
  `scripts/check-ux-traceability.sh` verlangt mindestens einen Test, dessen
  Name `nav_17` enthält.
- `scripts/check-input-parity.sh` lässt keinen neuen `GestureClick` /
  `GestureDrag` / `DragSource` / `DropTarget` unter
  `crates/reprise-gnome/src/ui` durch, über dem nicht direkt ein Kommentar
  `// input-parity: ACC-8 keyboard=<tested-partner>` steht — und der genannte
  Partner braucht einen echten Test.
- **Dateien bleiben unter 800 Zeilen.** `track_list_context_menu.rs` steht bei
  753, `track_list.rs` bei 598, `track_list_columns.rs` bei 577. Neue Logik
  gehört in die beiden neuen Module; in `track_list_context_menu.rs` darf nur
  die eine `wire`-Zeile aus Task 4 dazukommen.
- **NAV-10b bleibt unangetastet.** Die Wiedergabe darf weiterhin weder
  Selektion noch Fokus noch Viewport bewegen. `current_track_selection.rs` wird
  in diesem Plan **nicht** geändert. Wer meint, dort etwas ändern zu müssen,
  hat den Plan missverstanden — der laufende Song wirkt rein passiv und nur im
  Moment einer Shift-Eingabe.
- **Keine neuen nutzersichtbaren Strings.** Kein `po/`-Aufwand. Wenn ein
  Schritt einen neuen String zu brauchen scheint, den Schritt neu lesen.
- **Display-Tests laufen nicht in einer headless Sandbox ohne Xvfb.** Sie
  werden geschrieben und mit
  `#[ignore = "requires a display; run via xvfb-run"]` markiert; nicht
  behaupten, sie seien gelaufen, wenn kein Xvfb da ist. Nur dateiweise Läufe —
  `cargo test -p reprise-gnome` am Stück ist in diesem Projekt flaky.
- **Immutability:** `resolve` und `validate` sind reine Funktionen. Sie nehmen
  `AnchorState` per Wert und geben einen neuen zurück; sie mutieren nichts.

## Dateikarte

| Datei | Verantwortung nach dieser Änderung |
|---|---|
| `track_list_selection_anchor.rs` (neu) | `SelectMode`, `Anchored`, `AnchorState`, `SelectionOp`; die reinen Funktionen `validate` und `resolve` |
| `track_list_selection_input.rs` (neu) | Der Zellen-`GestureClick` und der `EventControllerKey`; übersetzt Eingaben in `resolve` und `SelectionOp` in `MultiSelection`-Aufrufe |
| `track_list.rs` | `Shared` trägt zusätzlich den `AnchorState` |
| `mod.rs` | Deklariert die beiden neuen Module |
| `track_list_context_menu.rs` | Eine Zeile mehr in der Controller-Verdrahtung (Zeile 428 herum) |
| `track_list_columns.rs`, `track_list_title_column.rs`, `rating_column.rs` | Je ein Aufruf mehr neben dem vorhandenen `wire_context_menu_gesture` |
| `docs/ux-rules.md` | NAV-17 |

Die Zellen-Aufrufstellen sind heute `track_list_columns.rs:294` und `:436`,
`track_list_title_column.rs:65` und `rating_column.rs:40`. **Verlass dich nicht
auf diese Liste** — sie ist der Stand von `0b65af7035`. Finde die tatsächliche
Menge mit `git grep -n "wire_context_menu_gesture" crates/reprise-gnome/src`
und bediene jede gefundene Stelle. Wenn eine weitere Datei angefasst werden
muss, damit das Verhalten stimmt, fass sie an.

---

### Task 1: Die reine Entscheidungslogik

**Files:**
- Create: `crates/reprise-gnome/src/ui/track_list/track_list_selection_anchor.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/mod.rs` (Moduldeklaration)
- Test: im selben Modul (`#[cfg(test)] mod tests`), wie es die Nachbarn tun

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `pub(super) enum SelectMode { Only, Toggle, Range, RangeAdditive }`
  - `pub(super) struct Anchored { pub position: u32, pub track_id: i64 }` (`Clone, Copy, Debug, Eq, PartialEq`)
  - `pub(super) struct AnchorState { pub anchor: Option<Anchored>, pub cursor: Option<Anchored> }` (`Clone, Copy, Debug, Default, Eq, PartialEq`)
  - `pub(super) enum SelectionOp { SelectOnly(u32), Toggle(u32), SelectRange { start: u32, len: u32, replace: bool } }` (`Clone, Copy, Debug, Eq, PartialEq`)
  - `pub(super) fn validate(state: AnchorState, lookup: impl Fn(u32) -> Option<i64>) -> AnchorState`
  - `pub(super) fn resolve(state: AnchorState, playing: Option<Anchored>, target: Anchored, mode: SelectMode) -> (SelectionOp, AnchorState)`

- [ ] **Step 1: Die fehlschlagenden Tests schreiben**

Lege die Datei mit Modulkopf, den Typen als leere Hülsen und den Tests an.
Beginne mit `todo!()` in beiden Funktionen, damit die Tests kompilieren und
scheitern.

```rust
//! NAV-17: der Selektionsanker der Track-Liste.
//!
//! GTKs `GtkListBase` führt einen eigenen Anker, den nur ein Klick oder eine
//! Fokusbewegung setzt. NAV-10b verbietet der Wiedergabe beides, also bleibt
//! GTKs Anker beim Abspielen stehen — nach einem Ansichtswechsel bei Zeile 0,
//! worauf Shift+Klick über die halbe Bibliothek zieht. Dieses Modul führt den
//! Anker deshalb selbst.
//!
//! Reine Logik, kein GTK: `validate` wirft veraltete Positionen weg, `resolve`
//! entscheidet. Der laufende Song wird nie gespeichert, sondern von `resolve`
//! als Rückfall entgegengenommen — so kann die Wiedergabe den Anker nicht im
//! Rücken des Nutzers verschieben. Naher Verwandter von
//! `podcasts_selection::apply_select`, das dieselbe Anker-Disziplin für die
//! Episodenzeilen führt.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectMode {
    Only,
    Toggle,
    Range,
    RangeAdditive,
}

/// Eine Zeile, festgehalten als Position **und** Track-id. Die Position ist
/// das, womit gerechnet wird — in einer Playlist darf derselbe Track mehrfach
/// stehen, weshalb auch der Löschpfad bewusst mit Positionen rechnet. Die id
/// dient allein dazu, in `validate` zu erkennen, dass die Position nach einem
/// Sortier-, Filter- oder Reload-Wechsel auf eine andere Zeile zeigt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Anchored {
    pub position: u32,
    pub track_id: i64,
}

/// `anchor` ist der feste Ausgangspunkt einer Spanne, `cursor` das bewegliche
/// Ende und zugleich unsere Kopie von GTKs Fokuszeile — GTK4 gibt die
/// Fokusposition einer `ColumnView` nicht öffentlich her.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AnchorState {
    pub anchor: Option<Anchored>,
    pub cursor: Option<Anchored>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectionOp {
    SelectOnly(u32),
    Toggle(u32),
    SelectRange { start: u32, len: u32, replace: bool },
}

pub(super) fn validate(state: AnchorState, lookup: impl Fn(u32) -> Option<i64>) -> AnchorState {
    todo!("Task 1 Step 3")
}

pub(super) fn resolve(
    state: AnchorState,
    playing: Option<Anchored>,
    target: Anchored,
    mode: SelectMode,
) -> (SelectionOp, AnchorState) {
    todo!("Task 1 Step 3")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(position: u32) -> Anchored {
        Anchored { position, track_id: i64::from(position) + 1_000 }
    }

    /// Das Modell dieser Tests: Zeile n trägt Track-id n + 1000.
    fn stable(position: u32) -> Option<i64> {
        (position < 100).then(|| i64::from(position) + 1_000)
    }

    #[test]
    fn nav_17_a_plain_click_sets_both_anchor_and_cursor() {
        let (op, state) = resolve(AnchorState::default(), None, at(7), SelectMode::Only);
        assert_eq!(op, SelectionOp::SelectOnly(7));
        assert_eq!(state.anchor, Some(at(7)));
        assert_eq!(state.cursor, Some(at(7)));
    }

    #[test]
    fn nav_17_a_toggle_moves_both_too() {
        let start = AnchorState { anchor: Some(at(3)), cursor: Some(at(3)) };
        let (op, state) = resolve(start, None, at(9), SelectMode::Toggle);
        assert_eq!(op, SelectionOp::Toggle(9));
        assert_eq!(state.anchor, Some(at(9)));
        assert_eq!(state.cursor, Some(at(9)));
    }

    #[test]
    fn nav_17_a_range_never_moves_the_anchor() {
        let start = AnchorState { anchor: Some(at(4)), cursor: Some(at(4)) };
        let (op, state) = resolve(start, None, at(8), SelectMode::Range);
        assert_eq!(op, SelectionOp::SelectRange { start: 4, len: 5, replace: true });
        assert_eq!(state.anchor, Some(at(4)), "der Anker bleibt stehen");
        assert_eq!(state.cursor, Some(at(8)), "nur der Cursor folgt");

        // Eine zweite Spanne wird neu vom Anker genommen, nicht angebaut.
        let (op, _) = resolve(state, None, at(2), SelectMode::Range);
        assert_eq!(op, SelectionOp::SelectRange { start: 2, len: 3, replace: true });
    }

    #[test]
    fn nav_17_a_range_without_any_anchor_selects_a_single_row() {
        let (op, state) = resolve(AnchorState::default(), None, at(42), SelectMode::Range);
        assert_eq!(op, SelectionOp::SelectOnly(42), "kein Aufziehen ab Zeile 0");
        assert_eq!(state.anchor, Some(at(42)), "der Klick stiftet den Anker");
        assert_eq!(state.cursor, Some(at(42)));
    }

    #[test]
    fn nav_17_a_range_without_an_anchor_starts_at_the_playing_row() {
        let (op, state) = resolve(AnchorState::default(), Some(at(5)), at(9), SelectMode::Range);
        assert_eq!(op, SelectionOp::SelectRange { start: 5, len: 5, replace: true });
        assert_eq!(state.anchor, Some(at(5)), "der laufende Song wird zum Anker");
        assert_eq!(state.cursor, Some(at(9)));
    }

    #[test]
    fn nav_17_an_own_anchor_beats_the_playing_row() {
        let start = AnchorState { anchor: Some(at(20)), cursor: Some(at(20)) };
        let (op, _) = resolve(start, Some(at(5)), at(22), SelectMode::Range);
        assert_eq!(op, SelectionOp::SelectRange { start: 20, len: 3, replace: true });
    }

    #[test]
    fn nav_17_a_backwards_range_is_ordered() {
        let start = AnchorState { anchor: Some(at(30)), cursor: Some(at(30)) };
        let (op, _) = resolve(start, None, at(25), SelectMode::Range);
        assert_eq!(op, SelectionOp::SelectRange { start: 25, len: 6, replace: true });
    }

    #[test]
    fn nav_17_an_additive_range_keeps_the_rest_of_the_selection() {
        let start = AnchorState { anchor: Some(at(4)), cursor: Some(at(4)) };
        let (op, _) = resolve(start, None, at(6), SelectMode::RangeAdditive);
        assert_eq!(op, SelectionOp::SelectRange { start: 4, len: 3, replace: false });
    }

    #[test]
    fn nav_17_a_stale_anchor_is_dropped_and_the_playing_row_takes_over() {
        // Zeile 4 trägt nach einem Sortierwechsel einen anderen Track.
        let start = AnchorState {
            anchor: Some(Anchored { position: 4, track_id: 999 }),
            cursor: Some(Anchored { position: 4, track_id: 999 }),
        };
        let validated = validate(start, stable);
        assert_eq!(validated, AnchorState::default());

        let (op, _) = resolve(validated, Some(at(5)), at(9), SelectMode::Range);
        assert_eq!(op, SelectionOp::SelectRange { start: 5, len: 5, replace: true });
    }

    #[test]
    fn nav_17_an_anchor_past_the_end_is_dropped() {
        let start = AnchorState { anchor: Some(at(500)), cursor: Some(at(500)) };
        assert_eq!(validate(start, stable), AnchorState::default());
    }

    #[test]
    fn nav_17_a_live_anchor_survives_validation() {
        let start = AnchorState { anchor: Some(at(4)), cursor: Some(at(8)) };
        assert_eq!(validate(start, stable), start);
    }

    #[test]
    fn nav_17_validation_drops_each_half_on_its_own() {
        let start = AnchorState {
            anchor: Some(at(4)),
            cursor: Some(Anchored { position: 8, track_id: 999 }),
        };
        let validated = validate(start, stable);
        assert_eq!(validated.anchor, Some(at(4)));
        assert_eq!(validated.cursor, None);
    }
}
```

Trage das Modul in `mod.rs` ein, neben den anderen `track_list_*`-Modulen:

```rust
pub(in crate::ui) mod track_list_selection_anchor;
```

- [ ] **Step 2: Tests laufen lassen und Fehlschlag bestätigen**

Run: `cargo test -p reprise-gnome track_list_selection_anchor 2>&1 | tail -20`
Expected: Die Tests panicken mit `not yet implemented` aus den `todo!()`.

- [ ] **Step 3: Die beiden Funktionen implementieren**

```rust
pub(super) fn validate(state: AnchorState, lookup: impl Fn(u32) -> Option<i64>) -> AnchorState {
    let keep = |candidate: Option<Anchored>| {
        candidate.filter(|held| lookup(held.position) == Some(held.track_id))
    };
    AnchorState { anchor: keep(state.anchor), cursor: keep(state.cursor) }
}

pub(super) fn resolve(
    state: AnchorState,
    playing: Option<Anchored>,
    target: Anchored,
    mode: SelectMode,
) -> (SelectionOp, AnchorState) {
    let moved = AnchorState { anchor: Some(target), cursor: Some(target) };
    match mode {
        SelectMode::Only => (SelectionOp::SelectOnly(target.position), moved),
        SelectMode::Toggle => (SelectionOp::Toggle(target.position), moved),
        SelectMode::Range | SelectMode::RangeAdditive => {
            // Ohne eigenen Anker fällt die Spanne auf den laufenden Song
            // zurück; gibt es auch den nicht, ist eine Spanne bedeutungslos
            // und die Eingabe wird zum einfachen Klick — das ist NAV-17s
            // Kern, denn GTK zöge hier ab Zeile 0 auf.
            let Some(anchor) = state.anchor.or(playing) else {
                return (SelectionOp::SelectOnly(target.position), moved);
            };
            let (start, end) = if anchor.position <= target.position {
                (anchor.position, target.position)
            } else {
                (target.position, anchor.position)
            };
            let op = SelectionOp::SelectRange {
                start,
                len: end - start + 1,
                replace: matches!(mode, SelectMode::Range),
            };
            // Eine Spanne bewegt den Anker nie — sie wird bei der nächsten
            // Eingabe wieder von ihm genommen, nicht an das Ergebnis angebaut.
            (op, AnchorState { anchor: Some(anchor), cursor: Some(target) })
        }
    }
}
```

- [ ] **Step 4: Tests laufen lassen und Erfolg bestätigen**

Run: `cargo test -p reprise-gnome track_list_selection_anchor 2>&1 | grep "^test result:"`
Expected: `test result: ok.` mit 11 bestandenen Tests.

- [ ] **Step 5: Clippy und Commit**

Run: `cargo clippy -p reprise-gnome --all-targets 2>&1 | grep -E "^(error|warning)" | head`
Expected: keine neuen Meldungen aus der neuen Datei.

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list_selection_anchor.rs \
        crates/reprise-gnome/src/ui/track_list/mod.rs
git commit -m "feat: pure selection anchor logic for the track list"
```

---

### Task 2: `Shared` trägt den Anker

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list.rs` (`Shared`, ab Zeile 96)
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_selection_anchor.rs` (Zugriffshelfer)

**Interfaces:**
- Consumes: `AnchorState`, `Anchored`, `validate` aus Task 1.
- Produces:
  - Feld `selection_anchor: std::cell::Cell<AnchorState>` auf `Shared`
  - `pub(super) fn live_anchor_state(shared: &Shared) -> AnchorState`
  - `pub(super) fn store_anchor_state(shared: &Shared, state: AnchorState)`
  - `pub(super) fn anchored_at(shared: &Shared, position: u32) -> Option<Anchored>`
  - `pub(super) fn playing_anchor(shared: &Shared) -> Option<Anchored>`

- [ ] **Step 1: Den fehlschlagenden Test schreiben**

Ans Ende des `tests`-Moduls in `track_list_selection_anchor.rs`. Der Test
belegt, dass der gespeicherte Anker gegen das echte Modell validiert wird —
also dass ein Reload ihn wirklich fallen lässt.

```rust
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_a_reload_drops_a_stale_anchor_against_the_real_model() {
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
        for id in 1..=20 {
            tx.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
                (id, format!("/synthetic/{id:03}.flac"), format!("Track {id:03}")),
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let track_list = crate::ui::track_list::TrackList::new(
            std::rc::Rc::new(conn),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let shared = &track_list.shared;

        let real = anchored_at(shared, 3).expect("Zeile 3 existiert");
        store_anchor_state(shared, AnchorState { anchor: Some(real), cursor: Some(real) });
        assert_eq!(live_anchor_state(shared).anchor, Some(real));

        // Eine Position, die es gibt, aber mit einer id, die dort nicht steht.
        let stale = Anchored { position: 3, track_id: real.track_id + 5_000 };
        store_anchor_state(shared, AnchorState { anchor: Some(stale), cursor: Some(stale) });
        assert_eq!(
            live_anchor_state(shared),
            AnchorState::default(),
            "ein Anker, dessen Zeile einen anderen Track trägt, wird verworfen"
        );
    }
```

- [ ] **Step 2: Test laufen lassen und Fehlschlag bestätigen**

Run: `cargo test -p reprise-gnome track_list_selection_anchor -- --ignored --exact ui::track_list::track_list_selection_anchor::tests::nav_17_a_reload_drops_a_stale_anchor_against_the_real_model 2>&1 | tail -20`
Expected: Kompilierfehler — `live_anchor_state`, `store_anchor_state` und
`anchored_at` gibt es noch nicht.

- [ ] **Step 3: Feld und Helfer implementieren**

In `track_list.rs`, in `struct Shared` (ab Zeile 96) neben den anderen
`Cell`-Feldern:

```rust
    /// NAV-17: der eigene Selektionsanker. `Cell`, weil `AnchorState` `Copy`
    /// ist und kein Borrow je eine GTK-Rückrufkette überspannen darf.
    pub(in crate::ui) selection_anchor:
        std::cell::Cell<super::track_list_selection_anchor::AnchorState>,
```

Im Konstruktor von `Shared` (dort, wo die übrigen Felder initialisiert werden)
`selection_anchor: std::cell::Cell::default(),` ergänzen.

Ans Ende von `track_list_selection_anchor.rs`, vor dem `tests`-Modul:

```rust
use super::track_list::Shared;

/// Liest den gespeicherten Anker und wirft dabei weg, was durch einen Sortier-,
/// Filter- oder Reload-Wechsel veraltet ist. Jeder Lesepfad geht hierdurch, was
/// das Verwerfen an ein einziges Vorkommen bindet, statt es an jede Stelle zu
/// hängen, die das Modell umbaut.
pub(super) fn live_anchor_state(shared: &Shared) -> AnchorState {
    let state = validate(shared.selection_anchor.get(), |position| {
        shared.model.track_at(position).map(|track| track.id)
    });
    shared.selection_anchor.set(state);
    state
}

pub(super) fn store_anchor_state(shared: &Shared, state: AnchorState) {
    shared.selection_anchor.set(state);
}

pub(super) fn anchored_at(shared: &Shared, position: u32) -> Option<Anchored> {
    shared
        .model
        .track_at(position)
        .map(|track| Anchored { position, track_id: track.id })
}

/// Der laufende Song als Rückfallanker — aufgelöst im Moment der Eingabe, nie
/// gespeichert. Genau das hält NAV-10b heil: Die Wiedergabe schreibt keinen
/// Zustand und kann den Anker daher nicht im Rücken des Nutzers verschieben.
pub(super) fn playing_anchor(shared: &Shared) -> Option<Anchored> {
    let track_id = shared.playing_track_id.get()?;
    let ids = shared.current_view_ids();
    let is_queue = matches!(
        *shared.source.borrow(),
        reprise_core::view_source::ViewSource::Queue
    );
    let position = super::current_track_selection::visible_position_for_track_in_source(
        &ids, track_id, None, is_queue,
    )?;
    Some(Anchored { position, track_id })
}
```

`visible_position_for_track_in_source` ist heute `pub(super)`
(`current_track_selection.rs:57`) und damit von hier aus erreichbar. Falls der
Sichtbarkeitsbereich das nicht hergibt, weite ihn auf `pub(in crate::ui)` —
nicht die Funktion kopieren.

- [ ] **Step 4: Test laufen lassen und Erfolg bestätigen**

Run: `xvfb-run -a cargo test -p reprise-gnome track_list_selection_anchor -- --ignored 2>&1 | grep "^test result:"`
Expected: `test result: ok.` — falls kein Xvfb vorhanden ist, mindestens
`cargo test -p reprise-gnome track_list_selection_anchor 2>&1 | grep "^test result:"`
grün und den Display-Test als ungelaufen melden.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list.rs \
        crates/reprise-gnome/src/ui/track_list/track_list_selection_anchor.rs
git commit -m "feat: keep the track list selection anchor on Shared"
```

---

### Task 3: Der Zeiger-Seam und NAV-17

**Files:**
- Create: `crates/reprise-gnome/src/ui/track_list/track_list_selection_input.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/mod.rs`
- Modify: jede Stelle, die `wire_context_menu_gesture` aufruft (siehe
  Dateikarte — mit `git grep` ermitteln, nicht abschreiben)
- Modify: `docs/ux-rules.md`

**Interfaces:**
- Consumes: alles aus Task 1 und 2.
- Produces:
  - `pub(in crate::ui) fn wire_cell_selection(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem, shared: &Rc<Shared>)`
  - `pub(super) fn pointer_mode(state: gtk4::gdk::ModifierType) -> Option<SelectMode>`
  - `pub(super) fn apply(shared: &Shared, op: SelectionOp)`

- [ ] **Step 1: Die fehlschlagenden Tests schreiben**

Neue Datei, mit `pointer_mode` und `apply` als `todo!()`.

```rust
//! NAV-17: Eingabe-Seams für den Selektionsanker der Track-Liste.
//!
//! Der Zellen-Gesture muss in der **Capture-Phase** liegen. GTKs
//! Selektionsmaschinerie hängt am `GtkListItemWidget`, also an einem Vorfahren
//! der Zelle, und gewinnt in der Bubble-Phase — `rating.rs` hält im Modulkopf
//! fest, wie ein einfacher `GestureClick` dort im Feld verlor. Da die
//! Capture-Phase vollständig vor der Bubble-Phase läuft, kommen wir davor.
//!
//! Beansprucht wird nur der Shift-Fall. Ein Klick ohne Shift merkt sich bloß
//! die Zeile und lässt das Ereignis weiterlaufen, damit GTK wie gewohnt
//! selektiert. Beobachten statt beanspruchen ist dort nötig, weil Ctrl+Klick
//! eine mehrzeilige Auswahl hinterlässt, aus der sich die getroffene Zeile
//! hinterher nicht mehr ablesen ließe.

use std::rc::Rc;

use gtk4::prelude::*;

use super::track_list::Shared;
use super::track_list_selection_anchor::{
    anchored_at, live_anchor_state, playing_anchor, resolve, store_anchor_state, SelectMode,
    SelectionOp,
};

pub(super) fn pointer_mode(state: gtk4::gdk::ModifierType) -> Option<SelectMode> {
    todo!("Task 3 Step 3")
}

pub(super) fn apply(shared: &Shared, op: SelectionOp) {
    todo!("Task 3 Step 3")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::gdk::ModifierType;

    #[test]
    fn nav_17_shift_claims_a_range_and_ctrl_shift_an_additive_one() {
        assert_eq!(pointer_mode(ModifierType::SHIFT_MASK), Some(SelectMode::Range));
        assert_eq!(
            pointer_mode(ModifierType::SHIFT_MASK | ModifierType::CONTROL_MASK),
            Some(SelectMode::RangeAdditive)
        );
    }

    #[test]
    fn nav_17_input_without_shift_is_observed_not_claimed() {
        assert_eq!(pointer_mode(ModifierType::empty()), None);
        assert_eq!(pointer_mode(ModifierType::CONTROL_MASK), None);
        assert_eq!(pointer_mode(ModifierType::ALT_MASK), None);
    }
}
```

- [ ] **Step 2: Tests laufen lassen und Fehlschlag bestätigen**

Run: `cargo test -p reprise-gnome track_list_selection_input 2>&1 | tail -20`
Expected: Panik aus `todo!()`.

- [ ] **Step 3: Den Seam implementieren**

```rust
pub(super) fn pointer_mode(state: gtk4::gdk::ModifierType) -> Option<SelectMode> {
    if !state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
        // Kein Shift: nur beobachten, GTK selektiert weiter selbst.
        return None;
    }
    Some(if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
        SelectMode::RangeAdditive
    } else {
        SelectMode::Range
    })
}

pub(super) fn apply(shared: &Shared, op: SelectionOp) {
    let selection = &shared.selection;
    match op {
        SelectionOp::SelectOnly(position) => {
            selection.select_item(position, true);
        }
        SelectionOp::Toggle(position) => {
            if selection.is_selected(position) {
                selection.unselect_item(position);
            } else {
                selection.select_item(position, false);
            }
        }
        SelectionOp::SelectRange { start, len, replace } => {
            selection.select_range(start, len, replace);
            // Der Nachweis, den das ptr-e2e-Szenario aus Task 5 liest: Es
            // prüft das Ergebnis über den stderr-Log der App, weil nur echte
            // Eingaben belegen können, dass das Ereignis überhaupt ankommt.
            tracing::info!(start, len, replace, "selection anchor range applied");
        }
    }
}

/// Hängt den Anker-Gesture an eine frisch `setup`-te Zelle — dieselbe Stelle
/// und dieselbe Lebensdauer wie `wire_context_menu_gesture`, das mit
/// `ListItem::position()` bereits einen stabilen Zeilenhandle über alle
/// Rebinds hinweg hat.
// input-parity: ACC-8 keyboard=nav_17_shift_arrow_extends_from_the_playing_row
pub(in crate::ui) fn wire_cell_selection(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    // Vor GTKs Selektionsmaschinerie am Vorfahren — siehe Modulkopf.
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);

    let item = item.clone();
    let shared = shared.clone();
    gesture.connect_pressed(move |gesture, _n_press, _x, _y| {
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            tracing::warn!("selection anchor: list item has no valid position; ignoring click");
            return;
        }
        let Some(target) = anchored_at(&shared, position) else {
            tracing::warn!(position, "selection anchor: no track at the clicked row");
            return;
        };
        let modifiers = gesture.current_event_state();
        let Some(mode) = pointer_mode(modifiers) else {
            // Beobachten: Anker merken, Ereignis weiterreichen. Auch der erste
            // Press eines Doppelklicks tut das — genau wie `pointer_intent`
            // es bei den Podcast-Zeilen begründet.
            store_anchor_state(
                &shared,
                super::track_list_selection_anchor::AnchorState {
                    anchor: Some(target),
                    cursor: Some(target),
                },
            );
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let (op, next) = resolve(
            live_anchor_state(&shared),
            playing_anchor(&shared),
            target,
            mode,
        );
        apply(&shared, op);
        store_anchor_state(&shared, next);
    });

    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}
```

Modul in `mod.rs` eintragen:

```rust
pub(in crate::ui) mod track_list_selection_input;
```

Dann an **jeder** Stelle, die `wire_context_menu_gesture` aufruft, direkt
darunter den neuen Aufruf ergänzen. Ermittle die Stellen mit
`git grep -n "wire_context_menu_gesture" crates/reprise-gnome/src`; zum Stand
`0b65af7035` sind es vier. Muster (die Bindungsnamen unterscheiden sich je
Aufrufstelle — `cover`, `label`, das `RatingWidget`):

```rust
        super::track_list_selection_input::wire_cell_selection(&cover, item, &shared);
```

- [ ] **Step 4: Tests und Gates laufen lassen**

Run: `cargo test -p reprise-gnome track_list_selection_input 2>&1 | grep "^test result:"`
Expected: `test result: ok.` mit 2 Tests.

Run: `scripts/check-input-parity.sh 2>&1 | tail -5`
Expected: grün. Schlägt es fehl, weil der genannte Tastatur-Partner noch nicht
existiert, ist das erwartet — Task 4 liefert ihn; dann Task 3 und 4 zusammen
committen statt den Kommentar zu verwässern.

- [ ] **Step 5: NAV-17 in `docs/ux-rules.md` eintragen**

Neben die übrigen NAV-Regeln, im Stil der Nachbarn:

```markdown
- **NAV-17** [active] [gtk] — **Eine Shift-Auswahl geht von einem Anker aus,
  nicht vom Listenanfang.** Den Anker setzt der Nutzer selbst: der letzte Klick
  ohne Shift. Gibt es keinen — frisch geladene Ansicht, gewechselte Sortierung
  oder Filterung —, tritt die Zeile des laufenden Songs an seine Stelle, sofern
  sie in der aktuellen Ansicht vorkommt. Gibt es auch die nicht, markiert
  Shift+Klick genau die getroffene Zeile, statt vom Listenanfang aufzuziehen.
  Eine Spanne bewegt den Anker nie; sie wird bei der nächsten Eingabe erneut von
  ihm genommen. Der laufende Song wirkt dabei rein passiv: Er bekommt weder
  Selektion noch Tastaturfokus, und die Wiedergabe bewegt weiterhin nichts —
  NAV-10b bleibt unberührt.
```

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list_selection_input.rs \
        crates/reprise-gnome/src/ui/track_list/mod.rs \
        crates/reprise-gnome/src/ui/track_list/track_list_columns.rs \
        crates/reprise-gnome/src/ui/track_list/track_list_title_column.rs \
        crates/reprise-gnome/src/ui/track_list/rating_column.rs \
        docs/ux-rules.md
git commit -m "feat: shift click selects from the playing row (NAV-17)"
```

---

### Task 4: Der Tasten-Seam

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_selection_input.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_context_menu.rs`
  (eine Zeile in der Controller-Verdrahtung, Zeile 428 herum)

**Interfaces:**
- Consumes: alles aus Task 1–3.
- Produces:
  - `pub(super) enum KeyIntent { Extend(i32), ExtendInPlace }`
  - `pub(super) fn key_intent(key: gtk4::gdk::Key, state: gtk4::gdk::ModifierType) -> Option<KeyIntent>`
  - `pub(in crate::ui) fn wire(column_view: &gtk4::ColumnView, shared: &Rc<Shared>)`

- [ ] **Step 1: Die fehlschlagenden Tests schreiben**

Ans `tests`-Modul in `track_list_selection_input.rs`.

```rust
    #[test]
    fn nav_17_shift_arrows_step_and_shift_space_stays() {
        use gtk4::gdk::Key;
        assert_eq!(
            key_intent(Key::Down, ModifierType::SHIFT_MASK),
            Some(KeyIntent::Extend(1))
        );
        assert_eq!(
            key_intent(Key::Up, ModifierType::SHIFT_MASK),
            Some(KeyIntent::Extend(-1))
        );
        assert_eq!(
            key_intent(Key::space, ModifierType::SHIFT_MASK),
            Some(KeyIntent::ExtendInPlace)
        );
    }

    #[test]
    fn nav_17_arrows_without_shift_stay_with_gtk() {
        use gtk4::gdk::Key;
        assert_eq!(key_intent(Key::Down, ModifierType::empty()), None);
        assert_eq!(key_intent(Key::space, ModifierType::empty()), None);
        // Alt+Pfeil gehört dem Reorder-Controller.
        assert_eq!(key_intent(Key::Down, ModifierType::ALT_MASK), None);
        assert_eq!(
            key_intent(Key::Down, ModifierType::SHIFT_MASK | ModifierType::ALT_MASK),
            None
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_a_single_row_selection_pulls_the_anchor_along() {
        // Das ist der Nachzug für Pfeiltasten ohne Modifier: GTK bewegt Fokus
        // und Auswahl selbst, und der Anker darf dabei nicht zurückbleiben.
        let (track_list, window) = display_fixture(40);
        let shared = &track_list.shared;
        store_anchor_state(shared, Default::default());

        shared.selection.select_item(6, true);
        while gtk4::glib::MainContext::default().iteration(false) {}

        let expected = anchored_at(shared, 6).unwrap();
        let state = live_anchor_state(shared);
        assert_eq!(state.anchor, Some(expected), "der Anker folgt einer Einzelauswahl");
        assert_eq!(state.cursor, Some(expected));

        // Eine mehrzeilige Auswahl darf ihn dagegen nicht verschieben — sonst
        // wüsste man nach Ctrl+Klick nicht mehr, welche Zeile gemeint war.
        shared.selection.select_range(10, 3, true);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert_eq!(
            live_anchor_state(shared).anchor,
            Some(expected),
            "eine Mehrfachauswahl lässt den Anker stehen"
        );

        window.close();
    }
```

Dazu eine Fixture-Hilfsfunktion, damit die Display-Tests sie nicht je einzeln
aufbauen — ins selbe `tests`-Modul, oberhalb der Tests:

```rust
    /// Ein präsentiertes Fenster mit `rows` synthetischen Zeilen. Gibt das
    /// Fenster mit zurück, weil es bis zum Testende leben muss.
    fn display_fixture(rows: i64) -> (std::rc::Rc<crate::ui::track_list::TrackList>, gtk4::Window) {
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
        for id in 1..=rows {
            tx.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
                (id, format!("/synthetic/{id:03}.flac"), format!("Track {id:03}")),
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let track_list = crate::ui::track_list::TrackList::new(
            std::rc::Rc::new(conn),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let window = gtk4::Window::builder()
            .default_width(900)
            .default_height(320)
            .child(track_list.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        (track_list, window)
    }
```

`TrackList::new` gibt heute ein `Rc<TrackList>` zurück — so nutzen es die
Nachbartests in `current_track_selection_tests.rs`. Weicht die Signatur ab,
passe den Rückgabetyp der Hilfsfunktion an, nicht die Tests.

Und der Display-Test, dessen Name der `input-parity`-Kommentar aus Task 3
nennt — er muss exakt so heißen:

```rust
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_shift_arrow_extends_from_the_playing_row() {
        let (track_list, window) = display_fixture(40);
        let shared = &track_list.shared;
        // Kein eigener Anker, aber Zeile 10 läuft.
        let playing = shared.model.track_at(10).unwrap().id;
        shared.playing_track_id.set(Some(playing));

        let target = anchored_at(shared, 12).unwrap();
        let (op, next) = resolve(
            live_anchor_state(shared),
            playing_anchor(shared),
            target,
            SelectMode::Range,
        );
        apply(shared, op);
        store_anchor_state(shared, next);

        for position in 10..=12 {
            assert!(
                shared.selection.is_selected(position),
                "Zeile {position} muss zur Spanne gehören"
            );
        }
        assert!(!shared.selection.is_selected(9), "die Spanne beginnt beim laufenden Song");
        assert!(!shared.selection.is_selected(13));

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_a_range_does_not_move_the_viewport() {
        // NAV-10b: Markieren verschiebt nichts. Der Zeigerpfad ruft bewusst
        // kein `scroll_to` — nur der Tastaturpfad tut das, und dort ist es
        // gewollt.
        let (track_list, window) = display_fixture(200);
        let shared = &track_list.shared;
        shared
            .column_view
            .scroll_to(120, None, gtk4::ListScrollFlags::FOCUS, None);
        let adjustment = shared.column_view.vadjustment().unwrap();
        // `scroll_to` setzt sich über spätere Schleifendurchläufe; einmal
        // pumpen reicht nicht. Das ist Testaufbau, nicht das Prüfobjekt.
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            adjustment.value() > 0.0
        });
        let before = adjustment.value();
        assert!(before > 0.0, "precondition: die Liste muss vom Anfang weg gescrollt sein");

        apply(shared, SelectionOp::SelectRange { start: 3, len: 9, replace: true });
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(
            (adjustment.value() - before).abs() < 1.0,
            "eine Spanne weit oberhalb des Viewports darf ihn nicht dorthin ziehen"
        );

        window.close();
    }
```

- [ ] **Step 2: Tests laufen lassen und Fehlschlag bestätigen**

Run: `cargo test -p reprise-gnome track_list_selection_input 2>&1 | tail -20`
Expected: Kompilierfehler — `KeyIntent` und `key_intent` fehlen.

- [ ] **Step 3: Den Tasten-Seam implementieren**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KeyIntent {
    /// Schrittweite relativ zum Cursor.
    Extend(i32),
    /// Shift+Space nimmt die Spanne ohne Bewegung neu vom Anker.
    ExtendInPlace,
}

pub(super) fn key_intent(
    key: gtk4::gdk::Key,
    state: gtk4::gdk::ModifierType,
) -> Option<KeyIntent> {
    if !state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
        return None;
    }
    // Alt+Pfeil ist Zeilen-Reorder (`track_list_keyboard_reorder`), nicht Auswahl.
    if state.contains(gtk4::gdk::ModifierType::ALT_MASK) {
        return None;
    }
    match key {
        gtk4::gdk::Key::Down | gtk4::gdk::Key::KP_Down => Some(KeyIntent::Extend(1)),
        gtk4::gdk::Key::Up | gtk4::gdk::Key::KP_Up => Some(KeyIntent::Extend(-1)),
        gtk4::gdk::Key::space | gtk4::gdk::Key::KP_Space => Some(KeyIntent::ExtendInPlace),
        _ => None,
    }
}

pub(in crate::ui) fn wire(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let keys = gtk4::EventControllerKey::new();
    // Vor GTKs eigener Auswahl-Navigation.
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let shared = shared.clone();
    let column_view = column_view.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(intent) = key_intent(key, modifiers) else {
            return gtk4::glib::Propagation::Proceed;
        };
        let state = live_anchor_state(&shared);
        let mode = if modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            SelectMode::RangeAdditive
        } else {
            SelectMode::Range
        };
        // Ohne Cursor beginnt die Tastatur dort, wo auch die Maus beginnt.
        let Some(origin) = state.cursor.or(state.anchor).or_else(|| playing_anchor(&shared))
        else {
            return gtk4::glib::Propagation::Proceed;
        };
        let n_items = shared.model.n_items();
        if n_items == 0 {
            return gtk4::glib::Propagation::Proceed;
        }
        let position = match intent {
            KeyIntent::ExtendInPlace => origin.position,
            KeyIntent::Extend(step) => {
                let stepped = i64::from(origin.position) + i64::from(step);
                stepped.clamp(0, i64::from(n_items - 1)) as u32
            }
        };
        let Some(target) = anchored_at(&shared, position) else {
            return gtk4::glib::Propagation::Proceed;
        };
        let (op, next) = resolve(state, playing_anchor(&shared), target, mode);
        apply(&shared, op);
        store_anchor_state(&shared, next);
        if matches!(intent, KeyIntent::Extend(_)) {
            // Bei einer Tastatureingabe *soll* die Zeile in den Blick kommen —
            // anders als beim Trackwechsel, den NAV-10b stillhält.
            column_view.scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
        }
        gtk4::glib::Propagation::Stop
    });
    column_view.add_controller(keys);
}
```

Pfeiltasten ohne Modifier bleiben bei GTK. Damit Anker und Fokusrahmen nicht
auseinanderlaufen, zieht ein Handler auf der Auswahl nach, sobald danach genau
eine Zeile selektiert ist. Ans Ende von `wire`, vor `column_view.add_controller`:

```rust
    {
        let shared_for_sync = shared.clone();
        shared.selection.connect_selection_changed(move |selection, _, _| {
            let mut only = None;
            for position in 0..selection.n_items() {
                if selection.is_selected(position) {
                    if only.is_some() {
                        return;
                    }
                    only = Some(position);
                }
            }
            let Some(position) = only else { return };
            let Some(anchored) = anchored_at(&shared_for_sync, position) else {
                return;
            };
            let state = shared_for_sync.selection_anchor.get();
            if state.cursor == Some(anchored) {
                return;
            }
            store_anchor_state(
                &shared_for_sync,
                super::track_list_selection_anchor::AnchorState {
                    anchor: Some(anchored),
                    cursor: Some(anchored),
                },
            );
        });
    }
```

Dann in `track_list_context_menu.rs`, neben den beiden vorhandenen Zeilen (bei
Zeile 428):

```rust
    super::track_list_selection_input::wire(column_view, shared);
```

- [ ] **Step 4: Tests und Gates laufen lassen**

Run: `cargo test -p reprise-gnome track_list_selection_input 2>&1 | grep "^test result:"`
Expected: `test result: ok.` mit 4 Tests.

Run: `xvfb-run -a cargo test -p reprise-gnome track_list_selection_input -- --ignored 2>&1 | grep "^test result:"`
Expected: `test result: ok.` — ohne Xvfb als ungelaufen melden, nicht als grün.

Run: `scripts/check-input-parity.sh 2>&1 | tail -5`
Expected: grün, der in Task 3 genannte Partner existiert jetzt.

Run: `scripts/check-ux-traceability.sh 2>&1 | tail -5`
Expected: grün, `nav_17` ist in Testnamen belegt.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list_selection_input.rs \
        crates/reprise-gnome/src/ui/track_list/track_list_context_menu.rs
git commit -m "feat: shift arrow and shift space share the selection anchor"
```


---

### Task 5: Am echten Fenster nachweisen, dass der Klick ankommt

Alle bisherigen Tests rufen `resolve`/`apply` direkt auf. Sie belegen die Logik,
aber nicht die eine Annahme, die den ganzen Zeigerpfad trägt: dass ein
Shift-Klick den Zellen-Gesture überhaupt erreicht, bevor GTKs
Selektionsmaschinerie am `GtkListItemWidget` ihn wegnimmt.

Genau für diese Fehlerklasse existiert `scripts/ptr-e2e/`. Sein README nennt sie
namentlich: Ein `GestureClick` auf einer nicht-interaktiven `Box` in einer
`ColumnView`-Zelle verlor den Klick an die Auswahlmaschinerie der Zeile, „so
star-rating clicks silently did nothing on a real desktop while every
signal-seam test stayed green". Das Harness treibt das echte Fenster mit
`xdotool` in einem Wegwerf-Xvfb und prüft das Ergebnis über den stderr-Log der
App. Ein Rust-Display-Test kann das nicht leisten — er ruft Handler auf, statt
Eingaben zu senden.

**Files:**
- Create: `scripts/ptr-e2e/selection-anchor.sh`
- Modify: `scripts/ptr-e2e/run.sh` (Shift-Klick-Helfer; Aufruf des neuen Flows
  neben `run_rating_flow`)
- Modify: `scripts/ptr-e2e/geometry.sh` (Koordinaten weiterer Zeilen, falls
  nötig)

**Interfaces:**
- Consumes: das `tracing::info!("selection anchor range applied")` aus Task 3,
  dazu die Helfer, die `run.sh` den Flow-Skripten bereitstellt: `click_at`,
  `screenshot`, `log_marker`, `assert_log_contains_since`, `log_step`.
- Produces: nichts, was Code konsumiert.

- [ ] **Step 1: Den Shift-Klick-Helfer ergänzen**

`run.sh` hat heute `click_at` (Zeile 310 herum), aber keinen Modifier-Klick.
Direkt darunter, im selben Stil:

```bash
shift_click_at() {
  xdotool keydown --window "$WINDOW_ID" shift
  click_at "$1" "$2"
  xdotool keyup --window "$WINDOW_ID" shift
}
```

Lies `click_at` vorher und übernimm dessen Aufrufkonvention wörtlich — ob es
`--window "$WINDOW_ID"` benutzt oder auf den Zeiger im Xvfb setzt, entscheidet,
wie `keydown` aussehen muss. Rate nicht; passe den Helfer an das an, was
`click_at` tatsächlich tut.

- [ ] **Step 2: Das Szenario schreiben**

`scripts/ptr-e2e/selection-anchor.sh`, nach dem Muster von `rating.sh`:

```bash
#!/usr/bin/env bash

# NAV-17: eine Shift-Auswahl geht vom laufenden Song aus, nicht ab Zeile 0.
# Helpers and geometry variables are supplied by run.sh before this function
# is called.
run_selection_anchor_flow() {
  log_step "flow: shift click anchors on the playing row…"
  screenshot "01-selection-anchor-initial"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/01-selection-anchor-initial.png"

  # Zeile 1 per Doppelklick abspielen. Danach existiert ein laufender Song,
  # und weil ein Doppelklick den Anker mitnimmt, wird er hier absichtlich
  # wieder weggeräumt: Ein Klick auf eine andere Ansicht und zurück verwirft
  # den Anker (die Positionen überleben den Modellumbau nicht), der laufende
  # Song bleibt.
  xdotool mousemove "$ROW1_TITLE_CELL_X" "$ROW1_TITLE_CELL_Y" click --repeat 2 1
  sleep 1
  screenshot "02-playing-row-1"

  MARKER=$(log_marker)
  shift_click_at "$ROW3_TITLE_CELL_X" "$ROW3_TITLE_CELL_Y"
  sleep 0.5
  screenshot "03-after-shift-click"
  assert_log_contains_since "$MARKER" "selection anchor range applied" \
    "shift click reached the cell gesture instead of GTK's row machinery"
}
```

Der Log-Nachweis ist die Kernaussage: Erscheint die Zeile nicht, hat GTK den
Klick genommen und die Capture-Annahme ist falsch. Ergänze — wenn `run.sh`
einen Helfer dafür bietet — zusätzlich eine Prüfung auf `start=1`, damit auch
belegt ist, dass die Spanne beim laufenden Song beginnt und nicht bei Zeile 0.

`geometry.sh` kennt heute nur `ROW0_*` und `ROW1_*` mit 51 px Abstand
(`ROW0_TITLE_CELL_Y=170`, `ROW1_TITLE_CELL_Y=221`). Ergänze `ROW3_*` in
derselben Rechnung. **Achtung:** Diese Koordinaten sind nicht stabil — ein
eingeblendetes Onboarding-Banner schiebt Zeile 0 nach unten. Wenn der Flow
danebengreift, prüfe zuerst den Screenshot aus Schritt 01, bevor du an der
Logik zweifelst.

- [ ] **Step 3: Den Flow in `run.sh` registrieren**

Neben den anderen Flow-Aufrufen (bei `run_rating_flow`, Zeile 581 herum) und in
der Liste, die die Flow-Skripte einliest. Sieh dir an, wie `rating.sh` dort
eingebunden ist, und mach es identisch.

- [ ] **Step 4: Laufen lassen**

Run: `cargo build 2>&1 | tail -3`
Expected: erfolgreich — das Harness braucht das Debug-Binary.

Run: `scripts/ptr-e2e/run.sh 2>&1 | tail -30`
Expected: Der neue Flow meldet seine Zusicherung als bestanden.

Scheitert die Zusicherung, ist die Capture-Phasen-Annahme im Feld falsch. Dann
**nicht** die Erwartung abschwächen, sondern auf den zweiten Weg wechseln: ein
einziger Capture-`GestureClick` auf der `ColumnView` selbst, der die Zeile über
`column_view.pick(x, y, gtk4::PickFlags::DEFAULT)` und den daran hängenden
`ListItem` bestimmt. `resolve`, `apply` und beide Tasten-Seams bleiben
unverändert; nur `wire_cell_selection` weicht einem `wire_view_selection`, und
die vier Zellen-Aufrufstellen entfallen wieder. Halte den Befund in
`docs/plans/track-list-selection-anchor.HANDOFF.md` fest, bevor du umbaust.

- [ ] **Step 5: Vollständiger Gate-Lauf über die berührten Dateien**

Run: `cargo clippy -p reprise-gnome --all-targets 2>&1 | grep -E "^(error|warning)" | head`
Expected: keine neuen Meldungen.

Run: `scripts/check-input-parity.sh && scripts/check-ux-traceability.sh`
Expected: beide grün.

Run: `for f in track_list_selection_anchor.rs track_list_selection_input.rs track_list.rs track_list_context_menu.rs; do printf "%-38s %s\n" "$f" "$(wc -l < crates/reprise-gnome/src/ui/track_list/$f)"; done`
Expected: alle unter 800. `track_list_context_menu.rs` startete bei 753 und darf
nur die eine `wire`-Zeile dazubekommen haben.

- [ ] **Step 6: Commit**

```bash
git add scripts/ptr-e2e/selection-anchor.sh scripts/ptr-e2e/run.sh scripts/ptr-e2e/geometry.sh
git commit -m "test: drive the shift click anchor through real pointer input"
```

---

## Abnahme

Von Hand in der laufenden App, weil genau das die Meldung war: Bibliothek
öffnen, einen Song mitten in einem Interpreten starten, die Ansicht wechseln und
zurückkommen, dann mit Shift+Klick auf die letzte Zeile des Interpreten
markieren. Die Auswahl muss beim laufenden Song beginnen, nicht am
Listenanfang. Danach dasselbe ohne laufenden Song: Shift+Klick markiert genau
eine Zeile.
