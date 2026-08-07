# Search Responsiveness — Umsetzungsplan

> **Für agentische Bearbeiter:** ERFORDERLICHER SUB-SKILL: `superpowers:subagent-driven-development` (empfohlen) oder `superpowers:executing-plans`, Aufgabe für Aufgabe. Schritte nutzen Checkbox-Syntax (`- [ ]`).

**Ziel:** Zwischen Tastendruck und Suchergebnis liegt eine statt zwei Wartezeiten, das Leeren der Suche wartet gar nicht, und eine Filteränderung hört auf, den laufenden Titel anzuspringen und danach nachzuzappeln.

**Architektur:** Drei voneinander unabhängige Eingriffe an bestehenden Nahtstellen. (1) `SearchEntry::set_search_delay(0)` schaltet GTKs eigene Drosselung ab, damit der Timer in `view_session.rs` der einzige Taktgeber ist; leerer Text umgeht ihn. (2) `filter_change_viewport` bekommt statt zwei Ausgängen drei, mit zwei neuen `ReloadViewport`-Varianten; ein Anker in `Shared` merkt sich die Position vor der Suche. (3) Das Ergebnis wird als `SEARCH-9` im Regelwerk festgeschrieben, wobei `FIL-9` auf Facetten eingeschränkt wird.

**Tech-Stack:** Rust, `gtk4` 0.11.4 (Feature `v4_22`), `libadwaita`, `glib`-Timeouts; Tests via `cargo test --workspace`, Display-Tests via `dbus-run-session -- xvfb-run`.

**Spec:** `docs/superpowers/specs/2026-08-07-search-responsiveness-design.md`

## Globale Randbedingungen

- **Regeltests sind display-frei.** Das Merge-Gate läuft ohne Xvfb. Ein Test, dessen Name mit `search_9_` beginnt, darf kein `gtk4::init()` brauchen. Sichtbare Belege kommen als zusätzliche, mit `#[ignore = "requires a display; run via xvfb-run"]` markierte Tests und tragen **keinen** Regelpräfix im Namen.
- **`cargo test` ohne `--workspace` läuft nur das Default-Member.** Immer `cargo test --workspace` verwenden (AGENTS.md:136).
- **Regelstatus im selben Commit.** `SEARCH-9` entsteht direkt als `[active] [gtk]`, weil der regelbenannte Test gleichzeitig entsteht.
- **UI-Strings sind englisch.** Dieser Plan ändert keine sichtbaren Strings; falls doch einer nötig wird, gehört er nach `ui/strings.rs`.
- **Keine Mutation geteilten Zustands ohne Grund.** `Shared`-Felder sind `Cell`/`RefCell`; neue Felder folgen dem Muster der Nachbarn (`Copy`-Payload → `Cell`).
- **Der Facettenpfad bleibt unverändert.** `reload_centering_playing_track` und alles, was daran hängt, wird in keiner Aufgabe angefasst.

---

### Aufgabe 1: Eine Wartezeit statt zwei

**Dateien:**
- Ändern: `crates/reprise-gnome/src/ui/window/window.rs:64-71` (SearchEntry-Builder)
- Ändern: `crates/reprise-gnome/src/ui/view_session.rs:22` (Konstante)
- Ändern: `crates/reprise-gnome/src/ui/window/library_chrome.rs:161-164` (Kommentar)
- Test: `crates/reprise-gnome/src/ui/view_session.rs` (Modul `tests` am Dateiende)

**Interfaces:**
- Konsumiert: nichts aus früheren Aufgaben.
- Produziert: `view_session::SEARCH_DEBOUNCE_MS` bleibt der Name der Konstante; Aufgabe 2 fasst sie nicht an.

- [ ] **Schritt 1: Den fehlschlagenden Test schreiben**

Ans Ende von `crates/reprise-gnome/src/ui/view_session.rs` anfügen (falls dort schon ein `mod tests` existiert, den Test dort einsortieren statt ein zweites Modul anzulegen):

```rust
#[cfg(test)]
mod tests {
    use super::SEARCH_DEBOUNCE_MS;

    /// SEARCH-9: exactly one wait sits between a keystroke and the result, and
    /// it is this one. 150 ms is the agreed value; the constant existing at
    /// 200 means GTK's own 150 ms delay is stacked underneath it.
    #[test]
    fn search_9_debounce_is_the_only_wait() {
        assert_eq!(SEARCH_DEBOUNCE_MS, 150);
    }
}
```

- [ ] **Schritt 2: Test laufen lassen, Fehlschlag bestätigen**

```bash
cargo test --workspace search_9_debounce_is_the_only_wait
```

Erwartet: FAIL, `assert_eq!` meldet `left: 200, right: 150`.

> Wenn statt eines Fehlschlags „0 passed; 0 filtered out" erscheint, hat der Filter nichts getroffen — dann ist der Testname falsch geschrieben. Die Zahl vor `passed` immer mitlesen; ein Lauf ohne Treffer meldet trotzdem `ok`.

- [ ] **Schritt 3: Die Konstante senken**

In `crates/reprise-gnome/src/ui/view_session.rs:22`:

```rust
/// SEARCH-9: the one and only wait between typing and the result. GTK's own
/// `search-delay` is switched off in `window.rs`, so this is not stacked on
/// top of anything — raising it here raises the felt latency one-to-one.
const SEARCH_DEBOUNCE_MS: u64 = 150;
```

- [ ] **Schritt 4: GTKs eigene Drosselung abschalten**

In `crates/reprise-gnome/src/ui/window/window.rs`, direkt nach dem `search_entry`-Builder (Zeile 64-68) und vor dem `update_property`-Aufruf:

```rust
    // SEARCH-9: `GtkSearchEntry` throttles `search-changed` by its own
    // `search-delay` (150 ms by default). Reprise debounces the query itself
    // in `view_session::wire_search`, so leaving GTK's delay on stacked two
    // waits and put half the latency out of reach of the code that owns it.
    search_entry.set_search_delay(0);
```

- [ ] **Schritt 5: Den Sofortpfad fürs Leeren einbauen**

In `crates/reprise-gnome/src/ui/view_session.rs`, in `wire_search`. Der bestehende Block ab `let text = entry.text().to_string();` wird ersetzt durch:

```rust
        let text = entry.text().to_string();
        // Browser state follows the visible entry synchronously so leaving a
        // place during the debounce window still captures the exact query.
        *track_list.shared.filter.borrow_mut() = text.clone();
        // SEARCH-9: clearing is not typing. Esc, the chip's ×, "Show all N
        // tracks" and a hand-emptied field all arrive here as empty text, and
        // none of them is the middle of a sequence worth waiting out.
        if text.is_empty() {
            track_list.reload();
            return;
        }
        let track_list = track_list.clone();
        let pending_for_timeout = pending.clone();
        let source_id =
            glib::timeout_add_local(Duration::from_millis(SEARCH_DEBOUNCE_MS), move || {
                track_list.reload();
                pending_for_timeout.borrow_mut().take();
                glib::ControlFlow::Break
            });
        *pending.borrow_mut() = Some(source_id);
```

Beachte: `text` wird jetzt geklont, weil es nach dem Schreiben in `shared.filter` noch für die Leerprüfung gebraucht wird. Der bereits vorhandene Abbruch eines wartenden Timers steht weiter oben in derselben Closure und bleibt unverändert — er greift auch für den Leerfall, weshalb ein alter Timer den soeben zurückgesetzten Filter nicht nachträglich überschreiben kann.

- [ ] **Schritt 6: Den Kommentar in `library_chrome.rs` berichtigen**

`crates/reprise-gnome/src/ui/window/library_chrome.rs:161-164` — der Kommentar begründet die Wahl von `connect_changed` mit GTKs 150 ms, die es nach Schritt 4 nicht mehr gibt. Ersetzen durch:

```rust
    // `connect_changed`, not `connect_search_changed`: the lens only reflects
    // "a query exists" (SEARCH-3) and must follow every keystroke. Since
    // SEARCH-9 the entry's own `search-delay` is 0 and the two signals fire
    // together, but the app's debounce still sits behind `search_changed` in
    // `view_session`, and the lens must not wait for it.
```

- [ ] **Schritt 7: Tests laufen lassen**

```bash
cargo test --workspace search_9_debounce_is_the_only_wait
cargo test --workspace -p reprise-gnome
```

Erwartet: der neue Test PASS; keine Regression in `reprise-gnome`. Display-Tests sind `#[ignore]` und laufen hier nicht mit.

- [ ] **Schritt 8: Committen**

```bash
git add crates/reprise-gnome/src/ui/view_session.rs \
        crates/reprise-gnome/src/ui/window/window.rs \
        crates/reprise-gnome/src/ui/window/library_chrome.rs
git commit -m "fix: one debounce between typing and search results, not two

GtkSearchEntry throttles search-changed by 150 ms of its own on top of the
app's 200 ms timer, so roughly 350 ms passed before a query ran and half of
it was invisible in the code. Switch GTK's delay off, drop ours to 150, and
let an emptied field skip the wait entirely — clearing is not typing."
```

---

### Aufgabe 2: Der Anker vor der Suche

**Dateien:**
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list.rs:98ff` (Feld in `Shared`)
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list_builder.rs:88-96` (Initialisierung)
- Test: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs` (Modul `tests`)

**Interfaces:**
- Konsumiert: nichts aus Aufgabe 1.
- Produziert: `Shared::pre_search_anchor: Cell<Option<(i64, f64)>>` — Aufgabe 3 liest und schreibt dieses Feld.

- [ ] **Schritt 1: Das Feld anlegen**

In `crates/reprise-gnome/src/ui/track_list/track_list.rs`, in `struct Shared`, direkt nach `track_reveal_pending` (Zeile 142):

```rust
    /// SEARCH-9: where the list stood before a search narrowed it, so clearing
    /// the search can put the user back instead of dropping them at the top or
    /// on the playing track. Captured once on the empty → non-empty transition
    /// and consumed when the query goes empty again. `(track id, offset)`
    /// rather than a raw scroll value, for the same reason BROWSE-2 uses that
    /// form: after a re-sort a pixel value points at a different row.
    pub(in crate::ui) pre_search_anchor: Cell<Option<(i64, f64)>>,
```

- [ ] **Schritt 2: Initialisieren**

In `crates/reprise-gnome/src/ui/track_list/track_list_builder.rs`, im `Shared { … }`-Literal direkt nach `track_reveal_pending: Cell::new(false),` (Zeile 96):

```rust
        pre_search_anchor: Cell::new(None),
```

- [ ] **Schritt 3: Bauen und bestätigen, dass nichts fehlt**

```bash
cargo build -p reprise-gnome
```

Erwartet: erfolgreicher Build. Fehlt die Initialisierung an einer zweiten Konstruktionsstelle, meldet der Compiler `missing field pre_search_anchor` mit exakter Zeile — dann dort dieselbe Zeile ergänzen.

- [ ] **Schritt 4: Committen**

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list.rs \
        crates/reprise-gnome/src/ui/track_list/track_list_builder.rs
git commit -m "feat: remember where the list stood before a search"
```

---

### Aufgabe 3: Die Filteränderung hört auf zu springen

**Dateien:**
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:64-76` (`ReloadViewport`, `filter_change_viewport`)
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:160-198` (`restore_reload_anchor`)
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:345-354` (`set_filter_and_reload`)
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:424-432` (`reload_with_anchor_and_viewport`, Hold-Bedingung)
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:760-777` (Testmodul)

**Interfaces:**
- Konsumiert: `Shared::pre_search_anchor` aus Aufgabe 2.
- Produziert: `ReloadViewport::{Top, RestorePreSearch}` neben den bestehenden `PreserveAnchor` und `CenterPlayingTrack`.

- [ ] **Schritt 1: Den fehlschlagenden Test schreiben**

Im Testmodul am Ende von `track_list_reload.rs` den bestehenden Test `fil_9_any_search_change_requests_playing_track_centering` **ersetzen** durch:

```rust
    /// SEARCH-9: three outcomes, decided solely by whether the *new* query is
    /// empty. Adding a character, deleting one and replacing the text are the
    /// same case — the result set is new either way, so the eye belongs at its
    /// top. Only emptying the query goes back to where the user came from.
    #[test]
    fn search_9_filter_change_decides_viewport_by_the_new_query() {
        assert!(matches!(
            filter_change_viewport("", "Match"),
            ReloadViewport::Top
        ));
        assert!(matches!(
            filter_change_viewport("Mat", "Match"),
            ReloadViewport::Top
        ));
        assert!(matches!(
            filter_change_viewport("Match", "Mat"),
            ReloadViewport::Top
        ));
        assert!(matches!(
            filter_change_viewport("Match", ""),
            ReloadViewport::RestorePreSearch
        ));
        assert!(matches!(
            filter_change_viewport("Match", "Match"),
            ReloadViewport::PreserveAnchor
        ));
    }
```

- [ ] **Schritt 2: Test laufen lassen, Fehlschlag bestätigen**

```bash
cargo test --workspace search_9_filter_change_decides_viewport_by_the_new_query
```

Erwartet: FAIL beim Kompilieren — `no variant named Top found for enum ReloadViewport`.

- [ ] **Schritt 3: Die Varianten und die Entscheidung**

In `track_list_reload.rs`, `ReloadViewport` (Zeile 64-68) erweitern:

```rust
#[derive(Clone, Copy)]
pub(super) enum ReloadViewport {
    PreserveAnchor,
    CenterPlayingTrack,
    /// SEARCH-9: a new result set is read from its top.
    Top,
    /// SEARCH-9: an emptied query returns to `Shared::pre_search_anchor`.
    RestorePreSearch,
}
```

und `filter_change_viewport` (Zeile 70-76) ersetzen:

```rust
fn filter_change_viewport(previous: &str, current: &str) -> ReloadViewport {
    if previous == current {
        ReloadViewport::PreserveAnchor
    } else if current.is_empty() {
        ReloadViewport::RestorePreSearch
    } else {
        ReloadViewport::Top
    }
}
```

- [ ] **Schritt 4: Test laufen lassen, Erfolg bestätigen**

```bash
cargo test --workspace search_9_filter_change_decides_viewport_by_the_new_query
```

Erwartet: PASS. Der Rest der Datei kompiliert noch nicht — die `match`-Ausdrücke über `ReloadViewport` sind jetzt unvollständig; das behebt Schritt 5.

- [ ] **Schritt 5: Die neuen Varianten im Wiederherstellungspfad behandeln**

In `restore_reload_anchor` (ab Zeile 165) den Rumpf ersetzen:

```rust
fn restore_reload_anchor(
    shared: &Shared,
    captured: &ReloadAnchor,
    viewport: ReloadViewport,
    hold: Option<AdjustmentHold>,
    resolved_ids: Option<Vec<i64>>,
) {
    // SEARCH-9: a new result set is read from its top. Doing this before the
    // early return below is what makes the typed-search path cheap — it needs
    // no id list at all, so the sorted full-table query disappears whenever
    // nothing is selected.
    if matches!(viewport, ReloadViewport::Top) {
        if let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view) {
            adjustment.set_value(0.0);
        }
    }
    // Resolving positions costs a sorted full-table id query; skip it when
    // the capture side already established there is nothing to put back and
    // the caller did not request a playing-track reveal.
    let reveal_playing_track = matches!(viewport, ReloadViewport::CenterPlayingTrack)
        && shared.playing_track_id.get().is_some();
    let restores_pre_search = matches!(viewport, ReloadViewport::RestorePreSearch)
        && shared.pre_search_anchor.get().is_some();
    if reload_restore::is_noop(captured) && !reveal_playing_track && !restores_pre_search {
        return;
    }
    let current_ids = resolved_ids.unwrap_or_else(|| shared.current_view_ids());
    select_captured_ids(shared, captured, &current_ids);

    if matches!(viewport, ReloadViewport::CenterPlayingTrack) {
        let playing_track_id = shared.playing_track_id.get();
        if playing_track_id.is_some_and(|track_id| current_ids.contains(&track_id)) {
            schedule_centered_scroll_restore(
                shared.column_view.clone(),
                playing_track_id,
                current_ids,
                SCROLL_RESTORE_MAX_ATTEMPTS,
            );
            return;
        }
    }

    // SEARCH-9: the search is over — put the user back where it started. A
    // consumed anchor is taken, not copied: the next search captures its own.
    if matches!(viewport, ReloadViewport::RestorePreSearch) {
        let anchor = shared.pre_search_anchor.take();
        schedule_scroll_restore(
            shared.column_view.clone(),
            anchor,
            current_ids,
            SCROLL_RESTORE_MAX_ATTEMPTS,
            hold,
        );
        return;
    }

    // `Top` already placed the viewport above; the captured anchor belongs to
    // the pre-filter list and must not pull it back.
    if matches!(viewport, ReloadViewport::Top) {
        return;
    }

    schedule_scroll_restore(
        shared.column_view.clone(),
        captured.anchor,
        current_ids,
        SCROLL_RESTORE_MAX_ATTEMPTS,
        hold,
    );
}
```

- [ ] **Schritt 6: Den Anker beim Suchbeginn fangen**

In `set_filter_and_reload` (Zeile 350-354) den Rumpf ersetzen:

```rust
pub(in crate::ui) fn set_filter_and_reload(shared: &Rc<Shared>, text: &str) {
    let previous = shared.filter.borrow().clone();
    let viewport = filter_change_viewport(previous.as_str(), text);
    // SEARCH-9: the empty → non-empty transition is the moment the user leaves
    // their place. Capture it once; a refinement of an existing query must not
    // overwrite it with a position inside the result set.
    if previous.is_empty() && !text.is_empty() {
        let captured = capture_reload_anchor(shared);
        shared.pre_search_anchor.set(captured.anchor);
    }
    *shared.filter.borrow_mut() = text.to_string();
    reload_with_viewport(shared, viewport);
}
```

- [ ] **Schritt 7: Den Anker beim Quellenwechsel verwerfen**

In `set_source_and_reload`, direkt nach `*shared.filter.borrow_mut() = String::new();`:

```rust
    // SEARCH-9: an anchor from the previous source points at a row this view
    // does not contain.
    shared.pre_search_anchor.set(None);
```

- [ ] **Schritt 8: Den Hold auf die Varianten beschränken, die scrollen**

In `reload_with_anchor_and_viewport` (Zeile 428) die Hold-Bedingung ersetzen:

```rust
    // SEARCH-9: `Top` writes the adjustment itself and wants no guard fighting
    // it; only the two variants that restore a captured position need one.
    let hold = matches!(
        viewport,
        ReloadViewport::PreserveAnchor | ReloadViewport::RestorePreSearch
    )
    .then(|| gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view))
    .flatten()
    .filter(|_| captured.anchor.is_some() || shared.pre_search_anchor.get().is_some())
    .map(|adjustment| AdjustmentHold::new(&adjustment));
```

- [ ] **Schritt 9: Alles laufen lassen**

```bash
cargo test --workspace -p reprise-gnome
```

Erwartet: PASS. Sollte `fil_9_filter_change_centers_playing_track_in_new_results` (`reload_restore.rs:232`) fehlschlagen, ist das ein Fehler in dieser Aufgabe — der Test prüft nur die Zentriermathematik, die für Facetten unverändert gilt, und darf nicht berührt werden.

- [ ] **Schritt 10: Committen**

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list_reload.rs
git commit -m "fix: filtering no longer jumps to the playing track

Every filter change asked for the playing track to be centred (FIL-9), which
on each debounce cost a sorted full-table id query, a jump away from what the
user was reading, and eight 16 ms refinement rounds writing the scroll
position. A typed query now reads from the top and an emptied one returns to
where the search started. Facet filters keep FIL-9."
```

---

### Aufgabe 4: SEARCH-9 im Regelwerk

**Dateien:**
- Ändern: `docs/ux-rules.md:1440-1445` (FIL-9 einschränken)
- Ändern: `docs/ux-rules.md` (SEARCH-9 nach SEARCH-8, Zeile ~2510 einfügen)
- Ändern: `crates/reprise-gnome/src/ui/track_list/current_track_selection_tests.rs:118-119` (Display-Test auf Facetten umstellen)

**Interfaces:**
- Konsumiert: das Verhalten aus Aufgaben 1 und 3.
- Produziert: nichts, was spätere Aufgaben lesen.

- [ ] **Schritt 1: FIL-9 auf Facetten einschränken**

`docs/ux-rules.md:1440-1445` ersetzen:

```markdown
- **FIL-9** [active] [gtk] — When a **facet** filter is set, changed or
  removed and the loaded track belongs to the new result set, its marked
  row is vertically centered instead of anchored to the top table edge.
  Selection and keyboard focus remain unchanged. Without a loaded track
  visible in the target, the existing ID-plus-offset anchor is retained.
  The header-bar search is no longer covered: SEARCH-9 governs it, because
  a query changes with every keystroke and paid for the centering far more
  often than a facet click does.
```

- [ ] **Schritt 2: SEARCH-9 anlegen**

In `docs/ux-rules.md` direkt hinter dem SEARCH-8-Absatz einfügen:

```markdown
- **SEARCH-9** [active] [gtk] — **Searching answers at once, and clearing
  answers immediately.** Exactly one wait sits between a keystroke and the
  result — the application's own debounce of 150 ms; the entry's built-in
  `search-delay` is switched off so the two never stack. Emptying the query
  waits not at all: Esc, the chip's ×, "Show all N tracks" and a
  hand-cleared field reload straight away. A query that is set or refined
  places the viewport at the top of its results and moves it no further
  after the model swap — it centers nothing (superseding FIL-9 for search).
  Emptying the query returns the viewport to where the list stood when the
  search began, as an ID-plus-offset anchor; if that row is gone, to the top.
```

- [ ] **Schritt 3: Den Display-Test auf Facetten umstellen**

`crates/reprise-gnome/src/ui/track_list/current_track_selection_tests.rs:118-119` — der Test filtert heute über Suchtext und belegt damit ein Verhalten, das es nicht mehr gibt. Er behält Namen und Regelbindung, wechselt aber auf einen Facettenfilter. Die Zeile `track_list.set_filter("Match");` (etwa Zeile 167) ersetzen durch:

```rust
    *track_list.shared.browse_filter.borrow_mut() = BrowseFilter {
        genre: Some("Synthetic".to_string()),
        ..BrowseFilter::default()
    };
    crate::ui::track_list::reload::reload_centering_playing_track(&track_list.shared);
```

Die darauffolgende Zusicherung `assert_eq!(track_list.shared.model.n_items(), 30);` bleibt gültig, sobald die Fixture genau 30 Zeilen dem Genre zuordnet — siehe nächster Absatz, der die IDs 31 bis 60 auf `"Synthetic"` legt.

Dazu die Testfixture anpassen: die 100 eingefügten Zeilen bekommen ein Genre, damit die Facette greift — im `INSERT` die Spalte ergänzen:

```rust
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, genre, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', ?4, 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                title,
                if (31..=60).contains(&id) { "Synthetic" } else { "Other" },
            ),
        )
        .unwrap();
```

Der Import `use reprise_core::queries::BrowseFilter;` muss oben in der Datei stehen; falls nicht vorhanden, ergänzen.

- [ ] **Schritt 4: Den Display-Test laufen lassen**

```bash
dbus-run-session -- xvfb-run -a cargo test --workspace \
  fil_9_filter_changes_center_the_visible_playing_track -- --ignored --test-threads=1
```

Erwartet: PASS, und die Zeile vor `passed` zeigt `1 passed`. Zeigt sie `0 passed`, hat der Filter nichts getroffen und der Test ist **nicht** gelaufen.

- [ ] **Schritt 5: Das Regelwerks-Gate laufen lassen**

```bash
cargo test --workspace ux_rules
```

Erwartet: PASS. Das Traceability-Gate prüft, dass jede `[active]`-Regel einen regelbenannten Test hat; `search_9_debounce_is_the_only_wait` und `search_9_filter_change_decides_viewport_by_the_new_query` decken SEARCH-9 ab.

- [ ] **Schritt 6: Committen**

```bash
git add docs/ux-rules.md \
        crates/reprise-gnome/src/ui/track_list/current_track_selection_tests.rs
git commit -m "docs: SEARCH-9 governs the search, FIL-9 keeps the facets"
```

---

### Aufgabe 5: Sichtbarer Beleg und Abnahme

**Dateien:**
- Erstellen: `crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs`
- Ändern: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs` (Modul einhängen, am Dateiende)

**Interfaces:**
- Konsumiert: alles aus den Aufgaben 1-3.
- Produziert: nichts.

- [ ] **Schritt 1: Den Display-Test schreiben**

Neue Datei `crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs`. Kein Regelpräfix im Namen — das ist ein sichtbarer Beleg, kein Regeltest, und Regeltests müssen display-frei bleiben:

```rust
//! Visible proof for SEARCH-9's viewport half: a typed query reads from the
//! top, and clearing it returns to where the search began. The rule-named
//! tests live in `track_list_reload.rs` and are display-free by design; these
//! need a real `ColumnView` with a real allocation and are `#[ignore]`d.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::track_list::TrackList;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn typed_search_reads_from_the_top_and_clearing_comes_back() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=200 {
        let title = if (150..=170).contains(&id) {
            format!("Match Track {id:03}")
        } else {
            format!("Other Track {id:03}")
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (id, format!("/synthetic/{id:03}.flac"), title),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    adjustment.set_value(1200.0);
    while gtk4::glib::MainContext::default().iteration(false) {}
    let departed_from = adjustment.value();
    assert!(
        departed_from > 0.0,
        "the test must start away from the top, else it proves nothing"
    );

    track_list.set_filter("Match");
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(
        adjustment.value(),
        0.0,
        "a typed query reads from the top of its results"
    );

    track_list.set_filter("");
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(
        (adjustment.value() - departed_from).abs() < 40.0,
        "clearing returns within a row of where the search began: expected \
         about {departed_from}, got {}",
        adjustment.value()
    );

    window.close();
}
```

- [ ] **Schritt 2: Das Modul einhängen**

Am Ende von `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`, neben die vorhandenen Display-Test-Einhängungen:

```rust
#[cfg(test)]
#[path = "search_viewport_display_tests.rs"]
mod search_viewport_display_tests;
```

- [ ] **Schritt 3: Laufen lassen**

```bash
dbus-run-session -- xvfb-run -a cargo test --workspace \
  typed_search_reads_from_the_top_and_clearing_comes_back -- --ignored --test-threads=1
```

Erwartet: `1 passed`. Bei `0 passed` ist der Test nicht gelaufen.

- [ ] **Schritt 4: Gegenprobe gegen die Basis**

Der Beweis zählt nur, wenn er vorher fehlgeschlagen wäre. Im Basis-Worktree (`origin/dev`, ohne diese Änderungen) darf derselbe Test nicht bestehen:

```bash
git stash list   # muss leer bleiben: never stash, worktrees share it
git -C ../.. worktree list
```

Statt zu stashen: den Test in einen Worktree auf `origin/dev` kopieren und dort laufen lassen. Fällt er dort **nicht** durch, misst er nicht das, was diese Änderung bewirkt — dann ist der Test falsch, nicht die Implementierung.

- [ ] **Schritt 5: Committen**

```bash
git add crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs \
        crates/reprise-gnome/src/ui/track_list/track_list_reload.rs
git commit -m "test: visible proof that filtering reads from the top and clearing returns"
```

---

## Was dieser Plan nicht enthält

Die **Messung** aus Teil 3 der Spec ist kein Task in diesem Plan. Sie läuft
gegen einen gebauten Stand, nicht gegen den Quelltext, und ist deshalb keine
Aufgabe, die ein Bearbeiter zwischen zwei Commits abhaken kann. Sie gehört vor
und nach diesen Plan:

- **Vorher**, gegen `origin/dev`: die Zahlen, die in der Abnahmetabelle der Spec
  heute nur Herleitungen sind.
- **Nachher**, gegen denselben Build plus diese Aufgaben: dieselben vier
  Szenarien, plus die Gegenprobe mit zurückgedrehter Änderung.

Erst diese zweite Messung entscheidet, ob die 150 ms aus Aufgabe 1 stehen
bleiben. Fällt die gemessene Latenz deutlich anders aus als erwartet, ist das
ein eigener Befund und die Ursachenanalyse geht neu auf — nicht die Konstante
wird dann so lange gedreht, bis die Zahl gefällt.

Ebenfalls nicht enthalten: der **Titel-Link aus einer fremden Playlist** und der
**Settings-Dialog**. Beide sind eigene Themen mit eigenen Befunden; der
Settings-Befund (`preferences.rs:282` baut alle fünf Seiten, bevor der Dialog
erscheint) ist bereits klar, der Titel-Link wartet auf die Messung.
