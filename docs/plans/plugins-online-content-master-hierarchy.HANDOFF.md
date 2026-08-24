---
slug: plugins-online-content-master-hierarchy
worktree: /home/marvin/Projects/reprise-plugins-hierarchy
branch: feat/plugins-background-activity-hierarchy
base: origin/dev @ 653568247e
phase: implemented
created: 2026-08-24
---
# Handover — Plugins: Fortschrittsanzeige, Hierarchie, kein Auto-Aufklappen

Umsetzung des dritten Entwurfs aus dem Claude-Design-Projekt
`c947ce4e-8f29-4551-93c0-0fde5e0f82de` (`Plugins Preferences.dc.html`,
Variante `1a`, plus `agent-prompt-plugins-hierarchy.md`). Der Plan ist
`plugins-online-content-master-hierarchy.md`; dort steht der Abschnitt
„Dritter Entwurf, 24.08.2026" mit allen Details und Messungen.

## Warum ein eigener Worktree

Der geteilte Haupt-Checkout `/home/marvin/Projects/reprise` wurde während
dieser Sitzung **sekundenaktuell von einer anderen Claude-Session** an genau
denselben Dateien bearbeitet (`preference_plugins.rs`, `preference_plugin_chrome.rs`,
Schreibvorgänge um 10:56; dort war bereits ein `ONLINE_STATUS_BADGE_CLASS`
aufgetaucht, also dieselbe Spec). Auf Nachfrage wurde entschieden: eigener
Worktree auf `origin/dev`. Die eine Änderung, die schon im Haupt-Checkout lag
(Strings in `strings_online_sources.rs`), wurde dort per `git checkout --`
zurückgenommen — der Haupt-Checkout ist also unberührt.

**Die andere Session arbeitet vermutlich weiter an derselben Aufgabe.** Vor
einem Merge klären, welcher Stand gilt.

## Stand

Alles liegt **uncommitted** im Worktree. 30 Pfade geändert/neu:

```
git -C /home/marvin/Projects/reprise-plugins-hierarchy status --short
```

Neue Module:

- `crates/reprise-gnome/src/ui/preferences/preference_background_bar.rs` (+ `_tests.rs`)
  — die Fußleiste: Zustandsmodell (`bar_state`), Zeilen-Widget, CSS,
  Anbindung an Cover- und Lyrics-Batch über `wire_background_bar`.
- `crates/reprise-gnome/src/ui/preferences/preference_online_master.rs`
  — der freistehende `Online content`-Schalter samt Badge, Leiste, Hinweiszeile.

Umgebaut: `preference_plugins.rs`, `preference_plugin_chrome.rs`,
`preferences.rs`, `preferences_window.rs`, `preference_online_module_effects.rs`,
`style/mod.rs`, `window/window.rs`, plus Tests und `scripts/ptr-e2e/preferences.sh`.

Regeln: `SET-11` → `SET-11a`, `SET-14a` → `SET-14b`, neu `SET-18`
(`docs/ux-rules.md`). Tests wurden entsprechend umbenannt — `SET-15`/`SET-16`
waren bereits vergeben (Location bzw. Layout), das kostet sonst die
Traceability-Stufe.

Kataloge: `po/reprise.pot` neu erzeugt, alle sieben Locales gemergt
(`--no-fuzzy-matching`), de und es vollständig übersetzt.

## Verifiziert

| Beleg | Ergebnis |
| --- | --- |
| `cargo fmt -p reprise-gnome -- --check` | grün |
| `cargo clippy -p reprise-gnome --all-targets -- -D warnings` | grün |
| `cargo test -p reprise-gnome --bin reprise` | 2031 passed, 0 failed |
| `scripts/tests/gettext-catalogs.sh` | grün |
| `scripts/check-{ux-traceability,shell,architecture,accessibility-semantics,input-parity,gnome-idioms,ai-hygiene,motion-tokens,frontend-thinness}.sh` | grün |
| `npm --prefix quality run lint:markdown` | grün |
| `PTR_E2E_PREFERENCES_ONLY=1 scripts/ptr-e2e/run.sh` | alle Prüfungen grün, echte Zeigerereignisse |
| Display-Suite (`scripts/check-display-tests.sh`) | 799 passed, 0 failed; 3 Messwerkzeuge übersprungen |
| Screenshots echte App | `artifacts/plugins-online-content/plugins-master-bracket-{on,off}.png` |
| Screenshot laufende Leiste | `artifacts/plugins-online-content/background-bar-running.png` |

## Die rote Stelle ist geschlossen — beide Diagnosen davor waren falsch

Rot war `set_18_background_activity_never_reaches_the_dialog_head`: der Test
verglich die x-Position des Dialogtitels vor und nach dem Start zweier Jobs.
Erst +43px, nach `set_width_chars(8)` in Isolation grün, im nächsten Vollauf
+73px. Gemessen statt geraten (`measure_background_bar_width_budget`, ein
Werkzeug, das die Display-Stufe an seinem `measurement:`-Grund vorbeilässt):

1. **Ein ellipsierendes `GtkLabel` verlangt 13px als Minimum, nicht seinen
   ganzen Text.** `set_width_chars(8)` hat das Minimum auf 72px *angehoben* —
   die Zeile war kein Fix, sie hat dem Dialog 59px Luft genommen. Ersatzlos
   entfernt.
2. **Der Titel wanderte wegen des Testfensters, nicht wegen der Leiste.**
   `xvfb-run` startet hier mit 640x480, `parent.set_default_size(900, 760)`
   greift ohne Fenstermanager nicht, das Elternfenster kam mit 630px hoch — und
   der Dialog erreichte seine gesetzten 760px nie. Deshalb grün allein, rot im
   Rudel und ein anderer Versatz je Lauf.

Die Assertion prüft jetzt die Regel statt der Fenstergröße: die Mindestbreite
des Dialoginhalts mit laufenden Jobs muss in die 760px passen. Gemessen 547px
(vorher 716px bei 44px Luft).

## Der Schaden, den kein Test bewachte

Beim Nachmessen fiel auf, dass die Zeile im echten Dialog „Album cover…" las.
Die Spaltenbreiten des Entwurfs (132 / 150 / 44) stammen aus einer breiteren
Zeile; hier ließen sie der Beschreibung 101px von 197px — genau die Zählung,
die den Job benennt, fiel weg. Neue, gemessene Breiten: 100 / 92 / 40, 12px
Spaltenabstand, 20px Innenabstand; der Beschreibung bleiben 211px. Dazu:

- Adwaita gibt dem Trog einer `GtkProgressBar` `min-width: 150px`; ein
  kleineres `width-request` verliert lautlos dagegen. Die Breite steht deshalb
  im Stylesheet.
- Die Beschreibung ellipsiert aus der Mitte — was überläuft, ist eine
  Übersetzung, und die Zählung am Ende ist die Hälfte, die zählt.

Bewacht von `set_18_a_running_job_keeps_the_count_it_is_reporting`. Gegenprobe:
`TRACK_WIDTH_PX` zurück auf 150 gesetzt → rot mit „leaves the description
153 px of the 197 px it needs"; zurückgenommen → grün.

Die vier SET-18-Geometrietests liegen jetzt in
`preferences_chrome_placement_tests.rs` statt in `preferences_window.rs` — das
hielt die Datei unter der 800-Zeilen-Grenze und beseitigte die doppelten
`artwork_job_row`/`lyrics_job_row`-Fixtures.

## Offen

- **Nichts ist committet.** Vorschlag:
  `git -C <worktree> add -A && git commit -m "feat(preferences): the plugins master looks like one, and every job is named"`
- **Der Haupt-Checkout hat einen eigenen Stand.** Eine andere Session hat dort
  `7a1e7aba11` („feat: implement plugins preferences online content master
  hierarchy") direkt auf das lokale `dev` gelegt — mit demselben Ziel, aber
  ohne Fußleiste, dafür mit fremder Arbeit im selben Commit (`list_geometry`,
  `settings_geometry`, Android-Visualizer). Vor einem Merge klären, welcher
  Stand gilt.
- **Android-Stufe von `check-project-quality.sh` ist im frischen Worktree rot**
  — fehlendes SDK-Platform/Lizenzen, kein Codebezug. Bekanntes Muster, siehe
  Memory `fresh-worktree-cannot-pass-the-android-gate-stage`.
- **`preference_plugins.rs` ist 738 Zeilen**, `plugins_page` rund 160 — unter
  dem 800er-Limit, aber über dem üblichen Rahmen. Eine Extraktion der
  Klammer-Konstruktion und der Connected-Gruppe wäre die nächste Aufräumarbeit.

## Bewusste Abweichungen vom Entwurf

- Der Entwurf zeigt **fünf** Plugins und schreibt „5 of 5 plugins on",
  „5 plugins paused" und „Concerts, New Releases and YouTube". Real sind es
  **sieben**. Zahlen und Namensliste werden deshalb aus der echten Modulliste
  gefüllt statt wörtlich übernommen — sonst stünde eine falsche Zahl in der
  Oberfläche.
- Die Spaltenbreiten der Job-Zeile sind die Proportionen des Entwurfs, nicht
  seine Pixel: seine Zeile war breiter als dieser Dialog (siehe oben).
- Das Chevron folgt dem Entwurf (hinterer Slot) statt der Rinne aus dem zweiten
  Entwurf; die Rinne hatte mit der zurückkehrenden Karte ihren Zweck verloren.
  Alle Schalter teilen sich weiter eine rechte Kante (`SET-14b`).
- `Connected services` kommt im Entwurf nicht vor und bleibt unverändert.
- Der Bibliotheks-Scan behält seine eigene Darstellung, bekommt aber einen
  Platz **in** der Fußleiste statt eines Overlays über dem Titel. Der Toast im
  Hauptfenster ist unangetastet.
