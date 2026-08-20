---
slug: releases-multiselect-context-menu
worktree: ~/Projects/reprise-releases-context-menu
branch: feature/releases-multiselect-context-menu
phase: planned
codex_session:
created: 2026-08-20
spec: docs/superpowers/specs/2026-08-20-releases-multiselect-and-context-menu-design.md
---
# Releases bekommt Mehrfachauswahl, ein Zeilenmenü und ein umkehrbares Hide — Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> or `superpowers:executing-plans` and work task by task. Steps are checkboxes.

**Ziel:** Die Releases-Tabelle wählt wie die Track-Liste aus (Ctrl/Shift), trägt
ein Zeilen-Kontextmenü mit Hide/Restore und Navigation, und jedes Ausblenden
hinterlässt einen Toast mit `Undo`.

**Architektur:** Die reine Auswahl-Arithmetik der Track-Liste wird generisch über
den Zeilenschlüssel und zieht in ein geteiltes Modul; Releases dockt an die
bestehende Kontextflächen-Schicht `ui/source_context_surface` an (wie Radio) und
bekommt ein eigenes, reines Menümodell nach dem Muster von
`radio_context_menu::build`. Das Schreiben wird zu einem Batch in einer
Transaktion, damit Undo genau die geschriebene Menge zurücknimmt.

**Tech Stack:** Rust, gtk4-rs, libadwaita, rusqlite, `gio::Menu`/`SimpleActionGroup`.

## Global Constraints

- Repo-Sprache: Code, Kommentare, Doku, Commit-Messages **englisch**. Dieser Plan
  ist deutsch, die Artefakte sind es nicht.
- Dateigrenze: 800 Zeilen. `releases_columns.rs` liegt bei 798 und **schrumpft**
  in diesem Plan (Task 10); es darf durch keinen Task wachsen. Neuer Code kommt
  in neue Dateien.
- Keine Mutation geteilter Zustände über Umwege: `Shared` wird per `Rc` gereicht,
  Callbacks als `Rc<dyn Fn(..)>`, Fenster-/Overlay-Bezüge als `glib::WeakRef`.
- Jeder Test, der ein Display braucht, trägt
  `#[ignore = "requires a display; run via xvfb-run"]`.
- Display-Testlauf immer so, sonst greift der Lauf die echte Wayland-Session an:
  `env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome <filter> -- --ignored`
- Alle sichtbaren Zeichenketten laufen über `crate::ui::strings` mit `N_!`,
  gezählte Formen über `strings::plural`. Keine formatierten Literale im Menü.
- `Files:`-Listen unten sind Startpunkt, kein Zaun. Wenn der Vertrag einer Task
  nur erfüllbar ist, indem eine ungenannte Datei angefasst wird, dann fass sie an.
  Anhalten nur, wenn der **Vertrag selbst** falsch ist.

## Ausgangslage in Zahlen

- `releases_model.rs:57,63,78` — `gtk4::SingleSelection`.
- `releases_view.rs:381` — `set_hidden` schreibt und ruft `render_cache`.
- `releases_columns.rs:149-309` — `status_column`, darin der Inline-Button.
- `releases_columns.rs:22` — `pub(super) type OnSetHidden = Rc<dyn Fn(String, bool)>;`
- `artist_news_query.rs:400-425` — `set_release_hidden` / `set_release_hidden_in`.
- `artist_news_query.rs:471-486` — `mark_releases_seen`, das Batch-Muster.
- **Kein** `ToastOverlay` und **kein** `MetadataNavigator` erreichen die
  Releases-Ansicht heute. Beide Nähte werden in diesem Plan gelegt.
- **Kein** bestehender Test deckt den Inline-Hide-Button ab. Task 10 löscht also
  keine Tests, sondern schließt eine Lücke, die vorher niemand bemerkt hat.

## Dateiübersicht

| Datei | Rolle |
| --- | --- |
| `ui/table_selection/mod.rs` (neu) | Modulwurzel des geteilten, widgetfreien Auswahlmoduls |
| `ui/table_selection/anchor.rs` (neu) | `SelectMode`, `SelectionOp`, `Anchored<Id>`, `AnchorState<Id>`, `validate`, `resolve` — verbatim aus der Track-Liste, generisch über `Id` |
| `ui/table_selection/input.rs` (neu) | `pointer_mode`, `KeyIntent`, `key_intent` — verbatim aus der Track-Liste |
| `track_list/track_list_selection_anchor.rs` | behält nur die GTK-berührenden Teile, zieht die Typen als Aliase mit `Id = i64` |
| `track_list/track_list_selection_input.rs` | behält `apply`, `wire`, `wire_cell_selection` |
| `reprise-core/src/artist_news_query.rs` | `apply_release_hidden_in` (neu, ohne Transaktion) + `set_releases_hidden` (neu, Batch) |
| `releases/releases_model.rs` | `MultiSelection`, Auswahl über MBIDs erhalten |
| `releases/releases_selection.rs` (neu) | Ctrl/Shift-Verdrahtung der Releases-Tabelle auf `table_selection` |
| `releases/releases_menu.rs` (neu) | reines `summarize(..)` + `build(..) -> gio::Menu` |
| `releases/releases_context_menu.rs` (neu) | Action-Group, Gesture, Tastatur, Popover |
| `releases/releases_hide.rs` (neu) | Hide/Restore inkl. Undo-Toast und Auswahl danach |
| `releases/releases_view.rs` | Nähte: `set_toast_overlay`, `set_on_navigate`, Verdrahtung |
| `releases/releases_columns.rs` | Zellen durch `source_context_surface::wrap`, Inline-Button raus |
| `ui/strings_releases.rs` | neue Labels inkl. gezählter Formen |
| `ui/window/window_action_wiring.rs` | Navigations-Callback ans `MetadataNavigator` |
| `docs/ux-rules.md` | neue Regel + Korrektur von NR-39 |

---

### Task 1: Die reine Auswahl-Logik wird geteilt und generisch

Heute liegen vier reine Funktionen im Track-List-Modul und sind an `i64`
gebunden. Releases führt Zeilen über `release_group_mbid: String`. Der Umzug
macht die Typen generisch — sonst wäre die Alternative eine zweite Kopie der
Anker-Arithmetik, und NAV-17 hätte zwei Wohnorte.

**Files:**
- Create: `crates/reprise-gnome/src/ui/table_selection/mod.rs`
- Create: `crates/reprise-gnome/src/ui/table_selection/anchor.rs`
- Create: `crates/reprise-gnome/src/ui/table_selection/input.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_selection_anchor.rs:16-111`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_selection_input.rs:24-58`
- Modify: `crates/reprise-gnome/src/ui/mod.rs` (Modul anmelden)

**Interfaces:**
- Produces:
  ```rust
  pub(in crate::ui) enum SelectMode { Only, Toggle, Range, RangeAdditive }
  pub(in crate::ui) struct Anchored<Id> { pub position: u32, pub id: Id }
  pub(in crate::ui) struct AnchorState<Id> { pub anchor: Option<Anchored<Id>>, pub cursor: Option<Anchored<Id>> }
  pub(in crate::ui) enum SelectionOp { SelectOnly(u32), Toggle(u32), SelectRange { start: u32, len: u32, replace: bool } }
  pub(in crate::ui) fn validate<Id: PartialEq>(state: AnchorState<Id>, lookup: impl Fn(u32) -> Option<Id>) -> AnchorState<Id>;
  pub(in crate::ui) fn resolve<Id: Clone>(state: AnchorState<Id>, fallback: Option<Anchored<Id>>, target: Anchored<Id>, mode: SelectMode) -> (SelectionOp, AnchorState<Id>);
  pub(in crate::ui) fn pointer_mode(state: gtk4::gdk::ModifierType) -> Option<SelectMode>;
  pub(in crate::ui) enum KeyIntent { Extend(i32), ExtendInPlace }
  pub(in crate::ui) fn key_intent(key: gtk4::gdk::Key, state: gtk4::gdk::ModifierType) -> Option<KeyIntent>;
  ```

- [ ] **Schritt 1: Modul anlegen und die reinen Teile verbatim übernehmen**

`anchor.rs` bekommt `SelectMode`, `Anchored`, `AnchorState`, `SelectionOp`,
`validate` und `resolve` **wortwörtlich** aus `track_list_selection_anchor.rs:16-111`.
Genau drei Änderungen sind erlaubt, sonst nichts:

1. `Anchored`/`AnchorState` werden generisch über `Id`; das Feld `track_id: i64`
   heißt `id: Id`.
2. Der Parameter `playing: Option<Anchored>` von `resolve` heißt `fallback` —
   die Track-Liste setzt dort ihre spielende Zeile ein, Releases hat keine und
   übergibt `None`. Der Doc-Kommentar erklärt genau das.
3. Sichtbarkeit `pub(super)` → `pub(in crate::ui)`.

`input.rs` bekommt `pointer_mode`, `KeyIntent` und `key_intent` verbatim aus
`track_list_selection_input.rs:24-58`, ebenfalls nur mit angepasster Sichtbarkeit.

`mod.rs`:

```rust
//! Selection arithmetic shared by the app's multi-select tables.
//!
//! Widget-free on purpose: the anchor rule (NAV-17) and the pointer/key
//! modifier reading are the same question in the track list and in the
//! releases table, and a rule with two homes drifts. Everything that touches
//! a `SelectionModel` stays in the table that owns it.

mod anchor;
mod input;

pub(in crate::ui) use anchor::{resolve, validate, AnchorState, Anchored, SelectMode, SelectionOp};
pub(in crate::ui) use input::{key_intent, pointer_mode, KeyIntent};
```

- [ ] **Schritt 2: Die Tests mitnehmen**

Alle 12 nicht-ignorierten `#[test]` aus `track_list_selection_anchor.rs:165-366`
und die vier aus `track_list_selection_input.rs:267-315` ziehen mit in die neuen
Module. Sie behalten ihre Namen (`nav_17_*`), damit die Regel-ID auffindbar
bleibt. Sie instanziieren `Id = i64`, also ändert sich in den Testkörpern nur
`track_id:` → `id:`.

Die vier Tests mit `#[ignore = "requires a display; ..."]` bleiben in der
Track-Liste — sie fahren echte GTK-Modelle und gehören zum GTK-berührenden Teil.

- [ ] **Schritt 3: Testlauf — muss rot sein, solange die Track-Liste noch ihre eigene Kopie hat**

```
cargo test -p reprise-gnome table_selection
```
Erwartung: Übersetzungsfehler oder doppelte Definitionen, solange Schritt 4
fehlt. Das ist der Beleg, dass die Tests wirklich am neuen Ort laufen.

- [ ] **Schritt 4: Track-Liste auf das geteilte Modul umstellen**

In `track_list_selection_anchor.rs` bleiben `live_anchor_state`,
`store_anchor_state`, `anchored_at`, `playing_anchor`. Der Kopf bekommt:

```rust
use crate::ui::table_selection;

pub(super) type Anchored = table_selection::Anchored<i64>;
pub(super) type AnchorState = table_selection::AnchorState<i64>;
pub(super) use table_selection::{resolve, validate, SelectMode, SelectionOp};
```

Alle Aufrufstellen, die `Anchored { position, track_id }` konstruieren, heißen
jetzt `Anchored { position, id }`. `resolve(state, playing, target, mode)` bleibt
aufrufseitig gleich — der Parameter heißt nur anders.

In `track_list_selection_input.rs` fallen `pointer_mode`, `KeyIntent` und
`key_intent` weg; stattdessen `pub(super) use crate::ui::table_selection::{key_intent, pointer_mode, KeyIntent};`.

- [ ] **Schritt 5: Grün, und zwar beide Seiten**

```
cargo test -p reprise-gnome table_selection
cargo test -p reprise-gnome nav_17
```
Erwartung: alle `nav_17_*`-Tests laufen (die vier Display-Tests werden als
`ignored` gezählt, nicht als Fehler). Zusätzlich muss übersetzen:
```
cargo check -p reprise-gnome
```

- [ ] **Schritt 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/table_selection crates/reprise-gnome/src/ui/mod.rs crates/reprise-gnome/src/ui/track_list
git commit -m "refactor(selection): the anchor rule gets one home for every table"
```

---

### Task 2: Ein Batch-Schreiber, damit Undo genau zurücknimmt

Der heutige Hide-Pfad schreibt ohne Transaktion, der Restore-Pfad öffnet selbst
eine. Wer den Restore-Pfad in einer Schleife innerhalb einer äußeren Transaktion
aufruft, bekommt von SQLite `cannot start a transaction within a transaction`.
Deshalb wird der transaktionslose Kern herausgezogen und die Klammer wandert
nach oben — genau einmal, für einen wie für fünfzig Einträge.

**Files:**
- Modify: `crates/reprise-core/src/artist_news_query.rs:400-425`
- Modify: `crates/reprise-core/src/artist_news.rs:76-79` (Re-Export)
- Test: `crates/reprise-core/src/artist_news_query_tests.rs`

**Interfaces:**
- Produces: `pub fn set_releases_hidden(db: &crate::db::Db, release_group_mbids: &[String], hidden: bool) -> Result<(), rusqlite::Error>`
- Consumes: nichts aus Task 1.

- [ ] **Schritt 1: Der scheiternde Test**

Ans Ende von `artist_news_query_tests.rs`. Er hat einen Kontrollarm: ein Release,
das **nicht** in der Liste steht, muss unangetastet bleiben — sonst würde ein
`UPDATE` ohne `WHERE` grün aussehen.

```rust
#[test]
fn hiding_a_batch_hides_exactly_the_named_releases_and_undo_restores_them() {
    let conn = migrated_conn();
    insert_release(&conn, "one", None);
    insert_release(&conn, "two", None);
    insert_release(&conn, "control", None);

    let batch = vec!["one".to_owned(), "two".to_owned()];
    set_releases_hidden(&conn, &batch, true).unwrap();

    assert_eq!(
        hidden_release_count(&conn).unwrap(),
        2,
        "both named releases are hidden, the control arm is not"
    );

    set_releases_hidden(&conn, &batch, false).unwrap();

    assert_eq!(
        hidden_release_count(&conn).unwrap(),
        0,
        "undo takes back exactly what the batch wrote"
    );
}

#[test]
fn an_empty_batch_writes_nothing() {
    let conn = migrated_conn();
    insert_release(&conn, "one", None);

    set_releases_hidden(&conn, &[], true).unwrap();

    assert_eq!(hidden_release_count(&conn).unwrap(), 0);
}
```

Der Import oben in der Datei wird um `set_releases_hidden` erweitert
(`use crate::artist_news::{..., set_release_hidden, set_releases_hidden, ...};`).

- [ ] **Schritt 2: Rot sehen**

```
cargo test -p reprise-core hiding_a_batch
```
Erwartung: FAIL, `cannot find function set_releases_hidden`.

- [ ] **Schritt 3: Den transaktionslosen Kern herausziehen**

In `artist_news_query.rs` wird der Körper von `set_release_hidden_in` **ohne
inhaltliche Änderung** in eine Funktion ohne Transaktionsklammer verschoben:

```rust
/// One row's visibility, without a transaction of its own. The caller owns
/// the bracket — `set_releases_hidden` opens exactly one for the whole batch,
/// and nesting `unchecked_transaction()` inside it would fail outright.
pub(crate) fn apply_release_hidden_in(
    conn: &Connection,
    release_group_mbid: &str,
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    if !hidden {
        for mbid in
            crate::deleted_releases::forget_deleted_release_memory(conn, release_group_mbid)?
        {
            update_release_hidden_in(conn, &mbid, false)?;
        }
        return Ok(());
    }
    update_release_hidden_in(conn, release_group_mbid, true)
}
```

Beachte: der Restore-Pfad aktualisiert weiterhin **nur** die von
`forget_deleted_release_memory` zurückgegebenen MBIDs, wie bisher. Diese
Semantik wird nicht angefasst.

- [ ] **Schritt 4: Die beiden Einstiege**

```rust
pub fn set_releases_hidden(
    db: &crate::db::Db,
    release_group_mbids: &[String],
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    if release_group_mbids.is_empty() {
        return Ok(());
    }
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    for mbid in release_group_mbids {
        apply_release_hidden_in(&transaction, mbid, hidden)?;
    }
    transaction.commit()
}

pub fn set_release_hidden(
    db: &crate::db::Db,
    release_group_mbid: &str,
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    set_releases_hidden(db, std::slice::from_ref(&release_group_mbid.to_owned()), hidden)
}
```

`set_release_hidden_in` entfällt; die Aufrufer im Crate rufen künftig
`apply_release_hidden_in` (gleicher Vertrag, ohne Klammer) — such sie mit
`grep -rn "set_release_hidden_in" crates/reprise-core/src` und stell jede um.

In `artist_news.rs:76-79` wird `set_releases_hidden` mit re-exportiert.

- [ ] **Schritt 5: Grün, und die alten Tests müssen mitkommen**

```
cargo test -p reprise-core artist_news_query_tests
cargo test -p reprise-core deleted_release
```
Erwartung: PASS, insbesondere `hide_sets_hidden_and_set_release_hidden_false_restores_it`
(`artist_news_query_tests.rs:328`) — der beweist, dass der Umbau den
Restore-Pfad samt Deleted-Memory nicht verändert hat.

- [ ] **Schritt 6: Commit**

```bash
git add crates/reprise-core/src/artist_news_query.rs crates/reprise-core/src/artist_news.rs crates/reprise-core/src/artist_news_query_tests.rs
git commit -m "feat(releases): hiding writes one transaction per batch"
```

---

### Task 3: Die Tabelle wählt mehrfach aus — und behält die Auswahl über ein Neuladen

`ReleasesModel::replace` leert den Store und füllt ihn neu; jede Auswahl ist
danach weg. Solange nur eine Zeile ausgewählt sein konnte, fiel das nicht auf.
Nach dem Ausblenden muss die Ansicht aber wissen, wo der Cursor hin soll.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/releases/releases_model.rs:50-100`
- Test: `crates/reprise-gnome/src/ui/releases/releases_model.rs` (Testmodul unten)

**Interfaces:**
- Produces:
  ```rust
  pub(super) fn selection(&self) -> &gtk4::MultiSelection;
  pub(super) fn selected_mbids(&self) -> Vec<String>;
  pub(super) fn select_mbids(&self, mbids: &[String]);
  pub(super) fn position_of(&self, mbid: &str) -> Option<u32>;
  ```

- [ ] **Schritt 1: Der scheiternde Test**

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_reload_puts_the_selection_back_on_the_same_releases() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let model = ReleasesModel::new();
    model.replace(vec![entry("one"), entry("two"), entry("three")]);

    model.select_mbids(&["one".to_owned(), "three".to_owned()]);
    assert_eq!(model.selected_mbids(), vec!["one".to_owned(), "three".to_owned()]);

    // The same rows in a different order — positions move, identity does not.
    model.replace(vec![entry("three"), entry("two"), entry("one")]);
    model.select_mbids(&["one".to_owned(), "three".to_owned()]);

    assert_eq!(
        model.selected_mbids(),
        vec!["three".to_owned(), "one".to_owned()],
        "selection follows the rows, and reports them in view order"
    );
}
```

`fn entry(mbid: &str) -> HistoryEntry` ist der Testhelfer, der bereits für die
beiden vorhandenen Tests in dieser Datei existiert; falls er dort noch anders
heißt, nimm den vorhandenen.

- [ ] **Schritt 2: Rot sehen**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome a_reload_puts_the_selection -- --ignored
```
Erwartung: FAIL, `no method named select_mbids`.

- [ ] **Schritt 3: Umstellen**

`selection: gtk4::SingleSelection` → `gtk4::MultiSelection`, ebenso in `new()`
(`gtk4::MultiSelection::new(Some(store.clone()))`) und im Getter. Dazu:

```rust
pub(super) fn position_of(&self, mbid: &str) -> Option<u32> {
    (0..self.store.n_items()).find(|position| {
        self.store
            .item(*position)
            .and_downcast::<ReleaseObject>()
            .is_some_and(|object| object.entry().release_group_mbid == mbid)
    })
}

pub(super) fn selected_mbids(&self) -> Vec<String> {
    let bitset = self.selection.selection();
    let Some((mut iter, first)) = gtk4::BitsetIter::init_first(&bitset) else {
        return Vec::new();
    };
    let mut positions = vec![first];
    positions.extend(iter.by_ref());
    positions
        .into_iter()
        .filter_map(|position| self.store.item(position).and_downcast::<ReleaseObject>())
        .map(|object| object.entry().release_group_mbid)
        .collect()
}

pub(super) fn select_mbids(&self, mbids: &[String]) {
    self.selection.unselect_all();
    for mbid in mbids {
        if let Some(position) = self.position_of(mbid) {
            self.selection.select_item(position, false);
        }
    }
}
```

- [ ] **Schritt 4: Grün**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases_model -- --ignored
cargo check -p reprise-gnome
```
`cargo check` zeigt jede Stelle, die noch `SingleSelection` erwartet — vor allem
`releases_view.rs` und `releases_columns.rs`. Stell sie um; wo bisher
`selected_item()` gelesen wurde, nimm `selected_mbids()`.

- [ ] **Schritt 5: Die bestehenden Ansichtstests dürfen nicht kippen**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases_view_tests -- --ignored
```
Erwartung: alle 12 Tests aus `releases_view_tests.rs` PASS.

- [ ] **Schritt 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases
git commit -m "feat(releases): the table selects more than one row"
```

---

### Task 4: Die ganze Zeile antwortet auf die rechte Maustaste

Adwaita besitzt das Zellen-Padding. Eine Gesture am Kind der Factory deckt
gemessen 52 % der Zeile ab; der Rest pickt das private `GtkColumnViewCellWidget`
und tut nichts. `source_context_surface::wrap` und `css()` sind ein Vertrag: wer
`TABLE_CSS_CLASS` setzt, muss **jede** Zelle durch `wrap` bauen, sonst verlieren
diese Zellen ihr Padding.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/releases/releases_columns.rs` (jede `connect_setup`-Zelle)
- Modify: `crates/reprise-gnome/src/ui/releases/releases_view.rs:87` (Klasse am `ColumnView`)
- Test: `crates/reprise-gnome/src/ui/releases/releases_view_tests.rs`

**Interfaces:**
- Consumes: `crate::ui::source_context_surface::{wrap, TABLE_CSS_CLASS}`.

- [ ] **Schritt 1: Der scheiternde Test**

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn acc_1_every_point_of_a_release_row_answers_the_secondary_button() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_release(&conn, "one", "First Album");

    let view = ReleasesView::new(conn.clone(), PathBuf::new());
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();
    view.refresh();
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(
        crate::ui::source_context_surface::row_points_without_a_surface(&view.shared.column_view),
        Vec::<(i32, i32)>::new(),
        "the whole row carries the context surface, padding included"
    );
}
```

- [ ] **Schritt 2: Rot sehen**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome acc_1_every_point_of_a_release_row -- --ignored
```
Erwartung: FAIL mit einer langen Liste unbedeckter Punkte.

- [ ] **Schritt 3: Zellen einwickeln**

In `releases_view.rs` direkt nach dem Bau der `ColumnView`:

```rust
column_view.add_css_class(crate::ui::source_context_surface::TABLE_CSS_CLASS);
```

In `releases_columns.rs` endet **jede** `connect_setup`-Closure damit, dass sie
das gebaute Kind einwickelt und das Ergebnis setzt:

```rust
let surface = crate::ui::source_context_surface::wrap(&child);
item.set_child(Some(&surface));
```

Das betrifft ausnahmslos alle Spaltenfabriken der Datei (Cover, Date, Release,
Artist, Type, Status, Link). Eine vergessene Spalte fällt in Schritt 4 auf, weil
ihre Zellen sichtbar ihr Padding verlieren — und der Test bleibt rot.

Wo eine Zelle bisher `item.child()` zurücklas, um sie zu binden, liest sie jetzt
das Kind der Fläche: `item.child().and_downcast::<gtk4::Box>()?.first_child()`.
Prüfe jede `connect_bind`/`connect_unbind` dieser Datei darauf.

- [ ] **Schritt 4: Grün, und die Spaltenbreiten dürfen nicht wandern**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases -- --ignored
```
Erwartung: alle Releases-Tests PASS, inklusive der sechs Layout-/Header-Tests in
`releases_columns.rs`.

- [ ] **Schritt 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases
git commit -m "feat(releases): the whole row is the context surface"
```

---

### Task 5: Ctrl und Shift wählen wie in der Track-Liste

**Files:**
- Create: `crates/reprise-gnome/src/ui/releases/releases_selection.rs`
- Modify: `crates/reprise-gnome/src/ui/releases/mod.rs`
- Modify: `crates/reprise-gnome/src/ui/releases/releases_columns.rs` (Aufruf je Zelle)

**Interfaces:**
- Consumes: `crate::ui::table_selection::{resolve, validate, AnchorState, Anchored, SelectMode, SelectionOp, pointer_mode}` aus Task 1; `ReleasesModel::position_of` aus Task 3.
- Produces:
  ```rust
  pub(super) struct ReleasesAnchor(std::cell::RefCell<AnchorState<String>>);
  pub(super) fn wire_cell(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem, shared: &Rc<Shared>);
  pub(super) fn apply(shared: &Rc<Shared>, op: SelectionOp);
  ```

- [ ] **Schritt 1: Der scheiternde Test**

```rust
#[test]
fn nav_17_a_release_range_starts_at_the_anchor_and_never_moves_it() {
    let state = AnchorState {
        anchor: Some(Anchored { position: 4, id: "anchor".to_owned() }),
        cursor: Some(Anchored { position: 4, id: "anchor".to_owned() }),
    };
    let target = Anchored { position: 1, id: "target".to_owned() };

    let (op, next) = resolve(state, None, target, SelectMode::Range);

    assert_eq!(op, SelectionOp::SelectRange { start: 1, len: 4, replace: true });
    assert_eq!(
        next.anchor.map(|anchored| anchored.position),
        Some(4),
        "a range never moves the anchor"
    );
}

#[test]
fn nav_17_a_release_range_without_an_anchor_takes_only_the_clicked_row() {
    let state = AnchorState { anchor: None, cursor: None };
    let target = Anchored { position: 2, id: "target".to_owned() };

    let (op, _) = resolve(state, None, target, SelectMode::Range);

    assert_eq!(
        op,
        SelectionOp::SelectOnly(2),
        "releases have no playing row to fall back on"
    );
}
```

Der zweite Test ist der eigentliche Punkt: NAV-17 nennt die spielende Zeile als
Ersatzanker. Releases hat keine, also greift der dritte Zweig der Regel — und
das muss belegt sein, nicht angenommen.

- [ ] **Schritt 2: Rot sehen**

```
cargo test -p reprise-gnome nav_17_a_release_range
```
Erwartung: FAIL, Modul `releases_selection` existiert nicht.

- [ ] **Schritt 3: Verdrahten**

`releases_selection.rs` hält den Anker in `Shared` (neues Feld
`selection_anchor: RefCell<AnchorState<String>>`, Standard leer) und spiegelt
`track_list_selection_input::wire_cell_selection` — mit demselben Aufbau, aber
`Id = String`:

```rust
pub(super) fn wire_cell(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem, shared: &Rc<Shared>) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    let item = item.clone();
    let shared = shared.clone();
    gesture.connect_pressed(move |gesture, _, _, _| {
        let Some(mode) = crate::ui::table_selection::pointer_mode(
            gesture.current_event_state(),
        ) else {
            // Without Shift, GTK's own click handling is correct; we only
            // follow along so the anchor stays where the user last put it.
            return;
        };
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let Some(target) = anchored_at(&shared, position) else {
            return;
        };
        let state = validated_state(&shared);
        let (op, next) = crate::ui::table_selection::resolve(state, None, target, mode);
        *shared.selection_anchor.borrow_mut() = next;
        apply(&shared, op);
        gesture.set_state(gtk4::EventSequenceState::Claimed);
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}

pub(super) fn apply(shared: &Rc<Shared>, op: SelectionOp) {
    match op {
        SelectionOp::SelectOnly(position) => shared.model.selection().select_range(position, 1, true),
        SelectionOp::Toggle(position) => {
            if shared.model.selection().is_selected(position) {
                shared.model.selection().unselect_item(position);
            } else {
                shared.model.selection().select_item(position, false);
            }
        }
        SelectionOp::SelectRange { start, len, replace } => {
            shared.model.selection().select_range(start, len, replace)
        }
    }
}
```

`anchored_at` liest die MBID an der Position über `ReleasesModel`;
`validated_state` ruft `table_selection::validate` mit einem Lookup, der für eine
Position die MBID liefert — so fällt ein Anker weg, dessen Zeile das Neuladen
nicht überlebt hat.

In `releases_columns.rs` ruft jede `connect_setup`-Closure zusätzlich
`releases_selection::wire_cell(&surface, item, &shared)`.

- [ ] **Schritt 4: Grün**

```
cargo test -p reprise-gnome nav_17_a_release_range
cargo check -p reprise-gnome
```

- [ ] **Schritt 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases
git commit -m "feat(releases): Ctrl and Shift select rows from an anchor"
```

---

### Task 6: Das Menümodell, rein und ohne Widget

**Files:**
- Create: `crates/reprise-gnome/src/ui/releases/releases_menu.rs`
- Modify: `crates/reprise-gnome/src/ui/strings_releases.rs`
- Modify: `crates/reprise-gnome/src/ui/releases/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  pub(super) const ACTION_GROUP: &str = "releases";
  pub(super) const ACTION_HIDE: &str = "hide";
  pub(super) const ACTION_RESTORE: &str = "restore";
  pub(super) const ACTION_GO_TO_ARTIST: &str = "go-to-artist";
  pub(super) const ACTION_GO_TO_ALBUM: &str = "go-to-album";

  pub(super) struct MenuSelection {
      pub count: usize,
      pub all_hidden: bool,
      pub single_artist: Option<String>,
      pub single_is_local: bool,
  }
  pub(super) fn summarize(entries: &[HistoryEntry]) -> MenuSelection;
  pub(super) fn build(selection: &MenuSelection) -> gio::Menu;
  ```

- [ ] **Schritt 1: Die scheiternden Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn entry(mbid: &str, hidden: bool, local_track_count: i64) -> HistoryEntry {
        let mut entry = crate::ui::releases::test_entry(mbid);
        entry.hidden = hidden;
        entry.local_track_count = local_track_count;
        entry
    }

    // The idiom the other menu tests use (`primary_menu.rs:395`): read the
    // attribute by name, typed, rather than trusting a constant.
    fn labels(menu: &gio::Menu) -> Vec<String> {
        let mut found = Vec::new();
        for section in 0..menu.n_items() {
            let Some(items) = menu.item_link(section, gio::MENU_LINK_SECTION) else {
                continue;
            };
            for index in 0..items.n_items() {
                if let Some(label) = items
                    .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.get::<String>())
                {
                    found.push(label);
                }
            }
        }
        found
    }

    #[test]
    fn ctx_6_one_visible_release_offers_hide_without_a_count() {
        let menu = build(&summarize(&[entry("one", false, 0)]));
        assert_eq!(labels(&menu).first().map(String::as_str), Some("Hide"));
    }

    #[test]
    fn ctx_6_a_multi_selection_carries_the_count() {
        let selection = summarize(&[entry("one", false, 0), entry("two", false, 0)]);
        assert_eq!(labels(&build(&selection)).first().map(String::as_str), Some("Hide 2 releases"));
    }

    #[test]
    fn a_hidden_selection_offers_restore_instead_of_hide() {
        let menu = build(&summarize(&[entry("one", true, 0)]));
        assert_eq!(labels(&menu).first().map(String::as_str), Some("Show again"));
    }

    #[test]
    fn ctx_4_navigation_needs_exactly_one_row() {
        let single = build(&summarize(&[entry("one", false, 0)]));
        assert!(labels(&single).iter().any(|label| label == "Go to artist"));

        let many = build(&summarize(&[entry("one", false, 0), entry("two", false, 0)]));
        assert!(
            !labels(&many).iter().any(|label| label == "Go to artist"),
            "a multi-selection has no unambiguous artist to navigate to"
        );
    }

    #[test]
    fn go_to_album_appears_only_when_the_library_actually_holds_tracks() {
        let absent = build(&summarize(&[entry("one", false, 0)]));
        assert!(!labels(&absent).iter().any(|label| label == "Go to album"));

        let present = build(&summarize(&[entry("one", false, 3)]));
        assert!(labels(&present).iter().any(|label| label == "Go to album"));
    }
}
```

`crate::ui::releases::test_entry(mbid)` ist ein `#[cfg(test)]`-Helfer, den dieser
Task in `releases/mod.rs` anlegt: ein `HistoryEntry` mit gesetzter MBID,
`artist_name: "Artist"`, `title: "Album"` und ansonsten neutralen Feldern.

- [ ] **Schritt 2: Rot sehen**

```
cargo test -p reprise-gnome releases_menu
```
Erwartung: FAIL, Modul existiert nicht.

- [ ] **Schritt 3: Labels**

In `strings_releases.rs`, mit `use super::{formatted, plural, text};` im Kopf:

```rust
pub const RELEASES_HIDE: &str = N_!("Hide");
pub const RELEASES_SHOW_AGAIN: &str = N_!("Show again");
pub const RELEASES_GO_TO_ARTIST: &str = N_!("Go to artist");
pub const RELEASES_GO_TO_ALBUM: &str = N_!("Go to album");

/// CTX-6: only the entry that removes rows from view carries the count.
pub fn hide_releases_label(count: usize) -> String {
    count_label(count, RELEASES_HIDE, N_!("Hide {count} releases"))
}

pub fn show_releases_again_label(count: usize) -> String {
    count_label(count, RELEASES_SHOW_AGAIN, N_!("Show {count} releases again"))
}

fn count_label(count: usize, singular: &str, plural_message: &str) -> String {
    if count <= 1 {
        return text(singular);
    }
    let count_text = count.to_string();
    plural(singular, plural_message, count, &[("count", count_text.as_str())])
}
```

Sollte `RELEASES_SHOW_AGAIN` in dieser Datei bereits existieren (die Statusspalte
beschriftet ihren Button so), nimm die vorhandene Konstante statt einer zweiten.

- [ ] **Schritt 4: Modell bauen**

```rust
pub(super) fn summarize(entries: &[HistoryEntry]) -> MenuSelection {
    let single = (entries.len() == 1).then(|| &entries[0]);
    MenuSelection {
        count: entries.len(),
        all_hidden: !entries.is_empty() && entries.iter().all(|entry| entry.hidden),
        single_artist: single.map(|entry| entry.artist_name.clone()),
        single_is_local: single.is_some_and(|entry| entry.local_track_count > 0),
    }
}

pub(super) fn build(selection: &MenuSelection) -> gio::Menu {
    let menu = gio::Menu::new();
    if selection.count == 0 {
        return menu;
    }

    let primary = gio::Menu::new();
    if selection.all_hidden {
        primary.append(
            Some(&strings::show_releases_again_label(selection.count)),
            Some(&format!("{ACTION_GROUP}.{ACTION_RESTORE}")),
        );
    } else {
        primary.append(
            Some(&strings::hide_releases_label(selection.count)),
            Some(&format!("{ACTION_GROUP}.{ACTION_HIDE}")),
        );
    }
    menu.append_section(None, &primary);

    // CTX-4: navigation needs an unambiguous target, so it belongs to a
    // single row only. A hidden row keeps it — hiding is about the releases
    // list, not about the library the row points into.
    if selection.single_artist.is_some() {
        let navigation = gio::Menu::new();
        navigation.append(
            Some(&strings::text(strings::RELEASES_GO_TO_ARTIST)),
            Some(&format!("{ACTION_GROUP}.{ACTION_GO_TO_ARTIST}")),
        );
        if selection.single_is_local {
            navigation.append(
                Some(&strings::text(strings::RELEASES_GO_TO_ALBUM)),
                Some(&format!("{ACTION_GROUP}.{ACTION_GO_TO_ALBUM}")),
            );
        }
        menu.append_section(None, &navigation);
    }
    menu
}
```

- [ ] **Schritt 5: Grün**

```
cargo test -p reprise-gnome releases_menu
```
Erwartung: alle fünf Tests PASS.

- [ ] **Schritt 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases crates/reprise-gnome/src/ui/strings_releases.rs
git commit -m "feat(releases): a row menu model that reads its own selection"
```

---

### Task 7: Das Menü öffnet sich — per Maus und per Tastatur

**Files:**
- Create: `crates/reprise-gnome/src/ui/releases/releases_context_menu.rs`
- Modify: `crates/reprise-gnome/src/ui/releases/releases_view.rs` (Verdrahtung in `new`)
- Modify: `crates/reprise-gnome/src/ui/releases/releases_columns.rs` (Gesture je Zelle)

**Interfaces:**
- Consumes: `releases_menu::{build, summarize, ACTION_*}` (Task 6); `ReleasesModel::selected_mbids` (Task 3).
- Produces:
  ```rust
  pub(super) fn wire(column_view: &gtk4::ColumnView, shared: &Rc<Shared>);
  pub(super) fn wire_cell(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem, shared: &Rc<Shared>);
  pub(super) fn selected_entries(shared: &Rc<Shared>) -> Vec<HistoryEntry>;
  ```

- [ ] **Schritt 1: Der scheiternde Test**

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ctx_2_a_secondary_click_outside_the_selection_claims_that_row_first() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_release(&conn, "one", "First Album");
    insert_release(&conn, "two", "Second Album");

    let view = ReleasesView::new(conn.clone(), PathBuf::new());
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();
    view.refresh();
    crate::ui::source_context_surface::settle_layout();

    view.shared.model.select_mbids(&["one".to_owned()]);
    crate::ui::releases::releases_context_menu::claim_row_for_menu(&view.shared, 1);

    assert_eq!(
        view.shared.model.selected_mbids(),
        vec!["two".to_owned()],
        "the menu never acts on rows the pointer is not on"
    );

    view.shared.model.select_mbids(&["one".to_owned(), "two".to_owned()]);
    crate::ui::releases::releases_context_menu::claim_row_for_menu(&view.shared, 1);

    assert_eq!(
        view.shared.model.selected_mbids().len(),
        2,
        "a secondary click inside the selection leaves it alone"
    );
}
```

- [ ] **Schritt 2: Rot sehen**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome ctx_2_a_secondary_click -- --ignored
```

- [ ] **Schritt 3: Die Auswahlregel und das Popover**

```rust
/// CTX-2: a secondary click on a row outside the selection makes that row
/// the selection before the menu opens; inside it, the selection stands.
pub(super) fn claim_row_for_menu(shared: &Rc<Shared>, position: u32) {
    if !shared.model.selection().is_selected(position) {
        shared.model.selection().select_range(position, 1, true);
    }
}

pub(super) fn selected_entries(shared: &Rc<Shared>) -> Vec<HistoryEntry> {
    shared
        .model
        .selected_mbids()
        .into_iter()
        .filter_map(|mbid| shared.model.position_of(&mbid))
        .filter_map(|position| shared.model.store().item(position).and_downcast::<ReleaseObject>())
        .map(|object| object.entry())
        .collect()
}

pub(super) fn wire_cell(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem, shared: &Rc<Shared>) {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = crate::ui::source_context_surface::secondary_click();
    let item = item.clone();
    let shared = shared.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let Some(parent) = gesture.widget() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        claim_row_for_menu(&shared, position);
        let menu = releases_menu::build(&releases_menu::summarize(&selected_entries(&shared)));
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_has_arrow(false);
        popover.set_parent(&parent);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}
```

Die Tastaturseite spiegelt `radio_context_menu::wire_keyboard`: ein
`source_context_surface::context_keys()`-Controller am `ColumnView`, der bei
`is_context_menu_shortcut` und nicht-leerer Auswahl dasselbe Modell baut und das
Popover mittig über der Ansicht öffnet (Muster:
`track_list_context_keys::present_keyboard_popover`, das dabei den Fokus über
`transient_focus::TransientFocusGuard` zurückgibt — übernimm diesen Teil, sonst
verliert die Tabelle nach dem Schließen den Fokus).

Die Action-Group wird in `wire` gebaut und mit
`column_view.insert_action_group(releases_menu::ACTION_GROUP, Some(&group))`
gesetzt. Jede Action liest die Auswahl **beim Auslösen** neu über
`selected_entries`, nie einen beim Öffnen eingefrorenen Schnappschuss — sonst
handelt das Menü auf Zeilen, die es nicht mehr gibt.

`hide`/`restore` rufen `set_hidden_batch` aus Task 8, `go-to-artist`/`go-to-album`
den Navigations-Callback aus Task 9. Beide Tasks folgen unmittelbar. Leg die vier
Actions hier schon an, aber lass ihre Körper vorerst nur
`tracing::warn!("releases menu action not wired yet")` schreiben — nie `todo!()`,
damit der Zwischenstand unter keinen Umständen paniken kann.

- [ ] **Schritt 4: Grün**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases -- --ignored
```

- [ ] **Schritt 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases
git commit -m "feat(releases): a row menu opens by secondary click and by keyboard"
```

---

### Task 8: Hide schreibt, meldet sich und lässt sich zurücknehmen

**Files:**
- Create: `crates/reprise-gnome/src/ui/releases/releases_hide.rs`
- Modify: `crates/reprise-gnome/src/ui/releases/releases_view.rs:50-76` (`Shared`), `:381-390` (`set_hidden`)
- Modify: `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs` (Overlay reichen)
- Modify: `crates/reprise-gnome/src/ui/strings_releases.rs`

**Interfaces:**
- Consumes: `reprise_core::artist_news::set_releases_hidden` (Task 2); `ReleasesModel::{selected_mbids, select_mbids, position_of}` (Task 3).
- Produces:
  ```rust
  pub(super) fn set_hidden_batch(shared: &Rc<Shared>, mbids: Vec<String>, hidden: bool);
  pub(super) fn selection_after_hide(hidden_positions: &[u32], remaining: u32) -> Option<u32>;
  pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay);  // auf ReleasesView
  ```

- [ ] **Schritt 1: Der scheiternde Test für die Cursorregel**

Reine Arithmetik, deshalb ohne Display:

```rust
#[test]
fn the_cursor_lands_on_the_row_that_moved_up() {
    assert_eq!(selection_after_hide(&[1, 2], 4), Some(1));
}

#[test]
fn hiding_the_tail_falls_back_to_the_new_last_row() {
    assert_eq!(selection_after_hide(&[3, 4], 3), Some(2));
}

#[test]
fn an_emptied_list_selects_nothing() {
    assert_eq!(selection_after_hide(&[0, 1], 0), None);
}
```

- [ ] **Schritt 2: Der scheiternde Test für den Toast**

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn hiding_a_selection_offers_undo_and_undo_brings_the_rows_back() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_release(&conn, "one", "First Album");
    insert_release(&conn, "two", "Second Album");
    insert_release(&conn, "control", "Untouched Album");

    let view = ReleasesView::new(conn.clone(), PathBuf::new());
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(view.root()));
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(&overlay));
    window.present();
    view.set_toast_overlay(&overlay);
    view.refresh();
    crate::ui::source_context_surface::settle_layout();

    let toast = releases_hide::hide_for_test(
        &view.shared,
        vec!["one".to_owned(), "two".to_owned()],
    )
    .expect("hiding raises a toast");

    assert_eq!(toast.title(), "2 releases hidden");
    assert_eq!(toast.button_label().as_deref(), Some("Undo"));
    assert_eq!(toast.timeout(), 10, "FB-1: an undo toast runs ten seconds");
    assert_eq!(view.shared.model.store().n_items(), 1);
    assert_eq!(
        reprise_core::artist_news::hidden_release_count(&conn).unwrap(),
        2
    );

    toast.emit_button_clicked();
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(view.shared.model.store().n_items(), 3);
    assert_eq!(
        reprise_core::artist_news::hidden_release_count(&conn).unwrap(),
        0,
        "undo restores exactly the two rows the batch wrote"
    );
    assert_eq!(
        view.shared.model.selected_mbids(),
        vec!["one".to_owned(), "two".to_owned()],
        "the restored rows come back selected, so the user sees what returned"
    );
}
```

`hide_for_test(shared, mbids) -> Option<adw::Toast>` ist ein
`#[cfg(test)]`-Einstieg, der `set_hidden_batch` fährt und den erzeugten Toast
zurückgibt — `None`, wenn kein Overlay hängt. `control` ist der Kontrollarm: seine
Unversehrtheit steckt in `n_items() == 1` nach dem Ausblenden und `== 3` danach.

- [ ] **Schritt 3: Rot sehen**

```
cargo test -p reprise-gnome selection_after_hide
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome hiding_a_selection_offers_undo -- --ignored
```

- [ ] **Schritt 4: Die Toast-Naht legen**

`Shared` bekommt `toast_overlay: glib::WeakRef<adw::ToastOverlay>` (Standard
leer), `ReleasesView` den Setter — exakt das Muster aus `track_list.rs:258,550`:

```rust
pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
    self.shared.toast_overlay.set(Some(overlay));
}
```

Gerufen wird er dort, wo das Fenster die anderen Ansichten versorgt — such die
Stelle mit `grep -rn "set_toast_overlay" crates/reprise-gnome/src/ui/window` und
häng Releases daneben. Findet die `WeakRef` kein Overlay, bleibt es bei einer
`tracing::warn!`-Zeile: der Schreibvorgang darf nie am fehlenden Toast hängen.

- [ ] **Schritt 5: Schreiben, melden, Cursor setzen**

```rust
const UNDO_TOAST_TIMEOUT_S: u32 = 10;

pub(super) fn set_hidden_batch(shared: &Rc<Shared>, mbids: Vec<String>, hidden: bool) {
    if mbids.is_empty() {
        return;
    }
    let positions: Vec<u32> = mbids.iter().filter_map(|mbid| shared.model.position_of(mbid)).collect();

    if let Err(error) = artist_news::set_releases_hidden(&shared.conn, &mbids, hidden) {
        tracing::warn!(%error, count = mbids.len(), "could not change release visibility");
        return;
    }
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not reload Releases after visibility change");
    }
    notify_refreshed(shared);

    let remaining = shared.model.store().n_items();
    if let Some(cursor) = selection_after_hide(&positions, remaining) {
        shared.model.selection().select_range(cursor, 1, true);
    } else {
        shared.model.selection().unselect_all();
    }

    show_undo_toast(shared, mbids, hidden);
}

/// The row that took the place of the first row that left, else the new last
/// row, else nothing — a selection pointing at departed rows is not a state
/// this table is allowed to sit in.
pub(super) fn selection_after_hide(hidden_positions: &[u32], remaining: u32) -> Option<u32> {
    if remaining == 0 {
        return None;
    }
    let first = hidden_positions.iter().copied().min().unwrap_or(0);
    Some(first.min(remaining - 1))
}
```

`show_undo_toast` baut den Text über gezählte Formen, die in `strings_releases.rs`
neben den Menülabels aus Task 6 stehen:

```rust
pub fn releases_hidden_toast(count: usize) -> String {
    count_label(count, N_!("1 release hidden"), N_!("{count} releases hidden"))
}

pub fn releases_restored_toast(count: usize) -> String {
    count_label(count, N_!("1 release restored"), N_!("{count} releases restored"))
}
```

und ruft
`crate::ui::toasts::show_with_action(&overlay, &text, &strings::text(strings::UNDO), UNDO_TOAST_TIMEOUT_S, move || …)`.
Der Undo-Callback hält **die MBID-Liste**, nicht Positionen, ruft
`set_releases_hidden` mit invertiertem Flag, lädt neu und selektiert die Liste
über `model.select_mbids(&mbids)`.

Wichtig: Der Undo-Callback darf **keinen** zweiten Toast auslösen, sonst hängt an
jedem Undo ein neues Undo. Führ die Schreib- und Nachladelogik als eigene
Funktion ohne Toast und lass beide Wege sie rufen.

`set_hidden` (`releases_view.rs:381`) bleibt als Einzelfall bestehen und delegiert
an `set_hidden_batch` mit einem Element — der Aktivierungspfad
(`ReleasesRowAction::Restore`) benutzt ihn weiter und bekommt damit denselben
Toast.

- [ ] **Schritt 6: Grün**

```
cargo test -p reprise-gnome selection_after_hide
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases -- --ignored
```

- [ ] **Schritt 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases crates/reprise-gnome/src/ui/window crates/reprise-gnome/src/ui/strings_releases.rs
git commit -m "feat(releases): hiding announces itself and can be taken back"
```

---

### Task 9: Von der Zeile in die Bibliothek

Die Ansicht kennt den `MetadataNavigator` nicht und soll ihn auch nicht kennen —
sie meldet eine Absicht, das Fenster führt sie aus. Das ist dieselbe Naht wie
`set_on_launch_error`.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/releases/releases_view.rs` (Feld, Setter)
- Modify: `crates/reprise-gnome/src/ui/releases/releases_context_menu.rs` (Actions)
- Modify: `crates/reprise-gnome/src/ui/window/window_action_wiring.rs:307-329`

**Interfaces:**
- Consumes: `reprise_core::browser::{navigation::NavigationIntent, AlbumKey, ArtistKey}`.
- Produces: `pub(in crate::ui) fn set_on_navigate(&self, callback: impl Fn(NavigationIntent) + 'static)` auf `ReleasesView`;
  `#[cfg(test)] pub(super) fn activate_for_test(shared: &Rc<Shared>, action: &str)` in
  `releases_context_menu.rs`, das eine Action der Gruppe direkt auslöst, ohne ein
  Popover zu öffnen.

- [ ] **Schritt 1: Der scheiternde Test**

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_row_menu_navigates_by_name_because_the_library_has_no_ids() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_release(&conn, "one", "First Album");

    let view = ReleasesView::new(conn.clone(), PathBuf::new());
    let seen: Rc<RefCell<Vec<NavigationIntent>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let seen = seen.clone();
        view.set_on_navigate(move |intent| seen.borrow_mut().push(intent));
    }
    let window = gtk4::Window::new();
    window.set_child(Some(view.root()));
    window.present();
    view.refresh();
    crate::ui::source_context_surface::settle_layout();

    view.shared.model.select_mbids(&["one".to_owned()]);
    releases_context_menu::activate_for_test(&view.shared, releases_menu::ACTION_GO_TO_ARTIST);

    assert_eq!(
        seen.borrow().as_slice(),
        [NavigationIntent::OpenArtist {
            artist: ArtistKey::new("Artist"),
            anchor_track_id: None,
        }]
    );
}
```

`"Artist"` ist der Künstlername, den `insert_release` in
`releases_view_tests.rs` setzt — prüf ihn nach und nimm den tatsächlichen Wert.

- [ ] **Schritt 2: Rot sehen**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome the_row_menu_navigates_by_name -- --ignored
```

- [ ] **Schritt 3: Naht und Actions**

`Shared` bekommt `on_navigate: RefCell<Option<Rc<dyn Fn(NavigationIntent)>>>`,
`ReleasesView` den Setter nach dem Muster von `set_on_refreshed`
(`releases_view.rs:276-278`). Die beiden Actions:

```rust
let entries = selected_entries(&shared);
let [entry] = entries.as_slice() else {
    // CTX-4: the menu only offers navigation for a single row, so a
    // multi-row activation means the model and the actions disagree.
    tracing::warn!("navigation action fired without a single selected release");
    return;
};
let intent = NavigationIntent::OpenArtist {
    artist: ArtistKey::new(&entry.artist_name),
    anchor_track_id: None,
};
if let Some(callback) = shared.on_navigate.borrow().clone() {
    callback(intent);
}
```

`ACTION_GO_TO_ALBUM` analog mit
`NavigationIntent::OpenAlbum { album: AlbumKey::new(&entry.title, &entry.artist_name), anchor_track_id: None }`.

- [ ] **Schritt 4: Im Fenster verdrahten**

In `window_action_wiring.rs` neben der bestehenden Track-List-Navigation
(`:307-329`):

```rust
{
    let navigator = navigator.clone();
    releases_view.set_on_navigate(move |intent| {
        navigator.navigate(intent, NavigationReason::ContextMenu);
    });
}
```

`NavigationReason` heißt dort ggf. anders — nimm denselben Wert, den die
Track-List-Navigation an derselben Stelle übergibt.

- [ ] **Schritt 5: Grün**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases -- --ignored
cargo check -p reprise-gnome
```

- [ ] **Schritt 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases crates/reprise-gnome/src/ui/window
git commit -m "feat(releases): a single row navigates into the library"
```

---

### Task 10: Der Inline-Button verschwindet

Erst jetzt — vorher wäre Hide zwischenzeitlich unerreichbar.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/releases/releases_columns.rs:149-309`
- Modify: `crates/reprise-gnome/src/ui/releases/releases_view.rs` (`OnSetHidden`-Verdrahtung)
- Test: `crates/reprise-gnome/src/ui/releases/releases_view_tests.rs`

- [ ] **Schritt 1: Der scheiternde Test**

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_status_cell_is_a_pill_and_nothing_else() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_release(&conn, "one", "First Album");

    let view = ReleasesView::new(conn.clone(), PathBuf::new());
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();
    view.refresh();
    crate::ui::source_context_surface::settle_layout();

    assert!(
        crate::ui::test_widgets::descendants::<gtk4::Button>(&view.shared.column_view)
            .into_iter()
            .all(|button| button.label().as_deref() != Some("Hide")),
        "hiding is a menu action now, not a hover button"
    );
}
```

Nutze den vorhandenen Helfer, mit dem andere Tests Nachfahren eines Typs
einsammeln; falls es keinen gibt, schreib ihn als `#[cfg(test)]`-Funktion in
`releases_view_tests.rs` (rekursiver Abstieg über `first_child`/`next_sibling`).

- [ ] **Schritt 2: Rot sehen**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome the_status_cell_is_a_pill -- --ignored
```

- [ ] **Schritt 3: Ausbauen**

Aus `status_column` fallen weg: der `Button` (ACTION_PAGE), der `Stack` samt
`PILL_PAGE`/`ACTION_PAGE`-Umschaltung, der `EventControllerMotion` und der
`EventControllerFocus`. Es bleibt das `Label` mit der Statusklasse aus
`release_status`/`release_status_label` und die Spaltendefinition samt
`widths::pin(&column, widths::PILL)`.

Damit hat `status_column` keinen Parameter `on_set_hidden` mehr. Zieh den
Parameter aus `status_column`, `append_columns`, `append_columns_with_query`
sowie dem Typ `OnSetHidden` (`releases_columns.rs:22`), wenn ihn danach niemand
mehr braucht — `grep -rn "OnSetHidden" crates/reprise-gnome/src` sagt es dir.

- [ ] **Schritt 4: Grün, und die Spaltentests halten**

```
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases -- --ignored
```
Erwartung: PASS, insbesondere `nr_39_the_column_editor_lists_status_and_link_and_hides_them`.

- [ ] **Schritt 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/releases
git commit -m "feat(releases): the status cell stops carrying a hover button"
```

---

### Task 11: Die Regeln nachziehen

**Files:**
- Modify: `docs/ux-rules.md` (Abschnitt „R. New releases"; NR-39 bei Zeile 2561)

- [ ] **Schritt 1: NR-39 korrigieren**

NR-39 behauptet heute: *„Hiding both removes the visible routes for hiding a
release and for opening its purchase link"*. Nach Task 10 ist das für Hide falsch
— die Statusspalte ist kein Weg mehr dorthin. Streich den Hide-Teil des Satzes
und lass die Aussage für die Link-Spalte stehen.

- [ ] **Schritt 2: Die neue Regel schreiben**

Neue Regel am Ende des Abschnitts „R. New releases", nummeriert nach der höchsten
dort vergebenen ID. Wortlaut nach dem Vorbild von SRC-14 (Zeile 5691), damit
beide Tabellen dieselbe Sprache sprechen:

> **NR-40** [active] [gtk] — **Release rows select and answer like episode rows.**
> A click selects the row alone, Ctrl-click toggles it, Shift-click extends the
> selection from the anchor across the rendered order (NAV-17; the releases table
> has no playing row, so a missing anchor means Shift-click takes the clicked row
> alone). A secondary click on a row outside the selection makes that row the
> selection before the menu opens. The same selection-aware menu is reached by
> secondary click and by Menu/Shift+F10. It offers exactly one primary entry —
> „Hide" for visible rows, „Show again" for hidden ones, carrying the selection
> count per CTX-6 — plus, for a single selected row, „Go to artist" and, only
> when the library holds tracks for it, „Go to album". Hiding and restoring write
> one transaction for the whole selection and raise a ten-second toast with
> „Undo"; undo restores exactly that set and leaves it selected. Known
> limitation: with the status column's hover button gone (NR-39), hiding has no
> touch affordance and no primary-click route.

- [ ] **Schritt 3: NAV-17 ergänzen**

An NAV-17 (Zeile 265) einen Satz anhängen: die Ankerregel gilt für jede
Mehrfachauswahl-Tabelle und lebt in `ui/table_selection`; die Track-Liste setzt
die spielende Zeile als Ersatzanker ein, Tabellen ohne Wiedergabe fallen direkt
auf den dritten Zweig.

- [ ] **Schritt 4: Prüfen, dass keine tote Regel zitiert wird**

```
grep -n "NR-40\|NR-39" docs/ux-rules.md
```
Erwartung: NR-40 genau einmal, NR-39 unverändert doppelt vergeben (bestehende
Kollision im Dokument bei Zeile 2561 und 2586 — **nicht** in diesem Task
auflösen, nur nicht verschlimmern).

- [ ] **Schritt 5: Commit**

```bash
git add docs/ux-rules.md
git commit -m "docs(ux-rules): release rows select and answer like episode rows"
```

---

## Abschluss

- [ ] **Gesamtlauf**

```
cargo test -p reprise-core artist_news
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome releases -- --ignored
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a cargo test -p reprise-gnome nav_17 -- --ignored
cargo clippy -p reprise-core -p reprise-gnome -- -D warnings
```

Leite lange Läufe nach `$SCRATCH/<name>.log` um und beantworte die Frage per
`grep`; ganze Logs zurücklesen kostet in jedem Folgeturn erneut.

- [ ] **Zeilengrenze prüfen**

```
wc -l crates/reprise-gnome/src/ui/releases/*.rs
```
Erwartung: keine Datei über 800; `releases_columns.rs` unter dem Ausgangswert 798.
