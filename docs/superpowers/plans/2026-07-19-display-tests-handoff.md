# Handoff — die verbliebenen roten Display-Tests (2026-07-19, 08:56)

Stand `58718667` auf `main`, Arbeitsbaum sauber, nicht gepusht.
Workspace-Tests, Clippy, Traceability (105 Regeln) und Architektur sind grün.
**Display-Tests: 6 von 177 rot.**

## Wie das überhaupt sichtbar wurde

`4703ee79` („report every display test instead of failing fast") hat den Runner
umgestellt. Vorher brach er beim ersten Fehler ab — und der erste war immer
`grid_5`. Eine ganze Nacht lang wurde daraus „ein Fehler" gelesen, während neun
Zusicherungen nicht hielten. **Das ist die eigentliche Lehre:** Ein Runner, der
abbricht, meldet nicht „alles andere ist grün". Er meldet gar nichts über den
Rest.

## Bereits behoben (nicht erneut anfassen)

| Test | Ursache | Klasse |
|---|---|---|
| `search_3_lens_checked_when_active` | Lupe hing an `connect_search_changed` (entprellt, ~150 ms Verzug); jetzt `connect_changed` | **echter Fehler** |
| `search_6_hidden_query_survives_as_chip` | `GtkSearchBar` löscht sein Entry beim Verlassen des Suchmodus — Query ging verloren; wird jetzt gestasht und zurückgeschrieben | **echter Fehler** |
| `nav_10_playback_marker_…` | Vorbedingung geprüft, bevor `scroll_to` gesetzt hatte | Testfehler |
| `grid_5_reveal_scrolls_to_playing_album` | Scrollen (Codex) + Fokus (`fix/grid-5-focus`) repariert; zuletzt verdeckte die eigene Diagnose den Fix, weil zwei `wait_until` **nacheinander** liefen | beides |

## Verworfene Hypothesen — nicht nochmal durchprobieren

1. **`has_focus()` scheitert nur an fehlendem Fenstermanager unter Xvfb.**
   Falsch: `is_focus()` ist ebenfalls rot. Die Verengung wurde zurückgenommen.
2. **NPP-10 scheitert an abgeschalteten Animationen.** Falsch: Der Test ruft
   selbst `settings.set_gtk_enable_animations(true)`, und
   `motion::animations_enabled()` liest dieselben Default-Settings.
3. **LYR-4 scheitert an ungesetztem Layout.** Falsch: allokierte Höhe ist 202,
   das Layout stand.

## Die sechs offenen Tests

### 1 · `lyr_4_start_of_song_is_not_centered` — **Entscheidung nötig, kein Debugging**

Gemessen: **Top-Offset 0, erwartet ~18**, Center-Offset −91, allokierte Höhe
202. Die Klemmung setzt die erste Zeile bündig an die Viewport-Kante. Ein
18-px-Wert existiert nirgends in `lyrics_view.rs`.

Zu entscheiden: Will LYR-4 einen Abstand über der ersten Zeile (dann fehlt er
im Code), oder ist die 18 im Test eine erfundene Zahl (dann muss die Erwartung
auf 0 und die Begründung in den Commit)? **Das ist eine Design-Frage.**

### 2 · `active_lines_center_and_clamp_in_a_mapped_panel`

`assertion failed: middle > 0.0 && middle < maximum` (`lyrics_view_tests.rs:265`,
Zeilennummer vor `58718667`). Hängt mutmaßlich an derselben Nullpunkt-Frage wie
LYR-4 — erst 1 entscheiden, dann diesen prüfen.

### 3 · `search_2_ctrl_f_reveals_and_focuses`

`grab_focus()` lief, bevor der Revealer das Entry gemappt hatte; das ist
behoben (`4175da48`, jetzt `idle_add_local_once`). **Der Test bleibt trotzdem
rot** — weder `has_focus()` noch `is_focus()` wird wahr. Nächster Schritt:
prüfen, ob das Entry im Testaufbau überhaupt jemals gemappt wird
(`search_bar.set_child(&entry)` + `window.set_content(&search_bar)`), und ob
`iteration(false)` den Revealer-Tick je ausführt. Der Frame-Clock ist der
Verdacht, aber **unbelegt**.

### 4 · `npp_10_track_change_uses_one_shared_crossfade`

`panel.has_track_animation()` ist falsch, obwohl `changed` wahr sein muss
(`None` → `Some(track)`) und Animationen eingeschaltet sind. Offen: Ob
`adw::TimedAnimation` ohne Frame-Clock eines sichtbar gemappten Panels
überhaupt startet. Gleiche Verdachtsrichtung wie 3 — wenn sich das bestätigt,
sind beide eine Klasse und brauchen dieselbe Harness-Lösung.

### 5 · `library_view_title_switches_between_source_title_and_view_switcher`

`assert_eq!` liefert `left: None`, `right: Some(GtkStack)`. NPP-11 hat das
Title-Widget auf einen `AdwViewSwitcher` umgestellt; der Test benennt den Umbau
im Namen und prüft vermutlich noch die alte Struktur. Wahrscheinlich der
billigste der sechs.

### 6 · `issue_rows_install_context_gestures_and_missing_cleanup_falls_back`

Noch nicht diagnostiziert.

## Vorgehen

Je Test: einzeln laufen lassen mit
`xvfb-run -a dbus-run-session -- cargo test --locked -p reprise-gnome <name> -- --ignored`,
**Ursache belegen**, dann entscheiden — Code falsch (Feature reparieren) oder
Test falsch (Erwartung korrigieren, Begründung in den Commit). Eine Erwartung
wird nie abgeschwächt, bis sie grün wird, ohne dass im Commit steht warum.

Wo eine Assertion nur „stimmt nicht" sagt, zuerst die gemessenen Werte in die
Fehlermeldung ziehen (siehe `58718667`) — das hat LYR-4 von einer Suche in eine
Entscheidung verwandelt.

## Danach

`docs/superpowers/plans/2026-07-19-folgetasks-plan.md` hat den Rest: NR-8
visuell abnehmen, EQ-Balken in Bewegung belegen, dann die Features
(Visualizer 21b, Artist News 22a, LYR-1, MSRV #8).

## Offene Entscheidungen

- **Push von `main`** — bisher bewusst nicht gepusht.
- Vier ungemergte Fremdbranches: `feat/album-view-improvements`,
  `feat/minor-improvements`, `feat/player-bar-blur`, `feat/tag-rework`.
- **Parallele Arbeit auf `main`**: Während des letzten Codex-Laufs kamen fremde
  Commits dazwischen und haben ihn zum Abbruch gebracht. Ohne Absprache
  wiederholt sich das.
- `/tmp` bei 73 % mit Teststreu.
