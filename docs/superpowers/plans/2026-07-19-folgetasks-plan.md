# Folgeaufgaben — Planung (2026-07-19)

> **P0 ist erledigt und dieser Abschnitt überholt (Stand 2026-07-19, 14:50).**
> Gemessen auf `a296b655`: **210/210 Display-Tests grün.** Von den neun unten
> genannten waren sieben zwischenzeitlich grün geworden, und die aktuelle
> Messung fand zehn andere Rote, die hier nicht stehen. Die Liste unten ist in
> **beide Richtungen** veraltet — nicht danach arbeiten.
>
> Belegte Diagnose und Ergebnis:
> [`2026-07-19-display-tests-diagnose.md`](2026-07-19-display-tests-diagnose.md).
> Kurz: **ein** echter Produktionsdefekt (Glass-Insets zielten auf den
> auto-eingefügten `GtkViewport` statt auf den Inhalt — Sidebar beschnitten,
> „My Stats" bei aufgeklappter Suche unerreichbar), der Rest Testfehler.
> Dazu drei Folgefixes: Escape kollabierte die Suchleiste nie (Ctrl+F sperrte
> einen im Suchfeld ein), das Compact-Zeilentoken band nie, und der
> Insets-Applier verlor seine Polsterung bei einem Kindtausch.
>
> **P1 und P2 unten sind weiterhin offen** und wurden nicht angefasst.
>
> Die Lehre aus dem Fail-Fast-Runner (letzter Absatz von P0) ist umgesetzt:
> `RELEASING.md` trägt jetzt eine ausführbare Rerun-Rezeptur, und **beide**
> Runner sind fail-closed — der Rust-Display-Runner und, seit `ffc48265`, auch
> der cua-Keyboard-Runner. Letzterer starb bis dahin bei der ersten roten
> Oberfläche, wodurch acht von zwölf Szenarien nie ein einziges Mal gelaufen
> waren.

## Der bestimmende Befund (historisch, überholt)

`4703ee79` („report every display test instead of failing fast") hat sichtbar
gemacht, was vorher hinter dem Abbruch beim ersten Fehler lag: **157 von 166
Display-Tests grün, 9 rot.** Sie scheitern auch einzeln und schon auf
`9d601ac5`, also vor der parallelen Arbeit — sie stammen aus Batch A/B und
waren nie grün.

Das heißt: neun Regeln stehen auf `[aktiv]`, ohne dass ihre Display-Zusicherung
hält. Nach der Statusdefinition im Regelwerk („`[aktiv]` = einklagbar, ein
regelbenannter Test ist grün und Merge-Blocker") ist das ein Widerspruch, der
Vorrang vor neuen Features hat.

## P0 · Die neun roten Display-Tests

Gruppiert nach vermuteter gemeinsamer Ursache — vor der Umsetzung je Gruppe
**erst reproduzieren und die Ursache belegen**, nicht raten.

**Gruppe A — Suchleisten-Chrome (3)**
- `library_chrome::search_3_lens_checked_when_active`
  (`library_chrome.rs:301`, `assert!(chrome.search_toggle.is_active())`)
- `library_chrome::search_6_hidden_query_survives_as_chip`
  (`library_chrome.rs:333`, `assert_eq!` links≠rechts)
- `shortcuts::search_2_ctrl_f_reveals_and_focuses`
- `library_chrome::library_view_title_switches_between_source_title_and_view_switcher`

Vermutung: NPP-11 hat das Title-Widget auf einen `AdwViewSwitcher` umgestellt;
die Chrome-Tests prüfen noch den Zustand davor. Der vierte Test benennt den
Umbau sogar. Zu klären: Ist der Lupen-Toggle-Zustand nach dem Umbau real
kaputt (SEARCH-3 wäre dann ein echter Nutzerfehler), oder prüfen die Tests eine
veraltete Struktur?

**Gruppe B — Lyrics-Zentrierung (2)**
- `lyrics_view_tests::active_lines_center_and_clamp_in_a_mapped_panel`
  (`:265`, `middle > 0.0 && middle < maximum`)
- `lyrics_view_tests::lyr_4_start_of_song_is_not_centered`
  (`:296`, `(line_viewport_top_offset(0) - 18.0).abs() < 2.0`)

Beide messen Geometrie im gemappten Panel. LYR-4 (Klemmung am Songanfang) kam
in Batch A; der zweite Test ist älter. Verdacht: Die Klemmung verschiebt den
Nullpunkt, gegen den der ältere Test misst — dann ist eine der beiden
Erwartungen falsch, nicht der Code.

**Gruppe C — Einzelfälle (3)**
- `now_playing::surface::npp_10_track_change_uses_one_shared_crossfade`
- `current_track_selection::nav_10_playback_marker_does_not_move_selection_or_viewport`
- `sidebar_issue_cleanup::issue_rows_install_context_gestures_and_missing_cleanup_falls_back`

NAV-10 ist neu aus Batch B, NPP-10 wurde laut Codex „konservativ mit LYR-4
versöhnt" — beide sind erste Kandidaten für echte Fehler statt veralteter
Tests.

**Vorgehen je Test:** einzeln unter `xvfb-run -a dbus-run-session` laufen
lassen, Ursache belegen, dann entscheiden — Code falsch (Feature reparieren)
oder Test falsch (Erwartung korrigieren, Begründung in den Commit). Kein
Test wird grün gemacht, indem die Erwartung abgeschwächt wird, ohne dass die
Begründung im Commit steht.

**Zusätzlich:** Die Lehre aus dem Fail-Fast selbst gehört ins Regelwerk oder in
`RELEASING.md` — ein Runner, der beim ersten Fehler abbricht, meldet „ein
Fehler", nicht „ein Fehler von neun". Das hat eine Nacht lang eine falsche
Sicherheit erzeugt.

## P1 · Verifikation, die noch aussteht

- **NR-8 visuell**: Leerzustand nach dem Einschalten, Retry-Zustand bei
  Offline, kein Badge-Punkt im Erstzustand. Der Unit-Test ist grün; die drei
  Zustände sind bisher nur behauptet. Fake-Einträge in der DB entfernen, Modul
  aus- und wieder einschalten, headless mitschneiden.
- **EQ-Balken in Bewegung**: Screenshots zeigen keine Animation. Ob die Balken
  laufen, ist unbelegt — entweder per E2E-Harness mit zwei Aufnahmen im
  Abstand, oder als `[manuell]`-Punkt in `RELEASING.md`.

## P2 · Neue Features (erst nach P0)

- **Audio-Visualizer (21b)** — nicht gebaut, kein Modul, keine VIS-Regeln.
  Braucht einen Tap auf die GStreamer-Pipeline; die vorhandene Optik
  (Waveform aus Peaks, EQ-Balken, Cover-Glow) sind Zustandsanzeigen, keine
  Signalreaktion. Eigenes Grilling nötig: Datenquelle, Kosten bei
  Dauerbetrieb, Verhalten bei `gtk-enable-animations=false`.
- **Artist News in der Artist-Detailansicht (22a)** — spezifiziert, offen.
- **LYR-1 lokale Songtexte** (LRC aus Tags + `.lrc`-Sidecar) — bewusst
  vertagt; solange nicht gebaut, bleibt der Lyrics-Tab netz-only, und die
  Zusage „eingebettete Songtexte immer" darf nirgends in der UI stehen.
- **MSRV / Rust-Edition** — GitHub-Issue #8.

## Offene Entscheidungen

- ~~**Push von `main`**~~ — erledigt 2026-07-19. `main` steht auf `a296b655`,
  lokal und `origin` deckungsgleich (gegen `ls-remote` geprüft, nicht nur gegen
  den Tracking-Ref).
- **Vier ungemergte Branches**: ~~`feat/album-view-improvements`~~ (erledigt:
  die Feature-Arbeit lag längst über `9bf482b1` auf `main`, der Worktree war
  ein veralteter Zwilling; nur Doku und Automation blieben übrig und sind
  gemergt), `feat/minor-improvements`, `feat/player-bar-blur`,
  `feat/tag-rework`. Die drei übrigen sind weiter unklar — bei
  `feat/tag-rework` lief am 19.07. noch ein Codex-Prozess.
- **Parallele Arbeit an `main`**: Während des letzten Codex-Laufs kamen fremde
  Commits (`fix/grid-5-focus`, MainContext-Serialisierung, Fail-Fast-Fix) auf
  main und haben den Lauf zum Abbruch gebracht. Wenn mehrere Stränge
  gleichzeitig auf main arbeiten, braucht es eine Absprache, sonst wiederholt
  sich das.
- **`/tmp`** liegt bei 73 % mit Teststreu aus meinen Läufen.
