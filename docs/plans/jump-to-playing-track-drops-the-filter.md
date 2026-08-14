---
slug: jump-to-playing-track-drops-the-filter
worktree: /home/marvin/Projects/reprise-jump-to-playing-track-drops-the-filter
branch: feature/jump-to-playing-track-drops-the-filter
phase: planned
codex_session:
created: 2026-08-14
---

# Der Sprung zum laufenden Titel vergisst den Filter — und „Clear all" bewegt die Ansicht nur einmal

## Was gebaut wird, in einem Absatz

Drei Dinge, die zusammengehören, weil sie dieselbe Frage beantworten: *wohin
schaut die Liste, nachdem eine Einschränkung verschwunden ist?* Erstens fährt
„Clear all ×" künftig **genau einen** Modell-Reload statt zwei, damit die
Zentrierung, die dieser Reload anstößt, nicht von einem zweiten Reload
überschrieben wird. Zweitens darf die Zentrierung ihren Erfolg nicht mehr aus
einer veralteten Widget-Allokation ableiten — heute meldet sie „passt schon,
nichts zu tun", wenn die *alte* gefilterte Liste in den Viewport passte, und
lässt die neue 2129-Zeilen-Liste oben stehen. Drittens bekommt das Leeren der
Suche eine Regel, die alle Wege (Chip-×, Escape, Feld leeren, „Clear all",
„Show all N tracks") gleich behandelt: zurück an die Vor-Such-Stelle — **außer**
es wurde während dieser Suche ein Titel gestartet, dann auf den laufenden Titel.
Und der Sprung zum laufenden Titel (Titel-Klick in der Player-Leiste,
Info-Spalte, `jump-to-now-playing`/`Ctrl+L`) behält von der Herkunfts-Place nur
Sammlung und Sortierung; Suchquery und Browse-Facetten fallen weg, während der
Eintrag, den der Nutzer verlässt, mit seinem Filter in die Zurück-Historie
wandert.

**Basis.** Gebaut wird auf `origin/dev` = **`57ff0bfc74`** (14.08.2026). Der
Entwurf zu diesem Plan war noch gegen `604677322e` geschrieben; seither sind
`#479` („ListLayout stops representing a state it can never be in",
`29b2edff4c`) und `#480` (Android) gelandet. `#479` hat `list_geometry.rs`,
`list_geometry_layout.rs`, das neue `list_geometry_content.rs`,
`reload_anchor_scroll.rs`, `reload_restore.rs` und `track_list_geometry.rs`
angefasst — also genau das Modul, auf dem Aufgabe 2 aufsetzt. Alle
Zeilenangaben in diesem Plan sind **`57ff0bfc74`-Nummern** und wurden für diesen
Plan einmal nachgemessen; sie sind trotzdem vor dem Bauen ein zweites Mal gegen
den dann aktuellen Stand zu prüfen. Die Symbolnamen sind das Verbindliche, nicht
die Zeilen.

---

## Ausgangslage

### Wurzel A1 — „Clear all" fährt zwei Reloads statt einem

Der Knopf hängt an einer Fensteraktion, nicht direkt am TrackList:
`browse_bar.rs:170-179` → `on_clear_all` → `window_runtime_wiring.rs:474-479`
aktiviert `win.clear-all-filters` → `window_runtime_wiring.rs:459-467` ruft
`SectionSearch::clear_all()`. Dieselbe Aktion hängt an „Show all N tracks"
(`track_list_builder.rs:68`) und an der Empty-State-Recovery
(`track_list_builder.rs:322`) — der Fix deckt diese drei Wege mit ab.

`SectionSearch::clear_all()` (`section_search.rs:296-310`) macht der Reihe nach:

1. `clear_facets()` → für `SearchScope::Tracks` (`section_search_wiring.rs:64-68`)
   → `TrackList::clear_all_restrictions()` (`track_list_filter_actions.rs:16-24`):
   leert `browse_filter`, `browse_bar`, `exclude_ai` und `shared.filter` und ruft
   `reload_centering_playing_track` (`track_list_reload.rs:360-362`) →
   `ReloadViewport::CenterPlayingTrack`. Das Modell wird neu befüllt, die
   Generation zählt hoch (`track_list_model.rs:547`), und
   `centered_scroll_restore::schedule` registriert seine Nachbesserungen auf
   dieser Generation (`centered_scroll_restore.rs:35` und `:50`).
2. `write_entry("")` (`section_search.rs:308`) — unkritisch: der debounced
   Handler in `view_session.rs:99-152` steigt bei `current_filter == entry.text()`
   früh aus (`:120-123`), weil `shared.filter` in Schritt 1 schon geleert wurde.
3. `apply_to_active("")` (`section_search.rs:309`) → `apply_to_scope`
   (`:348-361`) → Apply-Handler `section_search_wiring.rs:51-58` →
   `TrackList::set_filter("")` (`track_list.rs:445-447`) →
   `set_filter_and_reload` (`track_list_reload.rs:329-334`). Da `shared.filter`
   bereits `""` ist, liefert `filter_change_viewport("", "")` ein
   `PreserveAnchor` (`:92-100`) → **zweiter Modell-Reload**, Generation +1.

Der zweite Reload entwertet den ersten: beide verzögerten Zentrierungen prüfen
`shared.model.generation() == generation` und steigen wortlos aus. Belegt.

**Präzisierung gegenüber der ursprünglichen Diagnose.** Die Diagnose sagte, der
`PreserveAnchor`-Pfad schreibe „geschützt von 250 ms `AdjustmentHold`" die
unzentrierte Position zurück. Das ist nur die halbe Wahrheit und im gemeldeten
Fall vermutlich gar nicht der wirksame Mechanismus: der Hold entsteht in
`reload_with_anchor_and_viewport` nur, wenn `captured.anchor.is_some() ||
shared.pre_search_anchor.get().is_some()` **und** `adjustment.value() > 0.0`
(`track_list_reload.rs:437-448`). Wer im kurzen Trefferset oben steht, hat
`value == 0.0` → kein Hold. Und wenn zusätzlich nichts selektiert ist, ist der
Anker leer, `reload_restore::is_noop` greift und `restore_reload_anchor` kehrt
bei `PreserveAnchor` sofort zurück (`track_list_reload.rs:222-224`) — es wird
also gar nichts zurückgeschrieben. Der Schaden des zweiten Reloads ist **allein
die Generationserhöhung**, die die Zentrierung stilllegt; die Ansicht landet
dort, wohin GTKs eigene Allokation den alten, auf die neue Liste geklemmten Wert
setzt: oben. Das ändert nichts am Fix, aber ein Plan, der den Hold als Ursache
zementiert, schickt die Implementierung in die falsche Datei.

### Wurzel A2 — die Zentrierung leitet Erfolg aus einer veralteten Allokation ab

Das ist der Befund, der den Fix von A1 allein wertlos machen kann.
`centered_scroll_restore::apply` (`centered_scroll_restore.rs:62-88`) beginnt so:

```rust
fn apply(shared: &Shared, track_id: Option<i64>, current_ids: &[i64]) -> bool {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return false;
    };
    let page = adjustment.page_size();
    if adjustment.upper() <= page {
        return true;                       // "die Liste passt, nichts zu scrollen"
    }
    let Some(height) = ListGeometry::for_view(&shared.column_view)
        .live_row_height(current_ids.len())
        .map(crate::ui::list_geometry::RowHeight::pixels)
    else {
        return false;
    };
    …
```

`upper` ist unmittelbar nach dem Modelltausch aber noch die Allokation der
**alten** Liste — das ist keine Vermutung, das ist die Prämisse des ganzen
Moduls (`track_list_reload.rs:199-200`: „a freshly rebuilt list needs at least
one allocation pass before its adjustment reports usable geometry"). Wenn das
gefilterte Ergebnis in den Viewport passte — 15 Treffer auf „electr" in einem
900×320-Fenster ist der Normalfall —, gilt `upper <= page`, `apply` liefert
**`true`**, und `schedule` kehrt bei `centered_scroll_restore.rs:16-18` zurück,
**ohne** eine Nachbesserung zu registrieren und **ohne** `column_view.scroll_to`
zu rufen. Es gibt dann überhaupt keinen Zentrierungsversuch mehr — mit oder ohne
zweiten Reload.

War das Trefferset dagegen länger als der Viewport, greift der zweite Zweig:
`live_row_height` (`list_geometry.rs:482-485`) ruft `settled_row_height`
(`:143-155`), das `upper / n_rows` gegen die gemessene Widget-Höhe prüft. Mit
stale `upper` und neuer Zeilenzahl passt der Quotient nie → `None` → `apply`
liefert `false` → die Nachbesserungen werden scharfgemacht — und genau die
tötet dann A1. Das ist der Pfad, den die Diagnose beschreibt; er ist richtig,
aber er ist nur einer von zweien.

**Konsequenz für den Plan:** A1 und A2 müssen beide fallen, sonst ist der Bug je
nach Länge des Treffersets weiterhin reproduzierbar. Wer nur A1 baut und die
Abnahme mit einem langen Trefferset fährt, sieht Grün und hat den gemeldeten
Fall nicht getroffen.

### Nebenbefund A3 — derselbe zweite Reload wartet auf dem Rückweg von Aufgabe 5

`view_session::restore_browser_place` ruft am Ende `on_search_restored(&saved.search)`
(`view_session.rs:175-177`), und der Callback schreibt den Text in den
Header-Entry (`view_session.rs:92-96`). Der `restoring`-Guard schützt nur
`wire_search`s eigenen Handler — `SectionSearch` hängt mit
`entry.connect_search_changed` (`section_search.rs:106-111`) **ungeschützt** am
selben Entry und ruft `apply_to_active(&entry.text())`. Für eine *leere*
Query landet das über `section_search_wiring.rs:51-58` wieder bei
`set_filter("")` → `PreserveAnchor`-Reload. Sobald Aufgabe 5 die Query aus der
Ziel-Place entfernt, erzeugt jeder Sprung zum laufenden Titel also genau das
Zwei-Reload-Muster von A1 — diesmal gegen den Reveal-Anker. Der Fix für A1 muss
deshalb an einer Stelle sitzen, die beide Wege abdeckt (siehe Entscheidung D1).
Dieser Nebenbefund ist zugleich der Grund, warum Aufgabe 5 ohne Aufgabe 1 im
echten Fenster nicht abnehmbar ist (siehe `## Parallelität`).

### Wurzel B — der Sprung reist in die Ansicht zurück, aus der gespielt wurde

`window_playing_source_wiring.rs:53-82` baut den Track-Reveal mit
`origin = player.current_play_origin().place` (Rückfall
`BrowserPlace::from(ViewSource::Library)`, `:70-73`) und schickt
`NavigationIntent::RevealTrack`. `PlayOrigin` friert die **komplette** Place
inklusive Query ein — das ist kein Versehen, sondern PLAY-8 mit eigenem Test:
`play_origin.rs:206-220` (`play_8_origin_freezes_the_complete_browser_place`)
prüft ausdrücklich `origin.place.track_state().unwrap().search == "fire"`. **Der
Fix darf also nicht am `PlayOrigin` ansetzen**; der Queue-Kontext-Header (QUE-7)
und der Session-Restore (`session_restore.rs:156`) hängen an derselben Place.

Weiter: `metadata_navigation.rs:97-154` → `nav_history::navigate_from`
(`nav_history.rs:84-94`) → `BrowserNavigation::navigate`, Zweig
`navigation.rs:243-254`:

```rust
NavigationIntent::RevealTrack { origin, track_id } => {
    if track_id <= 0 { return None; }
    let mut target = if origin.track_state().is_some() { *origin }
                     else { self.library_root.clone() };
    set_explicit_track_anchor(&mut target, track_id);
    self.go_metadata_scope(target)
}
```

`target` behält `state.search` und `state.browse` (`browser.rs:143-150`), und
`library_shell::route_to_place` (`library_shell.rs:323-358`) →
`TrackList::restore_browser_place` → `view_session.rs:155-179` →
`prepare_track_view` (`:181-213`) setzt beides aktiv wieder, inklusive
`browse_bar.restore_filter(browse)` (`:199`). Der Chip ist zurück.

**Ergänzung, die die Diagnose nicht nennt:** `go_metadata_scope`
(`navigation.rs:316-331`) erkennt über `same_destination` (`:412-427`, vergleicht
**nur die Collection**), dass Ziel und Ausgangspunkt dieselbe Bibliothek sind,
und behandelt den Sprung als `Replace` — die gefilterte Place wird **ersetzt,
nicht auf den Back-Stack gelegt**. Wer heute aus der gefilterten Liste springt,
findet sie mit „Zurück" also gar nicht wieder; BROWSE-4 („Back restores the
point of origin", `docs/ux-rules.md:4585-4594`) ist für diesen Fall schon heute
nur auf dem Papier erfüllt. Sobald der Sprung die Query wegwirft, wird aus dem
Papierfehler ein spürbarer Verlust — die Historie-Frage ist damit nicht
optional, sondern Teil des Fixes.

Verdrahtung des Klicks (bestätigt): `player_bar.rs:205-220` →
`now_playing.rs:407-426` → `set_on_track_reveal`
(`window_playing_source_wiring.rs:156`); dieselbe Closure hängt an
`jump-to-now-playing` und `Ctrl+L` (`:163-166`).

Zweiter Produktions-Aufrufer von `RevealTrack`: `window_action_wiring.rs:191-202`
(My-Stats-Titel-Link) — der baut bereits eine **frische** `BrowserPlace::from(
ViewSource::Library)` ohne Query. Es gibt also genau zwei Sender, und beide
wollen „zeig mir diese Zeile", keiner will „nimm meine aktuelle Suche mit".

### Regellage heute (C)

* **SEARCH-9** (`docs/ux-rules.md:2889-2898`): „Emptying the query returns the
  viewport to where the list stood when the search began, as an
  ID-plus-offset anchor; if that row is gone, to the top." → das ist der
  `RestorePreSearch`-Pfad (`track_list_reload.rs:89, 238-241`).
* **FIL-9** (`:1602-1609`): eine **Facetten**-Änderung zentriert den geladenen
  Titel; „The header-bar search is no longer covered: SEARCH-9 governs it." →
  das ist `reload_centering_playing_track`, verdrahtet an
  `browse_bar.set_on_changed` (`track_list_builder.rs:251-259`).
* „Clear all" ist beides gleichzeitig und folgt heute **bedingungslos FIL-9**
  (`track_list_filter_actions.rs:23`), während das Chip-× bedingungslos
  SEARCH-9 folgt. Genau diese Inkonsistenz meldet der Nutzer, und die
  Nutzerentscheidung vom 14.08.2026 löst sie auf.

Höchste vergebene IDs (nachgemessen): SEARCH-15 (`:2125`), FIL-9 (`:1602`),
BROWSE-13 (`:4664`), NAV-17 (`:263`), PLAY-14 (`:395`). Frei sind also
**SEARCH-16**, **FIL-10**, **BROWSE-14**. Die Prozessregeln (`:18-21`) sind
append-only: IDs werden nie umgewidmet, geänderte Bedeutung bekommt eine neue
(Unter-)Regel.

### Was die Suite heute nicht sieht

* `clear_all_restrictions_resets_search_and_browse_in_one_pass`
  (`track_list_filter_actions.rs:50-86`) ruft `clear_all_restrictions()` direkt,
  fährt Schritt 3 nie und misst Filterzustand und Trefferzahl — **nie die
  Scrollposition**. Der Testname behauptet „in one pass", ohne die Anzahl der
  Reloads zu messen.
* `search_9_filter_change_decides_viewport_by_the_new_query`
  (`track_list_reload_tests.rs:33-55`) ist rein funktional auf
  `filter_change_viewport` und kennt weder Wiedergabe noch Viewport.
* `typed_search_reads_from_the_top_and_clearing_comes_back`
  (`search_viewport_display_tests.rs:22-109`) misst die Scrollposition richtig,
  aber ohne laufenden Titel — bleibt unter der neuen Regel grün und ist genau
  deshalb kein Wächter für den gemeldeten Fall.
* `browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track`
  (`reveal_track_display_tests.rs`) baut die Place **von Hand** und ruft
  `restore_browser_place` direkt — der Router, in dem die Query steckenbleibt,
  wird nie durchlaufen. Der Kommentar der Datei sagt selbst, sie simuliere „what
  the player bar's title link does".

---

## Entscheidungen (mit Begründung)

**D1 — Der eine Reload entsteht durch einen No-op in `set_filter_and_reload`,
nicht durch das Auslassen von Schritt 3.**
Abgewogen wurden drei Ansätze:

| Ansatz | Warum nicht / warum |
|---|---|
| In `clear_all()` Schritt 3 auslassen, wenn ein `clear_facets`-Handler lief | **Verworfen.** Schritt 3 macht zwei Dinge: `apply` *und* `commit` (`apply_to_scope`, `section_search.rs:348-361`). `commit("")` ist das, was den Suchchip löscht (`set_committed_search_query`). Wer Schritt 3 überspringt, lässt den Chip stehen — und für `SearchScope::Missing` (`section_search_wiring.rs:73-91`) ist `apply` die einzige Stelle, die `set_missing_search_query` zurücksetzt. Podcasts/YouTube/Radio/Releases/Concerts (`:94-224`) haben `clear_facets`-Handler, die **keinen** TrackList-Reload fahren; eine Sonderbehandlung „wenn ein Handler lief" wäre für sie schlicht falsch. |
| Reihenfolge umdrehen (erst `apply`, dann `clear_facets`) | **Verworfen.** Bei nicht-leerer Query bleiben es zwei Reloads (erst `RestorePreSearch`, dann `CenterPlayingTrack`), nur der letzte gewinnt. Zusätzlich verbraucht der erste bereits `pre_search_anchor` (`track_list_reload.rs:239`, `take()`), womit die neue Regel aus D3/G4 ihre Rückfallposition verliert. |
| **`set_filter_and_reload` kehrt zurück, wenn sich der Filter nicht ändert** | **Gewählt.** Ein Filter-Reload ohne Filteränderung ist per Definition Arbeit ohne Wirkung. Der Fix sitzt an *einer* Stelle, deckt A1 **und** A3 ab (Aufgabe 5 braucht ihn), und ändert für keinen Scope außer Tracks etwas — die anderen Scopes rufen `TrackList::set_filter` gar nicht. |

Nachgeprüfte Aufrufer von `TrackList::set_filter` / `set_filter_and_reload`:
`section_search_wiring.rs:56` (nur leere Query), der Dev-Hook
`arm_smoke_filter`, und drei Tests (`track_list_filter_actions.rs:68`,
`search_viewport_display_tests.rs:91,99`) — alle mit echter Änderung. Kein
Aufrufer verlässt sich auf „Reload als Auffrischung"; wer das will, ruft
`reload()`.

**D2 — Die Zentrierung darf „passt schon" nur aus gemessener Geometrie
schließen. `Assumed` heißt „später nochmal", nicht „fertig".**
*(Vom Grill gegenüber dem Entwurf verschärft; keine offene Frage mehr.)*

In `centered_scroll_restore::apply` entfällt die `upper <= page`-Abkürzung
ersatzlos. An ihre Stelle tritt eine Aussage über die **aktuelle** Zeilenzahl:

```rust
let n_sections = shared.queue_sections.borrow().len();   // Borrow sofort fallenlassen
let (content, row_source, header_source) = ListGeometry::for_view(&shared.column_view)
    .content_height(
        &shared.conn,
        &shared.list_geometry_cache,
        current_ids.len(),
        n_sections,
    );
let measured = row_source == RowHeightSource::Measured
    && header_source.is_none_or(|source| source == RowHeightSource::Measured);
if let ContentHeight::Known(px) = content {
    if measured && px <= page {
        return true;                      // wirklich nichts zu scrollen
    }
}
// keine weitere Abkürzung: der bestehende Rumpf ab `live_row_height` entscheidet
```

Die Signatur ist nachgeprüft: `ListGeometry::content_height`
(`list_geometry.rs:487-506`) liefert `(ContentHeight, RowHeightSource,
Option<RowHeightSource>)`; das zweite Element ist die Quelle der Zeilenhöhe, das
dritte die der Sektionskopfhöhe (`None`, wenn `n_sections == 0`).
`RowHeightSource` (`list_geometry.rs:30-34`) hat **genau zwei** Varianten,
`Assumed` und `Measured` — eine `Cached`-Variante existiert nicht. „Gecacht" und
„gemessen" fallen im Code zusammen: `trusted_row_height` (`:414-422`) liefert
`Measured` genau dann, wenn `TrustedRowHeight::from_cache` eine gesetzte
Messung im `ListGeometryCache` findet, und sonst `Assumed` (den CSS-Boden).
Die Bedingung „gemessen oder gecacht" aus der Nutzerentscheidung ist damit
exakt `row_source == RowHeightSource::Measured`.

**Warum `Assumed` nicht reicht — der Fehler ist asymmetrisch.** Ein falsches
„passt in den Viewport" bringt genau den gemeldeten Bug zurück: die Liste bleibt
oben stehen und niemand versucht es noch einmal. Ein falsches „passt nicht"
kostet eine Nachbesserungsrunde und sonst nichts. Die Runden sind endlich —
`schedule` registriert einen `changed`-Callback (`centered_scroll_restore.rs:35`)
und ein Idle (`:50`), mehr nicht.

**Warum kein Pfad verlorengeht, der heute funktioniert.** Alles, was die
Bedingung nicht erfüllt, fällt in den bestehenden Rumpf. Dessen nächster Schritt
ist `live_row_height` (`:70-75`), das über `settled_row_height` prüft, ob
`upper / n_rows` zur gemessenen Widget-Höhe passt. Bei stale Allokation liefert
es `None` → `apply` gibt `false` zurück, **ohne das Adjustment anzufassen**. Es
gibt also keinen Weg, auf dem der Wegfall der Abkürzung eine falsche Scrollung
erzeugt; das schlimmste Ergebnis ist eine zusätzliche Nachbesserungsrunde.
Umgekehrt liefert `live_row_height` nur dann `Some`, wenn die Allokation frisch
ist — genau die Fälle, die heute schon korrekt durchlaufen.

*Nicht gewählt:* die beiden Prüfungen einfach zu tauschen. Wenn die Liste
wirklich kürzer als der Viewport ist, meldet GTK `upper == page_size`, der
Quotient `upper / n_rows` passt dann nicht zur Zeilenhöhe, `live_row_height`
liefert `None`, und die Abkürzung wäre für den legitimen Fall verloren.

**Preis, der benannt gehört:** `center_loaded_track` / START-3
(`track_list_reload.rs:307-322`) teilt sich `apply`. Auf einem frischen Profil
kann die Zeilenhöhe beim ersten Start `Assumed` sein; dann meldet `apply` nicht
mehr früh „fertig", sondern bessert nach. Das ist die gewünschte Richtung, aber
es ist eine Verhaltensänderung im Startpfad. `start_restore_tests.rs` und
`fresh_start_allocation_display_tests.rs` sind deshalb **Pflichtläufe in
Aufgabe 2**, nicht bloß Erwähnungen im Risikoabschnitt.

**D3 — „Während der Suche wurde ein Titel gestartet" wird an der Wiedergabe
gemessen, nicht am Vergleich zweier Track-Ids.**
`shared.playing_track_id` wird in `current_track_selection.rs` gesetzt
(`update_current_track`, ~`:292`) und in `clear_now_playing` (~`:407`) geleert;
ein Vergleich „vorher/nachher" wäre möglich, würde aber den **Auto-Advance**
mitzählen: der Nutzer sucht, das Album läuft weiter, die Id ändert sich — ohne
dass er etwas getan hat. `CurrentTrackChange` (`current_track_selection.rs:23-28`)
unterscheidet das bereits: `PlaybackStarted`, `ExplicitTransport`,
`AutomaticAdvance`, `SessionRestore`. Gewählt: ein Merker wird gesetzt bei
`PlaybackStarted` **oder** `ExplicitTransport` (beides sind Nutzerhandlungen)
und **nicht** bei `AutomaticAdvance` oder `SessionRestore`, jeweils nur solange
`shared.filter` nicht leer ist.

Damit sind die Randfälle beantwortet:
*„vorher lief nichts, jetzt läuft etwas"* → `PlaybackStarted` → zentrieren.
*„vorher lief etwas, jetzt nichts"* (gestoppt) → kein Start, Merker bleibt
falsch → Vor-Such-Anker; zusätzlich ist `playing_track_id` dann `None`, und der
Zentrierungszweig prüft das ohnehin (`track_list_reload.rs:230`).
*„derselbe Titel läuft noch"* → kein Ereignis, kein Merker → Vor-Such-Anker.
Genau das will der Nutzer: hat er nur gesucht, kommt er zurück.

**D4 — Merker und Anker sind ein Zustand, kein Paar.**
`pre_search_anchor: Cell<Option<(i64, f64)>>` (`track_list.rs:159`) wird durch
`Cell<PreSearch>` mit `{ anchor: Option<(i64, f64)>, playback_started: bool }`
ersetzt (`Copy`, unveränderlich fortgeschrieben). Grund: der Merker hat exakt
denselben Lebenszyklus wie der Anker — gesetzt in `prepare_filter_change`
(`track_list_reload.rs:339-347`), genullt in `set_source_and_reload` (`:387`)
und `prepare_track_view` (`view_session.rs:196`), verbraucht beim Leeren
(`:239`). Zwei separate Cells wären dieselbe Entscheidung an zwei Stellen und
driften garantiert auseinander. *Kostenpunkt:* acht Codestellen, alle in
Aufgabe 3 aufgezählt.

**D5 — Der laufende Titel muss in der Zielansicht nicht enthalten sein.**
In einer Playlist- oder Smart-Ansicht kann der geladene Titel fehlen, auch ohne
Filter. `restore_reload_anchor` prüft das bereits (`track_list_reload.rs:230`:
`playing_track_id.is_some_and(|track_id| current_ids.contains(&track_id))`). Die
neue Viewport-Variante fällt in diesem Fall auf den Vor-Such-Anker zurück, und
wenn auch der fehlt, bleibt SEARCH-9s letzte Zusage stehen: „if that row is
gone, to the top". Deshalb **eine** neue Variante mit Rückfall statt zweier
Varianten, über die vor dem Query entschieden werden müsste — vor `run_query`
weiß niemand, ob der Titel in der ungefilterten Liste steht.

**D6 — Aufgabe 5 setzt im Core an, im `RevealTrack`-Zweig.**
Verglichen wurden vier Stellen:

* **`PlayOrigin`** — verworfen, bricht PLAY-8 (`play_origin.rs:206-220`) und den
  Queue-Kontext-Header.
* **`window_playing_source_wiring.rs`** (Intent-Konstruktion) — möglich, aber
  der zweite `RevealTrack`-Sender (`window_action_wiring.rs:195`) müsste
  dieselbe Regel wiederholen: dieselbe Entscheidung an zwei Stellen.
* **`restore_browser_place` / `route_to_place`** — verworfen: dieselbe Funktion
  bedient Back/Forward, und dort **muss** die Query exakt zurückkommen (BROWSE-2,
  `docs/ux-rules.md:4572-4577`). Eine Fallunterscheidung dort wäre ein
  Bool-Parameter durch drei Schichten.
* **`navigation.rs:243-254`** — **gewählt.** Eine Stelle, `[core]`-Test ohne
  Display, deckt beide Sender ab. Die Semantik gehört dorthin: „RevealTrack"
  heißt „zeig mir diese Zeile", nicht „zeig mir diese Zeile, falls mein Filter
  sie durchlässt".

**D7 — Der verlassene Eintrag behält seinen Filter und landet auf dem
Back-Stack.** *(Vom Grill bestätigt.)* Wenn der Reveal tatsächlich etwas
wegwirft (Query oder Facetten nicht leer), routet der Zweig über `go_new`
(`navigation.rs:333-346`) statt über `go_metadata_scope`; `navigate_from` hat die
aktuelle, gefilterte Place kurz zuvor über `replace_current`
(`nav_history.rs:136-140`) als `current` gesetzt, sie wird also unverfälscht auf
`back` gelegt. Wirft der Reveal nichts weg, bleibt alles wie heute (`Replace`,
kein Historieneintrag) — sonst würde jedes `Ctrl+L` in einer ungefilterten
Bibliothek die Historie mit identischen Einträgen zumüllen. Das erfüllt
BROWSE-4s „Back restores the point of origin" für diesen Fall erstmals wörtlich.

**D8 — Nur der Titel-Klick. Album- und Interpreten-Reveals bleiben in diesem
Plan unverändert.** *(Vom Grill bestätigt: der Umfang ist der Titel-Klick.)*
Cover (`OpenAlbum`) und Interpretenzeile (`OpenArtist`) der Player-Leiste
behalten ihre Query. Begründung: `OpenAlbum`/`OpenArtist` werden **auch** von
Zeilen-Drills gesendet (Kontextmenü „Go to album", Cover-Klick in der Tabelle),
und dort trägt `metadata_target_state` (`navigation.rs:300-314`) die Query
bewusst mit — SEARCH-8a (`docs/ux-rules.md:2867-2888`) nennt das ausdrücklich:
„carries the query that explains why the clicked row was visible". Der Intent
allein kann Drill und Sprung nicht unterscheiden; eine Änderung dort bräuchte
ein zusätzliches Feld (`carry_query: bool`) und eine eigene SEARCH-8a-Revision.
Zweitens wechselt der Album-/Interpreten-Sprung die Destination, die Query
erscheint dort als sichtbarer, wegklickbarer Chip — während der Track-Reveal in
derselben Liste bleibt, in der der Filter genau das versteckt, was gesucht wird.

Das ist eine **bewusst offengelassene Lücke, keine Behauptung von Vollständigkeit**:
wer den Bug dort erneut meldet, hat recht. Die Antwort darauf ist der benannte
Folgeplan am Ende dieses Dokuments, keine stille Erweiterung dieses Plans.

**D9 — „Hide AI music" bleibt.** `exclude_ai` ist kein Teil der `TrackViewState`
(`browser.rs:143-150`), sondern klebriger Zustand der BrowseBar (FIL-7,
`docs/ux-rules.md:1578-1593`). Der Sprung kann ihn folglich weder mitnehmen noch
löschen, und soll es auch nicht: es ist eine Voreinstellung, keine Ortsangabe.
(„Clear all" löscht ihn sehr wohl — `clear_all_restrictions` ruft
`browse_bar.clear_exclude_ai()`, `track_list_filter_actions.rs:22`; das ist
FIL-7s ausdrückliche Ausnahme und bleibt unverändert.)
Randfall, der benannt gehört: ist der laufende Titel KI-markiert und der Filter
an, führt der Sprung in eine Liste ohne ihn — `restore_reload_anchor` findet die
Id nicht und lässt den Viewport in Ruhe. Kein Absturz, kein Toast (der
`contains_track`-Vorfilter in `metadata_navigation.rs:129-138` fragt den Katalog,
nicht die Ansicht — `track_list.rs:525-532`). Dokumentieren, nicht reparieren.

---

## Aufgaben

### Aufgabe 1 — Ein Filter-Reload nur bei echter Filteränderung

**Dateien:** `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`
(`set_filter_and_reload`, `:329-334`), Doc-Kommentare in
`crates/reprise-gnome/src/ui/track_list/track_list_filter_actions.rs:13-15` und
`crates/reprise-gnome/src/ui/window/section_search.rs:293-310`.

**Änderung:** `set_filter_and_reload` kehrt ohne Wirkung zurück, wenn
`shared.filter` bereits `text` entspricht — also **vor** `prepare_filter_change`,
damit der Vor-Such-Anker nicht überschrieben wird. Der Kommentar erklärt, dass
„Clear all" den Filter über `clear_facets` schon geleert hat und der nachfolgende
`apply`-Aufruf nur noch den Chip (`commit`) zu erledigen hat. Der Doc-Kommentar
über `clear_all_restrictions`, der „one action resets search and browse facets
in a single reload" behauptet, wird auf die tatsächliche Mechanik korrigiert:
der Handler steigt nicht aus — der Reload tut es.

**Test:** `fil_2a_clear_all_reloads_the_track_list_once` (neu, `[gtk]`, Display,
in `track_list_filter_actions.rs`). Zählt die `on_reload`-Aufrufe über die
Closure, die `TrackList::new` ohnehin entgegennimmt (`track_list.rs:377-392`,
Parameter `on_reload` in `:380`), fährt den **echten** Weg:
`install_tracks(&search, &track_list)` wie in `section_search_wiring.rs`s
eigenem Test (`:226-297`), Query setzen, Zähler zurücksetzen,
`SectionSearch::clear_all()` — also den Rumpf der Fensteraktion
`win.clear-all-filters` (`window_runtime_wiring.rs:459-467`) — und dann
`assert_eq!(reloads, 1)`.

**Abnahme:** Genau ein Reload; `shared.filter` leer; `browse_filter` auf
Default; der committete Chip leer. Der bestehende Test
`clear_all_restrictions_resets_search_and_browse_in_one_pass` bleibt grün.
Der `SearchScope::Missing`-Weg (`section_search_wiring.rs:73-91`) wird einmal
mitgefahren, damit die Query dort nicht stehenbleibt.

### Aufgabe 2 — Die Zentrierung glaubt keiner veralteten und keiner angenommenen Geometrie

**Dateien:** `crates/reprise-gnome/src/ui/track_list/centered_scroll_restore.rs`
(`apply`, `:62-88`).

**Änderung:** Die `upper <= page`-Abkürzung (`:67-69`) entfällt ersatzlos und
wird durch die Bedingung aus D2 ersetzt. `return true` nur bei
`ContentHeight::Known(px)` **und** `row_source == RowHeightSource::Measured`
**und** (`header_source` ist `None` oder `Some(Measured)`) **und** `px <= page`.
Jeder andere Fall fällt in den bestehenden Rumpf ab `live_row_height`, der bei
stale Allokation `false` liefert, ohne das Adjustment anzufassen. `n_sections`
kommt aus `shared.queue_sections.borrow().len()` — das Borrow wird in eine
lokale Variable gelesen und sofort fallengelassen (Reentranz-Disziplin,
Vorbild: `track_list_geometry::remember_after_layout`, `:26-37`).

**Test:** `search_16_a_result_set_that_fits_still_centers_after_clear_all` (neu,
`[gtk]`, Display, in `search_viewport_display_tests.rs`). 200 synthetische
Zeilen, 900×320-Fenster; Query, die **3** Zeilen trifft (damit das gefilterte
Ergebnis in den Viewport passt und `upper <= page` gilt);
`update_current_track(id, None, CurrentTrackChange::PlaybackStarted)` für einen
der Treffer; `SectionSearch::clear_all()`; nach `settle_for(500ms)` muss
`adjustment.value()` innerhalb einer Zeilenhöhe am zentrierten Ziel liegen —
berechnet über `reload_restore::centered_track_scroll_target`.

**Pflichtläufe dieser Aufgabe (nicht optional, nicht in den Risikoabschnitt
verschieben):** `start_restore_tests.rs` und
`fresh_start_allocation_display_tests.rs` vollständig, weil `center_loaded_track`
/ START-3 (`track_list_reload.rs:307-322`) sich `apply` teilt und D2 den
Startpfad strenger macht. Beide müssen grün bleiben; bleibt einer rot, ist
zuerst gegen die Basislinie auf `origin/dev` zu prüfen (siehe Kontrollarm) und
erst dann D2 nachzuschärfen.

**Abnahme:** Der neue Test ist gegen Aufgabe 1 **allein** rot und wird erst mit
Aufgabe 2 grün. Das ist die Kernaussage dieses Schritts und gehört ins
Protokoll. Zusätzlich: `start_restore_tests` und
`fresh_start_allocation_display_tests` unverändert grün.

### Aufgabe 3 — Der Merker „während dieser Suche gestartet"

**Dateien:** `crates/reprise-gnome/src/ui/track_list/track_list.rs` (`:159`),
`track_list_builder.rs` (`:101`), `track_list_reload.rs` (`:221`, `:239`,
`:345`, `:387`, `:443`), `current_track_selection.rs` (`update_current_track`),
`crates/reprise-gnome/src/ui/view_session.rs:196`.

**Änderung:** `PreSearch { anchor, playback_started }` als `Copy`-Struct in einer
`Cell` (D4). `update_current_track` setzt `playback_started` auf `true`, wenn
`change` ∈ {`PlaybackStarted`, `ExplicitTransport`} **und** `shared.filter`
nicht leer ist — der Filter wird in einer eigenen Anweisung gelesen und die
Borrow sofort fallengelassen (Reentranz-Disziplin des Repos, Vorbild
`track_list_reload.rs:102-104`). Kein Fokus, keine Selektion, kein Viewport wird
angefasst: **NAV-10b bleibt unberührt**, und der geplante
`track-list-selection-anchor`-Plan, der auf der heutigen Passivität dieser Datei
aufbaut, bleibt tragfähig.

**Die vollständige Liste der Fundstellen** (nachgemessen, `grep -rn
pre_search_anchor crates/` — der Entwurf nannte fünf, es sind acht Codestellen
plus zwei Kommentare):

| Datei:Zeile | Was dort steht |
|---|---|
| `track_list.rs:159` | Felddeklaration |
| `track_list_builder.rs:101` | `Cell::new(None)` bei der Konstruktion |
| `track_list_reload.rs:221` | `.is_some()` als Teil der `is_noop`-Vorbedingung |
| `track_list_reload.rs:239` | `take()` im `RestorePreSearch`-Zweig |
| `track_list_reload.rs:345` | `set(captured.anchor)` in `prepare_filter_change` |
| `track_list_reload.rs:387` | `set(None)` in `set_source_and_reload` |
| `track_list_reload.rs:443` | `.is_some()` im Hold-Filter |
| `view_session.rs:196` | `set(None)` in `prepare_track_view` |
| `track_list_reload.rs:88`, `view_session.rs:110` | nur Kommentare, mitziehen |

**Test:** `search_16_only_a_user_start_during_the_search_arms_the_centering`
(neu, `[gtk]`; ohne Display, wenn `Shared` konstruierbar ist — sonst Display).
Vier Fälle: `PlaybackStarted` bei aktiver Query → gesetzt; `AutomaticAdvance`
bei aktiver Query → nicht gesetzt; `PlaybackStarted` ohne Query → nicht gesetzt;
nach `prepare_filter_change`s neuer Erfassung → zurückgesetzt.

**Abnahme:** Der Zustand wird an genau den acht oben genannten Stellen berührt;
ein `grep` nach `pre_search` findet keine neunte.

### Aufgabe 4 — Eine Regel für alle Wege, die die Suche leeren

**Dateien:** `track_list_reload.rs` (`ReloadViewport` `:82-90`,
`filter_change_viewport` `:92-100`, `restore_reload_anchor` `:201-257`,
`reload_filter_change` `:351-355`, `reload_with_anchor_and_viewport` `:425-454`),
`track_list_filter_actions.rs` (`clear_all_restrictions`),
`track_list_reload_tests.rs`.

**Änderung:**

1. Neue Variante `ReloadViewport::CenterPlayingElsePreSearch`.
2. Neue **reine** Funktion
   `fn viewport_after_clearing(had_query: bool, started_in_search: bool) -> ReloadViewport`:
   `(true, true)` → `CenterPlayingElsePreSearch`; `(true, false)` →
   `RestorePreSearch`; `(false, _)` → `CenterPlayingTrack`.
   `filter_change_viewport` ruft sie für den Leer-Fall auf und bleibt sonst wie
   sie ist.
3. `clear_all_restrictions` liest `had_query` **vor** dem Leeren von
   `shared.filter` und ruft statt `reload_centering_playing_track` einen Reload
   mit dem Ergebnis derselben Funktion. Damit zeigen Chip-×
   (`SectionSearch::clear_active_query()`, `section_search.rs:315-324`), Escape,
   Feld-leeren, „Clear all" und „Show all N tracks" dasselbe Verhalten.
4. **Der Fall „nur Facetten, keine Query" bleibt ausdrücklich FIL-9.** Ein
   Vor-Such-Anker existiert ausschließlich nach einem Übergang leere→nicht-leere
   Query — nur dort erfasst ihn `prepare_filter_change` (`:339-347`, Zuweisung
   `:345`). Wer nur Facetten gesetzt und dann „Clear all" gedrückt hat, hat
   `had_query == false` und keinen Anker; es bleibt beim heutigen,
   bedingungslosen Zentrieren. **Belegt, dass FIL-9s eigener Pfad unberührt
   bleibt:** `track_list_builder.rs:251-259` schreibt nur `shared.browse_filter`
   und ruft `reload_centering_playing_track(&shared)`; er liest `shared.filter`
   nicht und ruft `clear_all_restrictions` nicht auf. Eine Änderung an
   `clear_all_restrictions` erreicht diesen Pfad also nicht.
5. `restore_reload_anchor` behandelt die neue Variante: erst der
   Zentrierungszweig (`:230`), bei fehlender Id Fall-through in den
   Vor-Such-Zweig (`:238-241`).
6. **Nicht übersehen:** die Hold-Auswahl in `:436-440` listet die Varianten auf,
   die einen `AdjustmentHold` bekommen. `AdjustmentHold::new` schreibt sofort den
   *aktuellen* Wert als Ziel fest (`adjustment_hold.rs:100-131`) — ein Hold über
   der Zentrierung würde sie zurückdrehen. Die neue Variante erhält daher einen
   Hold wie `RestorePreSearch`, und der Zentrierungszweig gibt ihn frei, bevor er
   `centered_scroll_restore::schedule` ruft. Dafür bekommt `AdjustmentHold` ein
   `pub(super) fn release_now(&self)`, das das vorhandene private `release`
   (`adjustment_hold.rs:267-274`) aufruft; doppeltes Freigeben ist durch
   `active.replace(false)` schon abgesichert. Beachte, dass
   `reload_with_anchor_and_viewport` den Hold anschließend noch per
   `release_after(SCROLL_ADJUSTMENT_HOLD)` (`:452`) behandelt — das muss nach
   einem `release_now` folgenlos bleiben.
   *Zweite Wahl, falls sich der Zusatz an `AdjustmentHold` als untragbar
   erweist:* der neuen Variante gar keinen Hold geben — dann verliert der seltene
   Rückfallpfad (Titel nicht in der Zielansicht) den Schutz gegen GTKs
   Nach-Allokations-Reset. Bewusst als zweite Wahl markiert, nicht als
   gleichwertige Alternative.

**Test:**

* `search_16_clearing_chooses_its_viewport_from_the_search_that_ran` (neu,
  `[gtk]`, display-frei, in `track_list_reload_tests.rs`) — die reine
  Wahrheitstabelle der neuen Funktion, **einschließlich der Zeile
  `(false, true) → CenterPlayingTrack`**, damit die FIL-9-Abgrenzung aus Punkt 4
  einen Wächter hat.
* `search_9_filter_change_decides_viewport_by_the_new_query`
  (`track_list_reload_tests.rs:33-55`) wird an die neue Signatur angepasst und
  bleibt der Wächter für die unveränderten Fälle (Top bei gesetzter/verfeinerter
  Query, `PreserveAnchor` bei Gleichheit).
* `search_16_clearing_after_a_play_centers_the_loaded_track` (neu, `[gtk]`,
  Display, `search_viewport_display_tests.rs`): 200 Zeilen, auf 1200 scrollen,
  Query mit ~20 Treffern in der Mitte, Wiedergabe eines Treffers starten,
  **Chip-×-Weg** über `SectionSearch::clear_active_query()` — Viewport am
  zentrierten Ziel.
* `search_16_clearing_without_a_play_returns_to_the_pre_search_place` (neu,
  Display): dieselbe Bühne ohne Wiedergabestart — Viewport zurück bei
  `departed_from`. Der bestehende
  `typed_search_reads_from_the_top_and_clearing_comes_back` bleibt unverändert
  grün und ist der Beweis, dass die alte Zusage nicht kaputtgeht.

**Abnahme:** Alle Leer-Wege, ein Verhalten; FIL-9s Facettenpfad
(`fil_9_filter_change_centers_playing_track_in_new_results` in
`reload_restore.rs`) unverändert grün.

### Aufgabe 5 — `RevealTrack` lässt Query und Facetten fallen (Core)

**Dateien:** `crates/reprise-core/src/browser/navigation.rs` (`:243-254`).

**Änderung:** Nach der Übernahme der Herkunfts-Place werden `state.search` auf
`String::new()` und `state.browse` auf `BrowseFilter::default()` gesetzt;
`sort`, `collection` und alles andere bleiben. `set_explicit_track_anchor`
läuft wie bisher. Routing: hat die Herkunft tatsächlich etwas eingeschränkt,
`go_new`, sonst `go_metadata_scope` (D7). Ein kurzer Kommentar hält fest, warum
diese Stelle und nicht `PlayOrigin` (PLAY-8) und nicht `restore_browser_place`
(BROWSE-2) — sonst dreht das ein späteres Refactoring zurück.

**Test (alle `[core]`, ohne Display, in `navigation.rs`s Testmodul):**

* `browse_14_revealing_a_track_drops_the_origins_query_and_facets` — Herkunft
  mit `search` und `browse`; Ziel hat beides leer, Anker/Selektion/Fokus auf der
  Id, Sortierung erhalten.
* `browse_14_the_narrowed_place_the_jump_leaves_stays_on_back` — nach dem Reveal
  liefert `Back` die Place **mit** Query zurück.
* `browse_14_a_reveal_without_restrictions_still_replaces_instead_of_pushing` —
  ungefilterte Bibliothek: kein zusätzlicher Back-Eintrag.

**Abnahme:** `cargo test -p reprise-core browser::navigation` grün; die
bestehenden `browse_*`-Tests in `navigation.rs` und `nav_history.rs:284-307`
unverändert grün.

### Aufgabe 6 — Der Sprung durch den echten Router, sichtbar gemessen

**Dateien:** Testmodul in
`crates/reprise-gnome/src/ui/window/metadata_navigation.rs:164-484` (dort stehen
bereits `MetadataNavigator`-Tests ohne PlayerController, z. B.
`fil_1c_playing_inside_a_scope_keeps_the_scope_and_its_chip` `:299-367`).

**Änderung:** kein Produktionscode — dieser Schritt ist der Beweis, dass Aufgabe
5 auf dem Weg durch `MetadataNavigator::navigate` → `nav_history::navigate_from`
→ `library_shell::route_to_place` → `restore_browser_place` ankommt.

**Test:** `browse_14_the_now_playing_link_clears_the_search_and_lands_on_the_track`
(neu, `[gtk]`, Display). Ablauf: TrackList mit ~200 Zeilen; über
`track_list.set_filter("…")` eine echte Einschränkung setzen; einen
`on_search_restored`-Rekorder registrieren (`track_list.rs:480-482`); dann
`navigator.navigate(NavigationIntent::RevealTrack { origin: Box::new(
track_list.browser_place()), track_id }, "test now playing link")` — die
Herkunft ist damit **die gerade sichtbare, gefilterte Place**, genau wie
`PlayOrigin` sie eingefroren hätte, und nicht von Hand zusammengesetzt.
Geprüft wird: `shared.filter` leer, `browse_filter` Default, der Rekorder hat
`""` erhalten (das ist die Leitung, die im echten Fenster den Header-Entry und
über `SectionSearch` den Chip räumt), Viewport am Anker der Zeile, und
`history.go_back_from(track_list.browser_place())` liefert die Place mit der
alten Query.

**Abnahme:** grün; `browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track`
(`reveal_track_display_tests.rs`) unverändert grün.

**Ehrliche Lücke, die benannt gehört:** die Aktion `win.jump-to-now-playing` und
`Ctrl+L` selbst werden hier nicht gefahren — sie entstehen in
`window_playing_source_wiring.rs:163-166` und brauchen einen `PlayerController`
mit geladenem Titel. Beide teilen sich in derselben Funktion buchstäblich
dieselbe `Rc<dyn Fn()>` wie der Titel-Link (`:144-146`, `:156-158`, `:164`), was
das Risiko klein, aber nicht null macht. Der Weg über die Tastatur gehört auf
die `RELEASING.md`-Checkliste bzw. in ein cua-e2e-Szenario — als eigener,
benannter Punkt, nicht als stillschweigende Annahme. Er taucht deshalb als
Punkt 5 in der abschließenden Verifikation wieder auf.

### Aufgabe 7 — Instrumentierte Abnahme statt Behauptung

**Dateien:** keine (Messung).

**Änderung:** Die Läufe aus Aufgabe 1/2/4 werden einmal mit
`REPRISE_SCROLL_PROBE=1` gefahren. `scroll_probe` beschriftet die Schreiber
bereits eindeutig: `centered_refinement` (`centered_scroll_restore.rs:81`),
`top_restore` (`track_list_reload.rs`), `anchor.*.apply`/`anchor.*.hold_target`
(`reload_anchor_scroll.rs:27-51`), `hold` (`adjustment_hold.rs:197`). Damit wird
belegt, *welcher* Schreiber den Endwert setzt — und nicht nur, dass der Endwert
stimmt.

**Abnahme:** Im Protokoll steht für den „Clear all"-Fall genau eine
`Reload`-Zeile im `diagnostic_trail` und ein `centered_refinement`-Schreiber
als letzter Schreiber. Zeigt die Spur stattdessen `anchor.idle.apply`, ist der
Fix nicht der, für den er gehalten wird.

### Aufgabe 8 — Das Regelwerk zieht nach

**Datei:** `docs/ux-rules.md`.

1. **SEARCH-16 neu** `[active] [gtk]`, im Abschnitt bei SEARCH-9 (`:2889`).
   Vorgeschlagener Regeltext (englisch, wie das übrige Dokument):

   > **SEARCH-16.** Emptying the query — the chip's ×, Escape, clearing the
   > entry by hand, “Show all N tracks”, and “Clear all” alike — restores the
   > pre-search anchor, unless the user started playback during that query (a
   > deliberate start or an explicit transport, not an automatic advance), in
   > which case the loaded track is centred; if that track is absent from the
   > cleared list, the pre-search anchor applies again, and if that row is gone
   > too, the top. The rule needs a pre-search anchor to have been taken, which
   > happens only on the transition from an empty to a non-empty query:
   > clearing facets alone, with no query ever typed, stays with FIL-9.

2. **SEARCH-9 revidiert** (`:2897-2898`): der letzte Satz verweist auf
   SEARCH-16 statt die alte Zusage zu wiederholen, mit datiertem
   Revisionsvermerk im Stil von FIL-1c/FIL-8 (`:1476-1479`). Die übrigen
   Aussagen (150 ms, sofortiges Leeren, Top bei gesetzter Query) bleiben
   wortgleich — **es wird keine verwaiste Regel zurückgelassen, die die alte
   Rückkehr fordert.**
3. **FIL-9 ergänzt** (`:1602-1609`): ein Satz zieht die Grenze — „Clear all"
   mit aktiver Query folgt SEARCH-16, ohne Query bleibt es FIL-9.
4. **BROWSE-14 neu** `[active] [core]`: `RevealTrack` behält Sammlung und
   Sortierung der Herkunft und lässt deren Textsuche und Facetten fallen, auch
   wenn der Titel sie überlebt hätte; die verlassene, eingeschränkte Place
   wandert unverändert in die Zurück-Historie. Drills in Album/Interpret/Genre
   tragen die Query weiter (SEARCH-8a).
5. **SEARCH-8a ergänzt** (`:2867-2888`): ein Halbsatz, der `RevealTrack` von
   „metadata drills carry the query" ausnimmt und auf BROWSE-14 zeigt.

**Abnahme:** `scripts/check-ux-traceability.sh` grün — SEARCH-16 hat
`search_16_*`-Tests, BROWSE-14 hat `browse_14_*`-Tests, keine unbekannte oder
ersetzte ID wird referenziert. `scripts/check-display-tests.sh --rule-named`
kennt die neuen Display-Tests.

---

## Kontrollarm — jeder Test muss vorher rot gewesen sein

Nicht optional, nicht „der Test ist neu, also deckt er ab". Ausführbar, in
dieser Reihenfolge, **je Aufgabe getrennt**:

```bash
cd <worktree>
# 1. Tests schreiben, Produktionsänderung dieser Aufgabe herausnehmen
git diff -- <produktionsdateien der aufgabe> > "$SCRATCH/fix.patch"
git apply -R "$SCRATCH/fix.patch"
git diff --stat            # muss die Testdateien zeigen, die Produktionsdateien nicht

# 2. Roten Lauf belegen
R=$(mktemp -d); mkdir -p "$R"/{config,data,cache,state}
env XDG_CONFIG_HOME=$R/config XDG_DATA_HOME=$R/data XDG_CACHE_HOME=$R/cache \
    XDG_STATE_HOME=$R/state GDK_BACKEND=x11 WAYLAND_DISPLAY= GSK_RENDERER=cairo \
    REPRISE_AUDIO_SINK=fakesink TMPDIR=/tmp \
    dbus-run-session -- xvfb-run -a cargo test -p reprise-gnome --bins -- \
      --ignored --exact --nocapture <voller::modul::pfad::testname> \
  > "$SCRATCH/red-<name>.log" 2>&1
grep -E '^test result' "$SCRATCH/red-<name>.log"

# 3. Fix zurückspielen, grünen Lauf belegen
git apply "$SCRATCH/fix.patch"
… derselbe Lauf … > "$SCRATCH/green-<name>.log" 2>&1
grep -E '^test result' "$SCRATCH/green-<name>.log"
```

Fallen, die hier schon Läufe entwertet haben:

* Als Fehlersignal zählt **ausschließlich** eine Zeile `^test result: FAILED`.
  Ein `ok` mit `0 passed` heißt: `--exact` hat nichts gefunden. **Die Anzahl
  mitlesen**, jedes Mal.
* Ohne den vollen Modulpfad (`ui::track_list::…`) trifft `--exact` nichts.
* `cargo test --exact` ohne `--` läuft ins Leere.
* In `reprise-gnome` gibt es kein `--lib`-Target; `--bins` bzw. `--bin reprise`.
* Display-Tests sind `#[ignore = "requires a display; run via xvfb-run"]` und
  laufen nur mit `--ignored`.
* Für die `[core]`-Tests aus Aufgabe 5 reicht
  `cargo test -p reprise-core browser::navigation` ohne Display.
* Nach jedem Display-Lauf `xvfb-orphan-gc --apply`.

**Mutationsprobe für Aufgabe 2** (die Aufgabe, deren Notwendigkeit am
leichtesten bestritten wird): Aufgabe 1 anwenden, Aufgabe 2 **nicht**, und
`search_16_a_result_set_that_fits_still_centers_after_clear_all` fahren. Bleibt
er rot, ist A2 als eigene Wurzel belegt. Wird er grün, ist die A2-Analyse falsch
und der Plan gehört an dieser Stelle korrigiert, nicht durchgezogen.

**Basislinie zuerst.** Die Display-Suite trägt auf `dev` bekannte Rotfärbung.
Vor dem ersten eigenen Lauf einmal dieselben Tests auf `origin/dev`
(`57ff0bfc74`) fahren und die Ergebnisse notieren; sonst wird fremdes Rot diesem
Branch zugeschrieben. Ein roter Test in einer Datei, die dieser Branch nicht
anfasst, ist kein Befund dieses Branches. Das gilt insbesondere für
`start_restore_tests.rs` und `fresh_start_allocation_display_tests.rs`, die
Aufgabe 2 als Pflichtläufe fährt: ohne Basislinie ist dort kein Urteil möglich.

---

## Risiken und was schiefgehen kann

**Die Basis hat sich seit dem Entwurf bewegt, und zwar genau im Zielgebiet.**
`#479` (`29b2edff4c`, „ListLayout stops representing a state it can never be in")
hat `list_geometry.rs`, `list_geometry_layout.rs`, das neue
`list_geometry_content.rs`, `reload_anchor_scroll.rs`, `reload_restore.rs` und
`track_list_geometry.rs` umgebaut — `ListLayout::new` heißt jetzt
`ListLayout::sectioned` und gibt kein `Option` mehr zurück, `max_scroll` liefert
direkt einen `f64`. Aufgabe 2 setzt auf `ListGeometry::content_height` auf, die
in diesem Umbau ihre heutige Form bekommen hat. Die in D2 zitierte Signatur ist
gegen `57ff0bfc74` nachgemessen; sie ist trotzdem der erste Punkt, der vor dem
Bauen erneut zu prüfen ist. `#479` brachte außerdem
`docs/plans/list-geometry-invariants.md` mit — vor Aufgabe 2 lesen, damit dieser
Plan keine Invariante bricht, die dort gerade erst festgeschrieben wurde.

**`had_query` und „ein Vor-Such-Anker existiert" sind nicht dasselbe.**
Die neue Leer-Regel entscheidet über `had_query` (Aufgabe 4, Punkt 3), die
Nutzerentscheidung ist über die Existenz des Ankers formuliert. In der einen
Richtung deckt sich das: ohne Query wurde nie ein Anker erfasst
(`prepare_filter_change`, `:345`), also greift FIL-9 — das ist der Fall, um den
es geht. Die Gegenrichtung gilt **nicht**: eine Query kann aus einem
Session-Restore oder einer Back-Navigation kommen, während `prepare_track_view`
den Anker kurz zuvor genullt hat (`view_session.rs:196`). Dann ist
`had_query == true` und es gibt trotzdem keinen Anker. Genau dafür existiert
D5s Rückfallkette (Zentrierung → Vor-Such-Anker → oben); der Regeltext von
SEARCH-16 nennt sie ausdrücklich. Wer die Bedingung stattdessen auf
`pre_search.anchor.is_some()` umbaut, ändert dieses Verhalten still — das wäre
eine eigene Entscheidung und keine Vereinfachung.

**Andere Aufrufer von `clear_all_restrictions`** (`section_search_wiring.rs:66`
Tracks, `:88` Missing, plus zwei Tests): Missing ruft zusätzlich
`set_missing_search_query("")`. Aufgabe 4 ändert dort die Viewport-Wahl —
für Missing ist der Viewport funktionslos, weil die Zeilen aus
`MissingFilesView` kommen und `shared.model` leer bleibt (`track_list_reload.rs`
`run_query`, Zweig `ViewSource::Missing`). Kein Schaden, aber der Test für
Aufgabe 1 fährt den Missing-Scope einmal mit, damit die Query dort nicht
stehenbleibt.

**Andere Aufrufer von `set_filter`:** vollständig oben in D1 aufgezählt. Das
Risiko ist ein Aufrufer, der `set_filter(x)` als „lade neu" missbraucht — es
gibt keinen. Fände sich beim Bauen doch einer, kippt D1 und der Fix wandert nach
`SectionSearch::clear_all` mit einem separaten `commit`-Aufruf.

**Andere Aufrufer von `reload_centering_playing_track`:**
`track_list_builder.rs:258` (`browse_bar.set_on_changed` — der FIL-9-Facettenpfad)
und `track_list_filter_actions.rs:23`, plus zwei Tests in
`current_track_selection_tests.rs`. Nur der zweite ändert sich. Der
Facettenpfad **profitiert** von Aufgabe 2 (auch er scheiterte bisher still,
wenn das vorherige Ergebnis in den Viewport passte) — das ist eine
Verhaltensänderung an einer Stelle, die dieser Plan nicht adressiert, und
gehört als solche in den Abnahmebericht, nicht in eine Fußnote.

**Andere Aufrufer von `RevealTrack`:** `window_action_wiring.rs:195` (My Stats,
frische Library-Place ohne Query — für den ändert sich nichts, außer dass die
Facetten-Nullung nun explizit statt zufällig ist) und
`window_playing_source_wiring.rs:75`. Dazu drei Core-Tests. `go_new` statt
`Replace` betrifft nur den Fall „es wurde etwas weggeworfen".

**`center_loaded_track` / START-3** (`track_list_reload.rs:307-322`) teilt sich
`centered_scroll_restore::apply`. D2 macht den Startpfad strenger: liefert
`content_height` beim Start `Assumed` oder `Unknown`, wird nicht mehr früh
„fertig" gemeldet, sondern nachgebessert. Das ist die gewünschte Richtung, aber
es ist die Stelle, an der dieser Plan am ehesten fremde Tests kippt. Deshalb
sind `start_restore_tests.rs` und `fresh_start_allocation_display_tests.rs`
Pflichtläufe **in Aufgabe 2** und nicht bloß Erwähnungen hier.

**Der `AdjustmentHold` ist ein scharfes Werkzeug.** Sein Modulkommentar
(`adjustment_hold.rs:12-28`) beschreibt einen Fall, in dem zwei Holds die App
mit 100 % CPU eingefroren haben. Aufgabe 4 gibt einen Hold vorzeitig frei,
statt einen zweiten zu erzeugen — bewusst, aus genau diesem Grund. Wer die
Alternative wählt (`centered_scroll_restore` erzeugt seinen eigenen Hold, der
den ersten per `supersede_holds_on` verdrängt), muss die Korrekturbudget-Tests
(`:511-539`) mitdenken.

**Kollisionen mit offenen Plänen** (geprüft am 14.08.2026):

* `docs/plans/queue-section-anchor.md` (`phase: coded`) **ist inzwischen als
  #477 gelandet** — der Plan-Status ist veraltet, die Arbeit steht in
  `origin/dev`. Sein Beschluss 10 hält fest, dass das Zentrierungsmodell
  (`scroll_center` / `pending_reveal_anchor`) bewusst zeilenbasiert geblieben
  ist. Aufgabe 2 rührt dieses Modell **nicht** an; sie ändert nur, wann die
  Zentrierung sich für fertig erklärt.
* `docs/plans/navback-scroll-jump-to-top.md` (`phase: reviewed`) beschreibt
  Funktionen (`apply_scroll_anchor_if_allocated`, `schedule_scroll_restore`,
  `Shared::last_row_height`), die es **auf `origin/dev` nicht mehr gibt**. Der
  Branch `fix/navback-anchor` existiert allerdings noch — lokal, auf `origin`
  und als ausgecheckter Worktree `~/Projects/reprise-navback` (dort auf
  `32fc63ea0c`). Der Plan ist von der Entwicklung überholt, sein kritischer
  Befund über `last_row_height` betrifft keinen Code, den dieser Plan anfasst.
  Nicht als offene Baustelle behandeln, aber vor dem Bauen einmal verifizieren,
  dass niemand ihn parallel wiederbelebt.
* `docs/plans/search-popover-commit-chip.md` (`phase: planned`) fasst laut
  Plantext `section_search.rs`/`section_search_wiring.rs` an — dieselben Dateien
  wie Aufgabe 1 (dort allerdings nur Kommentare). **Nachgeprüft: es gibt weder
  einen Branch `feature/search-popover-commit-chip` (lokal noch auf `origin`)
  noch einen Worktree.** Der Plan ist nie gebaut worden; eine akute Kollision in
  `section_search.rs` besteht damit nicht. Sollte er später anlaufen, ist die
  Berührungsfläche ohnehin auf Doc-Kommentare beschränkt.
* `docs/plans/track-list-selection-anchor.md` (`phase: planned`, NAV-17)
  verlangt, dass `current_track_selection.rs` passiv bleibt. Aufgabe 3 schreibt
  dort ein `Cell` und bewegt nichts Sichtbares — verträglich, aber im Diff
  ausdrücklich zu prüfen.
* `docs/plans/jump-to-playing-source-item.md` (`phase: coded`) regelt die
  externen Modi (Podcast/YouTube/Radio) und schreibt die Reihenfolge in
  `MetadataNavigator::navigate` fest: der Reveal-Callback läuft **vor** dem
  Routing (`metadata_navigation.rs:47-57, 97-124`). Dieser Plan fasst diese
  Reihenfolge nicht an — Aufgabe 5 sitzt im Core, Aufgabe 6 fügt nur einen Test
  hinzu.
* `docs/plans/list-geometry-service.md` (`phase: grilled`, Branch
  `feat/list-geometry-service`) ist der Vorläufer des Moduls, auf dem Aufgabe 2
  aufsetzt. Vor dem Bauen prüfen, ob er noch offene Arbeit an
  `list_geometry.rs` vorsieht, die mit D2 kollidiert.

**Was der Plan nicht repariert (und was er dazu sagt):** Cover- und
Interpreten-Sprung aus der Player-Leiste behalten die Query (D8). Wer den Bug
dort erneut meldet, hat recht — die Antwort ist der Folgeplan mit `carry_query`,
keine stille Erweiterung dieses Plans.

---

## Abschließende Verifikation

Am Ende des Strangs, von einer Stelle aus, nach Aufgabe 8:

1. `reveal_track_display_tests.rs` — der Titel-Link landet weiterhin am Anker.
   Aufgabe 5 ändert den Router, Aufgabe 2 und 4 ändern den Reload; keine der
   beiden Hälften darf für sich allein behaupten, dieser Test sei in Ordnung.
2. `navback_anchor_display_tests.rs` und
   `queue_section_geometry_display_tests.rs` — Back/Forward und die sektionierte
   Queue unverändert; beide hängen an `reload_anchor_scroll`, das Aufgabe 4
   indirekt über die Hold-Freigabe berührt.
3. Ein Durchlauf des ganzen Weges mit `REPRISE_SCROLL_PROBE=1`: suchen →
   abspielen → **Clear all** → Titel-Link. Erwartet: ein `Reload` je Schritt,
   `centered_refinement` als letzter Schreiber nach „Clear all", ein
   `anchor.*.apply` nach dem Titel-Link, **kein** `hold`-Schreiber, der danach
   noch korrigiert.
4. `scripts/check-ux-traceability.sh` und
   `scripts/check-display-tests.sh --rule-named` — erst hier aussagekräftig,
   weil die Regeln aus Aufgabe 8 und die Tests aus den Aufgaben 1–6 zusammen
   vorliegen müssen.
5. Der manuelle Griff, den kein Test fährt: `Ctrl+L` im laufenden Fenster bei
   aktiver Suche (siehe die benannte Lücke in Aufgabe 6). Ergebnis ins
   Protokoll, mit oder ohne Befund.

Dazu die Läufe, die schon in den Aufgaben stehen und hier nur nicht vergessen
werden dürfen: `fil_9_filter_change_centers_playing_track_in_new_results`,
`typed_search_reads_from_the_top_and_clearing_comes_back`,
`clear_all_restrictions_resets_search_and_browse_in_one_pass`,
`start_restore_tests`, `fresh_start_allocation_display_tests`,
`glide_reload_display_tests`.

---

## Nachfolgeaufgabe

**`reveal-intents-carry-query-flag`** — eigener Plan, nicht Teil dieses hier.

Auslöser: D8. Cover-Klick (`OpenAlbum`) und Interpretenzeile (`OpenArtist`) der
Player-Leiste nehmen die Suchquery weiterhin mit, weil derselbe Intent auch von
Zeilen-Drills gesendet wird und `metadata_target_state` (`navigation.rs:300-314`)
die Query dort bewusst trägt (SEARCH-8a).

Inhalt des Folgeplans:

* ein Feld `carry_query: bool` (oder ein gleichwertiger, benannter Typ) an
  `NavigationIntent::OpenAlbum` / `OpenArtist`, gesetzt von den Sendern;
* eine SEARCH-8a-Revision, die zwischen „Drill aus einer Zeile" (Query trägt
  weiter) und „Sprung aus der Player-Leiste" (Query fällt) unterscheidet, mit
  datiertem Revisionsvermerk statt umgewidmeter ID;
* Tests analog zu `browse_14_*`, aber für die beiden Metadaten-Sprünge.

Der Plan ist erst sinnvoll, wenn BROWSE-14 aus diesem Plan steht — sonst hätte
die Revision keinen Gegenpol, auf den sie zeigen kann.

---

## Parallelität

**Nicht schneidbar, und deshalb nicht geschnitten.** Der Entwurf schlug zwei
Stränge vor (A: Aufgaben 1–4, 7, 8; B: Aufgaben 5, 6). Dieser Schnitt ist
verworfen. Die Gründe, nachgeprüft und nicht bloß befürchtet:

1. **Die Aufgaben 1–4 fassen alle dieselben zwei Dinge an.** Alle vier arbeiten
   in `track_list_reload.rs` (Aufgabe 1 `:329-334`, Aufgabe 3 `:221/:239/:345/
   :387/:443`, Aufgabe 4 `:82-100/:201-257/:425-454`; Aufgabe 2 ist über
   `centered_scroll_restore::apply` an denselben Reload-Zyklus gebunden), und
   drei von ihnen teilen sich den Zustand `pre_search`. Ein Schnitt mitten
   hindurch erzeugt Konflikte in genau der Datei, in der die Entscheidung lebt.
2. **Aufgabe 5 wäre allein lauffähig, aber nicht allein abnehmbar.** Sie sitzt
   im Core und hat drei display-freie Tests; die laufen ohne den Rest. Im echten
   Fenster ist sie ohne Aufgabe 1 jedoch nicht korrekt — Nebenbefund A3: das
   Zurückschreiben der leeren Query in den Header-Entry erzeugt denselben
   zweiten Reload, diesmal gegen den Reveal-Anker. Ein Strang, der nur „meine
   Core-Tests sind grün" melden darf, aber nicht „der Sprung funktioniert",
   liefert keine abnehmbare Teilmenge.
3. **`docs/ux-rules.md` hat nur einen Besitzer.** SEARCH-16 (aus Aufgabe 4) und
   BROWSE-14 (aus Aufgabe 5) landen beide in dieser Datei, dazu die Revisionen
   an SEARCH-9, FIL-9 und SEARCH-8a. Bei zwei Strängen müsste ein Strang dem
   anderen den Regeltext zuliefern, ohne ihn zu schreiben — Zusatzaufwand und
   eine Übergabestelle für einen Gewinn von zwei Aufgaben.

Der Nutzen wäre also gering, die Kosten real. **Ein Strang, Aufgaben 1 bis 8 in
dieser Reihenfolge.** Das ist ein Befund, kein Versäumnis: nicht jeder Plan
zerfällt in unabhängige Teile, und ein erzwungener Schnitt kostet hier mehr, als
er einspart.

Was innerhalb des einen Strangs trotzdem parallelisierbar bleibt, wenn zwei
Hände zur Verfügung stehen: Aufgabe 5 (Core, `navigation.rs`) berührt keine
Datei der Aufgaben 1–4 und kann vorgezogen oder nebenher gebaut werden — ihre
Abnahme im Fenster (Aufgabe 6) muss aber hinter Aufgabe 1 stehen.
