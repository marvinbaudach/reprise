---
slug: updates-popover-delta
worktree: ~/Projects/reprise-updates-popover-delta
branch: feature/updates-popover-delta
phase: complete
codex_session:
created: 2026-08-03
---
# Updates-Popover: Delta-Melder statt Übersicht — Implementierungsplan

Branch `feature/updates-popover-delta`, Basis `origin/dev` (5fff82d1).
Design und Begründung: `docs/superpowers/specs/2026-08-03-updates-popover-delta-design.md`.
Der Plan ist ohne weiteren Kontext umsetzbar; alle Entscheidungen sind getroffen.

## 1. Ziel & Nicht-Ziele

**Ziel:** Der ✦-Popover beantwortet genau eine Frage — *was ist neu seit meinem
letzten Nachschauen?* Er zeigt eine gedeckelte Charge statt einer Volliste,
nennt die volle Chargengröße im Sektionskopf, lässt leere Sektionen verschwinden
und bekommt das dazu passende Layout.

**Nicht-Ziele:** Die Releases- und Concerts-Übersichtsseiten bleiben unberührt.
Fetch-Logik, Zeitplanung, Erstlaufzustand (NR-8) und Fehlersurface (NR-21/22)
bleiben unverändert. Keine DB-Migration — der vorhandene `seen_at`-Stempel
genügt.

## 2. Ausgangslage

| Datei | Rolle heute |
|---|---|
| `crates/reprise-core/src/updates.rs` | Badge-Arithmetik, `FeedRefresh`, `fetch_allowed` |
| `crates/reprise-core/src/artist_news_query.rs` | `query_releases`, `unseen_release_count`, `mark_releases_seen` |
| `crates/reprise-core/src/concerts/query.rs` | `query_unseen`, `count_unseen`, `mark_scope_seen` |
| `crates/reprise-gnome/src/ui/updates/popover.rs` | Orchestrierung, `render`, `opening_effect` |
| `crates/reprise-gnome/src/ui/updates/shell.rs` | Widget-Aufbau, 336 px, Scroller, Footer |
| `crates/reprise-gnome/src/ui/updates/feed_snapshot.rs` | Cache-Lesen für beide Feeds |
| `crates/reprise-gnome/src/ui/updates/concerts_section.rs` | Concerts-Zeilen, `MAX_DELTA_ROWS = 3` |
| `crates/reprise-gnome/src/ui/updates/release_row.rs` | Release-Zeile, Chip↔Aktionen-`Stack` |
| `crates/reprise-gnome/src/ui/updates/css.rs` | `new-release-*`-Klassen |
| `crates/reprise-gnome/src/ui/strings_news.rs` | Strings des Moduls |

Die drei Ursachen (U1 Volliste, U2 Stempel vor Zählung, U3 asymmetrische
Sektionen) stehen ausführlich in der Spec.

## 3. Schritt 1 — Core: die Charge (`updates.rs`)

Neu in `crates/reprise-core/src/updates.rs`, im Stil des vorhandenen Moduls
(Doc-Kommentar, der begründet *warum* das hier und nicht im Widget lebt):

```rust
/// Was der Updates-Popover für einen Feed zeigt.
pub struct DeltaBatch<T> {
    /// Die gedeckelte, anzuzeigende Menge.
    pub shown: Vec<T>,
    /// Die volle Chargengröße — Quelle der Zahl im Sektionskopf.
    pub total: usize,
}

pub fn delta_batch<T>(
    items: Vec<T>,
    seen_at: impl Fn(&T) -> Option<i64>,
    cap: usize,
) -> DeltaBatch<T>
```

Semantik, exakt:

1. Sind Einträge mit `seen_at == None` vorhanden → die Charge sind **genau
   diese**.
2. Sonst → die Charge sind alle Einträge mit `seen_at == Some(max)`, wobei `max`
   das größte vorkommende `seen_at` ist.
3. `total` = Länge der Charge **vor** der Deckelung. `shown` = die ersten `cap`
   Einträge in Eingabereihenfolge (die Aufrufer liefern bereits sortiert).
4. Leere Eingabe → `shown` leer, `total == 0`.
5. `cap == 0` → `shown` leer, `total` trotzdem die volle Chargengröße.

Die Reihenfolge der Eingabe bleibt in `shown` erhalten; die Funktion sortiert
nicht um.

Tests in `crates/reprise-core/src/updates_tests.rs` (das Modul ist bereits über
`#[path]` eingebunden): jede der fünf Regeln, plus der gemischte Fall (einige
`None`, einige `Some`) — dort gewinnen die `None`, und die gestempelten kommen
**nicht** dazu.

## 4. Schritt 2 — Core: Badge und Liste deckungsgleich

`unseen_release_count` schließt `LibraryPresence::Complete` aus. Die Delta-Liste
muss dieselbe Kandidatenmenge verwenden, sonst zeigt die Liste mehr, als das
Badge zählt.

In `crates/reprise-core/src/artist_news_query.rs` eine Funktion ergänzen, die
die Kandidatenmenge des Popovers liefert — dieselbe Filterung, die
`unseen_release_count` heute inline macht:

```rust
/// Die Kandidaten des Updates-Popovers: sichtbar, und nicht schon
/// vollständig in der Bibliothek. Dieselbe Menge, die `unseen_release_count`
/// zählt — Badge und Liste dürfen nicht auseinanderlaufen.
pub fn delta_candidates(
    db: &crate::db::Db,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error>
```

Implementierung: `query_releases_in(conn, false, today)` (also ohne hidden),
danach `retain(|r| r.presence != LibraryPresence::Complete)`.
`unseen_release_count` wird darauf umgestellt, damit es genau **eine** Definition
der Kandidatenmenge gibt. Ein Test hält fest, dass beide dieselbe Menge sehen —
ein Complete-Release taucht weder in `delta_candidates` noch in der Zählung auf.

Für Concerts wird nichts Neues gebraucht: `query_unseen` liefert schon
Ungesehenes, aber die Charge braucht auch den „letzter Besuch"-Fall. Deshalb in
`crates/reprise-core/src/concerts/query.rs` ergänzen:

```rust
/// Alle Events im Scope samt `seen_at`, ungedeckelt — die Eingabe für
/// `updates::delta_batch`.
pub fn query_scope_with_seen(
    db: &crate::db::Db,
    filter: &ConcertFilter,
    location: Option<&AppLocation>,
    today: NaiveDate,
) -> Result<Vec<(ConcertRow, Option<i64>)>, rusqlite::Error>
```

Implementierung: `filtered_events(...)` durchreichen, je Event `(event.row,
event.seen_at)`. `query_unseen` bleibt bestehen (andere Aufrufer möglich), wird
aber vom Popover nicht mehr benutzt.

## 5. Schritt 3 — GTK: `feed_snapshot.rs`

`ConcertsSnapshot.unseen: Vec<ConcertRow>` wird zu einer Charge:

```rust
pub(super) struct ConcertsSnapshot {
    pub credentials: bool,
    pub filter: ConcertFilter,
    pub delta: reprise_core::updates::DeltaBatch<ConcertRow>,
    pub count: usize,          // unverändert: Gesamtzahl für den Sprung-Link
}
```

Gefüllt über `query_scope_with_seen` → `delta_batch(rows, |(_, seen)| *seen, 3)`,
danach auf `ConcertRow` gemappt. Fehler weiterhin `tracing::warn!` + leere Menge.

Analog eine Funktion für Releases ergänzen, die `delta_candidates` liest und
`delta_batch(.., |r| r.seen_at, 5)` anwendet — die Deckelungen als benannte
Konstanten (`RELEASES_DELTA_CAP = 5`, `CONCERTS_DELTA_CAP = 3`), nicht als
Literale im Aufrufer. `MAX_DELTA_ROWS` in `concerts_section.rs` entfällt, das
Deckeln passiert jetzt an einer Stelle.

## 6. Schritt 4 — GTK: `popover.rs`

**Die Reihenfolge in `render` ist der Kern dieses Umbaus.** Heute läuft der
Stempelblock vor `unseen_release_count`, weshalb die Kopfzahl beim Öffnen immer
0 ist. Neue Reihenfolge, bindend:

1. Chargen beider Feeds bestimmen (`feed_snapshot`).
2. Rendern — Zeilen, Sektionsköpfe **und Kopfzahlen** aus `DeltaBatch::total`.
3. Erst danach stempeln (nur wenn `mark_seen == true`).
4. Erst danach Badge berechnen und setzen.

`opening_effect` wird angepasst: Es liefert die IDs **aller ungesehenen
Kandidaten** — nicht nur der angezeigten, und nicht mehr die aller gelisteten.
Sein Doc-Kommentar begründet heute die alte Regel und muss mitgezogen werden:
gestempelt wird die ganze Charge, weil das Badge sonst nie auf 0 käme; die
Kopfzahl nennt die volle Größe und der Sprung-Link führt zum Rest.

`mark_scope_seen` für Concerts bleibt wie es ist (es stempelt bereits den ganzen
Scope), wandert aber hinter das Rendern.

Sektions- und Leerlogik (B3):

- Releases-Sektion sichtbar ⇔ Modul aktiv **und** `total > 0`.
- Concerts-Sektion sichtbar ⇔ Modul aktiv **und** Credentials **und**
  `total > 0`.
- Beide unsichtbar → eine einzelne Zeile „Nothing new since your last look"
  zwischen Kopfzeile und Trennlinie.
- Der Erstlaufzustand (NR-8, `EmptyPresentation::Checking` / `NoReleases`)
  bleibt erhalten und hat Vorrang vor der Leerzeile: solange noch nie
  erfolgreich gefetcht wurde, gilt weiter die heutige Erstlauf-Darstellung.
- Die Sprung-Links bleiben sichtbar, solange ihr jeweiliges Modul aktiv ist —
  auch wenn ihre Sektion gerade wegfällt. Sonst wäre die Übersicht aus dem
  Popover nicht mehr erreichbar.

Der `new_tag` wird von einem Releases-Sonderfall zum gemeinsamen Zähler-Chip
beider Sektionen (zwei Instanzen derselben Klasse).

## 7. Schritt 5 — GTK: `shell.rs` (Layout)

- Breite `POPOVER_WIDTH` 336 → **380**.
- **Kopfzeile:** `updates_header` („UPDATES") links, rechts der Fetch-Bereich —
  Icon-Button (`view-refresh-symbolic`, Tooltip aus `FETCH_NOW`, weiterhin mit
  dem `Stack` Icon↔Spinner) und daneben das Alter (`updated`-Label). Der
  bisherige `build_footer`-Block entfällt als eigener Bereich; seine Widgets
  ziehen in den Kopf.
- **Fehlerzeile:** das `failure`-Label bekommt eine eigene Zeile direkt unter
  der Kopfzeile, standardmäßig unsichtbar (NR-21 unverändert).
- **Scroller entfällt.** Die `ListBox` wird direkt eingehängt;
  `SCROLLER_MAX_HEIGHT` und der `ScrolledWindow` fallen weg.
- **Sektionskopf:** Titel links, Zähler-Chip rechts — für beide Sektionen
  identisch aufgebaut. Der Concerts-Untertitel (`concerts_section_subtitle`,
  `UPDATES_NEW_NEAR_YOU` / `UPDATES_NEWLY_ANNOUNCED`) entfällt samt Funktion und
  ihrem Test.
- **Fuß:** Trennlinie, dann die zwei Sprungzeilen — unverändert in Aufbau und
  Reihenfolge.
- Die Leerzeile („Nothing new…") ist ein eigenes, dimmed Label im
  `UpdatesShell`, zentriert, standardmäßig unsichtbar.

## 8. Schritt 6 — GTK: `release_row.rs` (Hover)

Der `right_stack` (Crossfade zwischen `CHIP_CHILD` und `ACTIONS_CHILD`)
verschwindet. Stattdessen liegen Aktionen und Chip **nebeneinander** in einer
horizontalen Box: Aktionen links, Chip rechts.

- Die Aktionen sind permanent allokiert (der Platz ist reserviert, deshalb die
  380 px), aber mit `opacity = 0` und `can_target(false)` im Ruhezustand.
- `wire_hover_and_focus` setzt statt `set_visible_child_name` künftig
  `set_opacity(1.0)` + `set_can_target(true)` bzw. zurück. Die Zustandslogik
  (Pointer **oder** Fokus hält sie sichtbar) bleibt exakt wie sie ist,
  einschließlich `stack_target`s Wahrheitstabelle — nur das Ziel der Zuweisung
  ändert sich; die Funktion entsprechend umbenennen.
- `sensitive` bleibt **true**, damit die Buttons per Tastatur erreichbar sind;
  der Fokus-Enter blendet sie dann ein.
- Der Übergang läuft über `crate::ui::motion::MICRO_MS` wie der bisherige
  Crossfade und respektiert damit das Motion-Gating unverändert.

`release_row.rs` ist mit 28 KB die größte Datei des Moduls. Den Chip-/
Aktionsblock bei dieser Gelegenheit in eine eigene Datei
(`release_row_actions.rs`) herauslösen, mitsamt seinen Tests.

## 9. Schritt 7 — CSS (`css.rs`)

- `.new-release-tag` wird zum **gefüllten** Zähler-Chip:
  `background-color: @accent_bg_color; color: @accent_fg_color;` — statt der
  heutigen 18-%-Tönung. Die Status-Chips (`.new-release-chip`,
  `-neutral`, `-partial`) bleiben unverändert umrandet; Füllung schlägt
  Umrandung, damit gewinnt das Neu-Signal die Hierarchie ohne neue Farbe.
- Eine Klasse für die Leerzeile ergänzen (dimmed, zentriert) oder die
  vorhandene `dim-label`-Vokabel nutzen — keine neue Farbe erfinden.
- Alle Werte weiterhin über `@accent_*`/`@window_fg_color`, nie hartkodiert:
  ein Theme-Wechsel muss den Popover mitfärben (Modul-Doc oben in `css.rs`).

## 10. Schritt 8 — Strings (`strings_news.rs`)

- `UPDATES_NEW_RELEASES_HEADER`: Wert „NEW RELEASES" → „RELEASES".
- Neu: `UPDATES_NOTHING_NEW` = „Nothing new since your last look".
- `new_releases_new_count(count)` → in `updates_new_count(count)` umbenennen
  (liefert weiterhin „N new"), weil ihn jetzt beide Sektionen benutzen. Test
  mitziehen.
- `UPDATES_NEW_NEAR_YOU` und `UPDATES_NEWLY_ANNOUNCED` entfallen mitsamt ihrer
  Verwendung.
- Alle Strings über die vorhandenen `N_!`-Makros. **Die `po/*.po`-Dateien nicht
  von Hand bearbeiten** — sie werden separat regeneriert. `po/POTFILES.in` nur
  anfassen, wenn eine **neue Datei** mit Strings entsteht.

## 11. Schritt 9 — Regelwerk (`docs/ux-rules.md`, Abschnitt R)

Das Regelwerk ist bindend und muss mitgezogen werden. Nach dem etablierten
Muster: alte Regel auf `[replaced by …]` setzen, Text stehen lassen, neue Regel
darunter ergänzen.

- **NR-9a** → `[replaced by NR-9b]`. **NR-9b** [active] [gtk]: Der Popover zeigt
  die Charge — alle ungesehenen Einträge, und wenn es keine gibt, die Einträge
  des letzten Besuchs (`seen_at = MAX(seen_at)`). Öffnen stempelt die gesamte
  ungesehene Charge im Scope, auch unterhalb der Deckelung. Anzeige und
  Kopfzahl beruhen auf dem Zustand vor dem Stempel, das Badge auf dem danach.
  Vollständig in der Bibliothek vorhandene Releases führt der Popover nicht.
- **NR-10** → `[replaced by NR-10a]`. **NR-10a** [active] [gtk]: Hover oder
  Fokus blenden die Zeilenaktionen ein, ohne den Status-Chip zu verdrängen;
  der Chip bleibt in jedem Zustand sichtbar.
- **NR-13** → präzisieren: „In library"-Markierung und „Show in library" gelten
  in der Releases-Übersicht; der Delta-Popover führt diese Releases nicht.
- **NR-23** [active] [gtk] (neu): Deckelung 5 Releases / 3 Concerts, kein
  Scroller; der Zähler-Chip nennt die volle Chargengröße; eine Sektion ohne
  Charge fällt samt Kopf weg; sind beide leer, erscheint genau eine ruhige
  Zeile. Die Sprung-Links bleiben sichtbar, solange ihr Modul aktiv ist.

## 12. Tests

**Core (laufen ohne Display, hier liegt die Beweislast):**
- `updates_tests.rs`: die fünf `delta_batch`-Regeln plus der gemischte Fall.
- `artist_news_query`-Test: `delta_candidates` und `unseen_release_count` sehen
  dieselbe Menge; ein `Complete`-Release ist in keiner von beiden.
- `concerts`: `query_scope_with_seen` liefert Scope-Events samt `seen_at`.

**GTK (Display-Gate):**
- Regression U2: nach `render(mark_seen = true)` ist die Kopfzahl > 0 und das
  Badge unsichtbar — der Test, der den heutigen Fehler festnagelt.
- Regression U3: leere Charge blendet Kopf **und** Liste aus; kein Kopf ohne
  Inhalt.
- NR-10a nach STYLE-1: geprüft wird das **Ergebnis** — der Chip ist sichtbar
  und behält seine Allokation, während die Aktionen sichtbar sind. Nicht „die
  Eigenschaft ist gesetzt".
- Die vorhandenen `popover_tests.rs` mitziehen; Tests, die die alte Volllisten-
  Semantik festschreiben, ersetzen statt löschen.

**Hinweis zur Sandbox:** Die GTK-Tests hängen an einem Display, das in der
Codex-Sandbox nicht existiert. Sie sind trotzdem zu schreiben. Für die Läufe in
der Sandbox gilt: `cargo test -p reprise-core` muss grün sein und
`cargo build -p reprise-gnome` muss durchlaufen; die Display-Tests fährt der
Reviewer danach isoliert nach. `XDG_CACHE_HOME` auf ein Verzeichnis **innerhalb
des Worktrees** setzen, sonst laufen Cover-Tests falsch-rot.

## 13. Nicht anfassen

- Die Releases- und Concerts-Übersichtsseiten (`ui/releases/`, `ui/concerts/`).
- Fetch-Pfad, Zeitplanung, `FeedRefresh`, `fetch_allowed`, NR-8-Erstlauf.
- DB-Schema und Migrationen — dieser Umbau braucht keine.
- `po/*.po` (siehe Schritt 8).
- Dateien außerhalb dieses Worktrees.

## 14. Commits

Fokussierte Commits entlang der Schritte, englische Messages nach
`<type>: <description>`:

1. `feat(core): the batch a reader has not seen yet`
2. `refactor(core): one definition of the popover's candidates`
3. `feat(updates): show the batch, not the pile`
4. `feat(updates): the popover header carries fetch and age`
5. `feat(updates): row actions no longer evict the status chip`
6. `docs: NR-9b, NR-10a and NR-23 for the delta popover`
