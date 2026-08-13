---
slug: android-browse
worktree: ~/Projects/reprise-android-browse
branch: feature/android-browse
phase: planned
codex_session:
created: 2026-08-03
---
# Android, Paket 3 — Browsen und Suchen

**Goal:** Die App zeigt Alben, Interpreten und eine Suche — aus den Abfragen,
die `reprise-core` bereits hat.

**Basis:** `dev` (`be838e5c34`, Wiedergabe gemergt).

## Der eigentliche Zweck

Die alte Architekturfrage — wie viel Präsentationslogik lässt sich teilen —
wurde bisher **geschätzt**. Dieses Paket beantwortet einen Teil davon
**empirisch**, weil zum ersten Mal eine zweite echte Oberfläche danach fragt.

Der Kern hat bereits alles, was eine Browse-Ansicht braucht:

| | |
| --- | --- |
| `queries::library_views` | `query_albums`, `query_artists`, `query_album_track_ids`, `query_artist_detail_albums` |
| `queries::browse` | `BrowseFilter`, `BrowseFacet`, `query_browse_values` |
| `queries::clauses` | `build_track_query(sort_field, sort_dir, has_filter)` |

**Die Frage ist also nicht, ob Compose diese Logik nachbauen muss, sondern ob
sie ihr passt.** Trägt eine dieser Signaturen eine GTK-Annahme, zeigt sie sich
hier — so wie `LibrarySource` fünf Annahmen zeigte, als die erste fremde Quelle
kam.

**Notiere jede Stelle, an der die Kotlin-Seite etwas nachbauen muss, das
`reprise-gnome` schon hat.** Diese Liste ist wertvoller als die Funktion — sie
sagt, welche der vier verbleibenden P1a-Cluster wirklich geteilt gehören, und
sie ersetzt eine Schätzung durch eine Messung.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`,
  `bash scripts/tests/gettext-catalogs.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe. Testbilanz nach
  **Schlüsselwort** summieren.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Der Desktop darf sich nicht ändern.**
- **Keine Browse-, Sortier- oder Suchlogik in Kotlin.** Kommt eine
  Kern-Abfrage nicht mit dem aus, was Compose braucht, wird der Kern erweitert
  — nicht Kotlin. Sonst steht dieselbe Logik ein drittes Mal in TypeScript,
  wenn Tauri kommt.
- **`reprise-android-ffi` hängt nur an `reprise-core`.**
- Kein `#[allow(…)]`, kein Schema-Wechsel.

---

## Task 1: Die FFI-Oberfläche fürs Browsen

**Files:**
- Modify: `crates/reprise-android-ffi/src/`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Alben, Interpreten, Suche freilegen**

Alben und Interpreten auflisten, die Tracks eines Albums holen, und eine
Textsuche über `build_track_query`. Nur das — Sortierung nach Spalten,
Facetten und intelligente Playlists gehören nicht in diesen Schnitt.

**Halte in der Commit-Nachricht fest, wo eine Kern-Signatur nicht gepasst hat**
und was du stattdessen gebraucht hättest. Auch wenn du sie umgehen konntest.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 2: Die Oberfläche

**Files:**
- Modify: `android/app/src/main/java/…`

- [ ] **Step 1: Drei Ansichten und eine Suche**

Titel, Alben, Interpreten als Reiter; ein Suchfeld, das die Titelliste filtert.
Ein Album antippen zeigt seine Tracks; ein Track daraus füllt die Warteschlange
**mit diesem Album**, nicht mit der ganzen Bibliothek.

Kein Design-Anspruch. Lesbar und ehrlich reicht, wie in den Paketen davor.

- [ ] **Step 2: Volle Gates und Commit**

Den Gerätelauf nicht selbst ausführen.

---

## Task 3: Die Messung, um die es geht

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`

- [ ] **Step 1: Was Compose nachbauen musste**

Für jede Stelle: was `reprise-gnome` dafür hat, was Compose stattdessen tat,
und ob das eine geteilte Schicht rechtfertigt oder oberflächenspezifisch ist.

**Sei streng.** Eine Zeile Formatierung ist kein Fall für eine geteilte
Schicht. Eine Entscheidungsregel — welche Tracks ein Album umfasst, wie
sortiert wird, was eine leere Suche bedeutet — ist einer.

- [ ] **Step 2: Ledger, Gates, Commit**

---

## Was dieses Paket nicht ist

Keine intelligenten Playlists, keine Sortierspalten, keine Facetten, keine
Schreibseite. Drei Ansichten und eine Suche — genug, damit die App benutzbar
ist, und genug, damit die Messung etwas aussagt.
