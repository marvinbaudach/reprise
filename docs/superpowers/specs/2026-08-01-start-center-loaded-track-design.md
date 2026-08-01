# Start: Bibliothek zeigen, den geladenen Song zentrieren und wie pausiert markieren

Beim Kaltstart zeigt die Playerleiste den zuletzt geladenen Titel, die
Trackliste steht aber irgendwo anders — bei 1.831 Titeln praktisch immer weit
weg von dem, um den es gerade geht. Der Start soll stattdessen vorhersagbar
sein und eine Aussage treffen: **immer die Bibliothek, darin der geladene
Titel zentriert, markiert wie ein pausierter Song.**

Man öffnet den Player, um Musik zu hören. Die zuletzt besuchte Ansicht ist
dafür überwiegend Rauschen und gelegentlich schädlich — ein vergessener
Suchtext oder eine offene Facette sieht beim Öffnen aus wie „meine Bibliothek
ist weg". Wo jemand aufgehört hat (Playlist, Queue, Podcast-Kanal), merken wir
uns deshalb bewusst nicht mehr.

## Befund

Der Zentrier-Mechanismus existiert bereits, läuft aber wirkungslos.

* `CurrentTrackChange::SessionRestore` ist in `reveal_policy`
  (`current_track_selection.rs:35`) auf `TrackRevealPolicy::Center`
  abgebildet, ausgelöst von `session_restore.rs:62`
  (`notify_restored_current_track`).
* Dieser Aufruf steht in `window_runtime_wiring.rs:640` — **vor**
  `route_to_place` (Zeile 651). Zum Zeitpunkt des Zentrierens ist das Modell
  der Zielansicht noch nicht aufgebaut; direkt danach stellt
  `restore_browser_place` den gespeicherten Scroll-Anker der letzten Sitzung
  her und besitzt den Viewport.
* `playing_track_id` wird in `update_current_track`
  (`current_track_selection.rs:238`) nur für `PlaybackStarted`,
  `AutomaticAdvance` und `ExplicitTransport` gesetzt — nicht für
  `SessionRestore`. Der geladene Track trägt beim Start also gar keinen
  Marker, obwohl NAV-10a fordert, dass jede sichtbare Instanz des
  **geladenen** Tracks ihn trägt.

Wiederverwendet wird `track_list_reload.rs:172`
`schedule_centered_scroll_restore` — Prepaint per `ColumnView::scroll_to` plus
Idle-Refinement im 16-ms-Takt. Genau die gehärtete Variante, die eine frisch
aufgebaute Liste braucht, deren Adjustment erst nach einer Allokationsrunde
brauchbare Geometrie meldet. Die einfache Idle-Schleife in
`reveal_track_position` (8 Runden, kein Prepaint) reicht dafür nicht.

## Beschlüsse

1. **Der Start geht immer in die Bibliothek.** `ViewSource::Library`,
   unabhängig davon, wo die letzte Sitzung endete.
2. **Die gemerkte Sortierung bleibt, Suche und Facetten nicht.** Sortierung
   ist eine Vorliebe, Suche und Facetten sind flüchtige Verfeinerungen.
3. **Der geladene Track wird zentriert**, sofern er in der Bibliotheksansicht
   vorkommt.
4. **Marker ja, Auswahl nein** (NAV-10a: „Markieren und Scrollen sind
   getrennt"), **Optik wie ein pausierter Song** — derselbe Marker, derselbe
   eingefrorene Equalizer wie bei einer Pause mitten in der Sitzung.

## Design

### 1. Die Startansicht ist eine reine Funktion

Neu in `session_restore.rs`:

```rust
pub(super) fn startup_place(state: &SessionState) -> BrowserPlace
```

Liefert immer `BrowserPlace::tracks(TrackCollection::Library(LibraryScope::All), …)`
mit `sort` aus `state.sort_field`/`state.sort_dir` und ansonsten
`TrackViewState::default()` — leere Suche, leere Facetten, kein Anker, keine
Selektion, `TrackFocus::Content`. Display-frei testbar.

`window_runtime_wiring.rs` benutzt sie statt `session_state.browser_place`
und `session_state.library_root`; aktuelles Place und Bibliotheks-Wurzel sind
beim Start dasselbe. Beide Felder werden weiterhin **gespeichert** (Schema
unverändert, Rückweg offen), aber beim Start nicht mehr gelesen.

### 2. Marker beim Restore

`update_current_track` behandelt `SessionRestore` künftig so:

* `playing_track_id` wird gesetzt (bisher nicht) — der Marker erscheint auf
  jeder sichtbaren Instanz des geladenen Tracks. Er überlebt den
  anschließenden Modellaufbau, weil jede Zellen-`bind` den Marker gegen
  `playing_track_id` setzt (`track_list_columns.rs:104`).
* `set_playback_paused(true)` legt `.playback-paused` auf den `ColumnView`.
  Die CSS-Regel in `eq_bars.rs` setzt darüber `animation-play-state: paused`
  — der Equalizer startet eingefroren statt zu tanzen, obwohl nichts läuft.
  Dieselbe Klasse, die eine Pause mitten in der Sitzung setzt. Die erste
  echte `Playing`-Meldung räumt sie von selbst wieder ab.
* Die Reveal-Policy für `SessionRestore` fällt auf `MarkerOnly`. Das ist
  keine Rücknahme von Beschluss 3, sondern seine Umsetzung: `SessionRestore`
  feuert, bevor die Zielansicht überhaupt existiert; den Viewport besitzt
  Sektion 3.

### 3. Zentrieren nach dem Routing

Neu in `track_list_reload.rs`, direkt neben dem Scheduler, den es benutzt:

```rust
pub(in crate::ui) fn center_loaded_track(shared: &Shared)
```

Liest `shared.playing_track_id`, holt `current_view_ids()` und plant die
zentrierte Wiederherstellung, wenn der Track darin vorkommt. Sonst passiert
nichts — die Liste startet oben. `TrackList::center_loaded_track` reicht
durch; aufgerufen wird sie in `window_runtime_wiring.rs` **nach**
`route_to_place`.

### 4. Warum kein zweiter Scroller entsteht

Der Startplace trägt `anchor: None`. `view_state_memory::restore_scroll_when_ready`
steigt bei `anchor.is_none()` sofort aus (`view_state_memory.rs:192`), und
`capture_reload_anchor` liefert für eine unberührte Liste (nichts selektiert,
Scrollwert 0) einen No-op-Anker, den `restore_reload_anchor` überspringt.
Es gibt also nachweislich genau einen Scroller.

Die einzige weitere Bewegungsquelle ist `active_content_focus.focus_later()`
am Ende von `route_to_place`, ein einzelner Idle-Callback. Unsere
Zentrierung läuft danach: Prepaint synchron, anschließend acht
Refinement-Runden à 16 ms. Der Display-Test prüft den Endzustand nach dem
Settle und würde eine Umkehrung dieser Reihenfolge rot machen.

### 5. Reihenfolgeabhängigkeit beim Marker

`restore_session_queue` ruft in `session_player.rs:111` selbst
`sync_state(PlaybackState::Stopped)`, was über `on_playback_state` →
`clear_now_playing()` den Marker löscht. Der Marker darf deshalb erst
**danach** gesetzt werden. Mit der bestehenden Reihenfolge in
`restore_runtime` stimmt das bereits — es bleibt eine stille Kopplung, die
der Display-Test festhält.

### 6. Randfälle

| Fall | Verhalten |
| --- | --- |
| Kein Track geladen (leere Session) | Bibliothek von oben, kein Marker |
| Geladener Track existiert nicht mehr in der Bibliothek | Bibliothek von oben, kein Zentrieren |
| Podcast-Episode oder Radio-Stream geladen (`QueueItem` ohne `track_id`) | Bibliothek von oben; siehe Folgeschritt |
| Liste passt komplett in den Viewport | `centered_track_scroll_target` liefert `None`, nichts scrollt |
| Bibliothek leer (frische Installation) | Keine Id-Liste, nichts scrollt, Empty-State greift wie bisher |

### 7. Regelwerk

**START-1** (`docs/ux-rules.md:1092`) wird neu formuliert:

> **START-1** [active] [gtk] — Normaler Start: immer die Bibliotheksansicht
> mit der gemerkten Sortierung, ohne Suchtext und ohne Facetten. Der geladene
> Track ist darin zentriert und markiert, sein Equalizer eingefroren wie bei
> einer Pause; Auswahl und Fokus bleiben unangetastet (NAV-10a). Kommt er in
> der Bibliothek nicht vor, startet die Liste oben. Wiedergabe pausiert auf
> dem letzten Track (Position wiederhergestellt), der Startup-Reconcile läuft
> still (Karte nur bei echter Arbeit).

**BROWSE-5** (`docs/ux-rules.md:3161`) wird amendiert: die zuletzt besuchte
Position wird **nicht** mehr wiederhergestellt. Erhalten bleiben die
strukturierte Wiedergabe-Herkunft und die gemerkte Sortierung; Ansicht,
Suchflächen, Utilities und roher Widget-Fokus überleben den Neustart nicht.

BROWSE-2 bleibt unverändert — innerhalb der Sitzung besitzt weiterhin jeder
History-Eintrag seinen Anker.

### 8. Tests

Regel-benannt gemäß `AGENTS.md`.

**Display-frei:**
* `start_1_startup_place_is_always_the_library_root` — `startup_place`
  ignoriert gespeicherte Quelle, Suche und Facetten, behält die Sortierung,
  trägt keinen Anker und keine Selektion.
* `start_1_session_restore_marks_without_moving_the_viewport` —
  `reveal_policy(SessionRestore, _)` ist `MarkerOnly`.

**Display (xvfb, Einzelprozess — die Suite ist im Rudel unzuverlässig):**
* `start_1_loaded_track_is_centered_and_marked_paused` — nach
  `SessionRestore` plus `center_loaded_track` steht der Viewport auf dem
  zentrierten Wert, die Zeile trägt den Marker, der `ColumnView` trägt
  `.playback-paused`, und nichts ist selektiert.
* `start_1_absent_loaded_track_leaves_the_list_at_the_top` — ein geladener
  Track, den die Ansicht nicht enthält, bewegt den Viewport nicht.

Der bestehende Test
`nav_10a_row_activation_marker_does_not_move_selection_or_viewport` bleibt
gültig und unverändert.

### 9. Folgeschritt (nicht in diesem Umfang)

Die sauberere Verallgemeinerung lautet: **die Startansicht folgt dem
geladenen Element, nicht der zuletzt besuchten Ansicht** — Track → Music,
Episode → Podcasts, Stream → Radio. Für Podcasts und Radio braucht das das
Aufdecken in den Quellen-Listen, das
`2026-07-31-source-list-reveal-design.md` auf einem eigenen Branch liefert.
Sobald der gelandet ist, ist die Erweiterung klein. Bis dahin landen
Episoden- und Radio-Hörer in der Bibliothek.

## Berührte Dateien

| Datei | Änderung |
| --- | --- |
| `ui/session_restore.rs` | `startup_place` + Unit-Test |
| `ui/window/window_runtime_wiring.rs` | Startplace statt gespeichertem Place; `center_loaded_track` nach dem Routing |
| `ui/track_list/current_track_selection.rs` | `SessionRestore` setzt Marker + Pausen-Klasse, Policy `MarkerOnly` |
| `ui/track_list/track_list_reload.rs` | `center_loaded_track` |
| `ui/track_list/track_list.rs` | `TrackList::center_loaded_track` |
| `ui/track_list/start_restore_tests.rs` | neu: Display-Tests (Dateigrößen-Regel) |
| `ui/track_list/mod.rs` | Testmodul registrieren |
| `docs/ux-rules.md` | START-1, BROWSE-5 |

## Risiken

* **Geometrie beim Kaltstart.** Fenstergröße, Spaltenbreiten und
  Modellinhalt landen in derselben Frame-Folge. Deshalb der
  Prepaint-plus-Refinement-Scheduler statt der einfachen Idle-Schleife.
* **Verlorene Gewohnheit.** Wer bisher in einer Playlist oder in Podcasts
  aufgehört hat, landet künftig in der Bibliothek. Bewusst so entschieden;
  Sektion 9 nimmt den unangenehmsten Teil davon zurück, sobald die
  Vorbedingung steht.
* **`window_runtime_wiring.rs` liegt bei 787 Zeilen.** Die Änderung ist
  netto kleiner als das, was sie ersetzt — die 800-Zeilen-Grenze bleibt
  gewahrt, muss aber geprüft werden.
