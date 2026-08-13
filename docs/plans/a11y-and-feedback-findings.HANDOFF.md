# Übergabe — die sechs Befunde aus den Nachtläufen (#403–#407, #411)

Stand 2026-08-11, 21:35 · alle sechs sind **gemeldet, keiner ist behoben**

## Woher sie kommen und was das wert ist

Alle sechs stammen aus dem CUA-Explorationslauf **M4b** (11 von 12 Missionen
sauber, 102 min, sechs Missionen × Seeds 11/29, Release-Build). Evidenz liegt
außerhalb des Repos und ist **nur zu lesen**:
`~/.cache/reprise-explore-evidence/2026-08-11-m4b/<mission>-seed-<n>/trajectory.jsonl`.

Das Harness, das sie gemessen hat, ist seit PR #402 auf `dev`. Wichtig für das
Vertrauen in die Zahlen: **die Zählungen sind seither einmal korrigiert worden.**
Ein Orakel meldete 194 Warte-Befunde, von denen 180 Artefakte waren. Wer eine
Zahl aus einem älteren Bericht zitiert, zitiert womöglich die alte Rechnung —
maßgeblich sind die Zahlen in den Issues und in diesem Dokument.

Zwei der sechs tragen **ausdrückliche Vorbehalte** (#407, #411). Die sind kein
Schmuck: sie benennen, was die Messung *nicht* zeigt.

---

## #403 — Seitenleistenzeilen ohne barrierefreie Aktion

**Gemessen:** `actions == []` auf `Music`, `Radio`, `Podcasts`, `YouTube`,
`Queue`, `My Stats`. Bis zu 8 Läufe über 4 Missionen, beide Seeds, allein für
`Music` 53 Vorkommen. Der am besten belegte der sechs.

**Einstieg:** `crates/reprise-gnome/src/ui/sidebar/sidebar_presentation.rs:252`
(`navigation_row`).

**Die Falle — selbst nachgesehen:** Die Zeile ist **nicht** namenlos und nicht
rollenlos. Sie wird als `ListBoxRow` mit `activatable(true)`, `focusable(true)`,
`AccessibleRole::ListItem` und gesetztem `Property::Label` gebaut. Der naheliegende
Fix („Label ergänzen") geht also ins Leere. Was fehlt, ist eine **Aktion** im
AT-SPI-Sinn — die Rolle `ListItem` trägt keine. Die Richtung ist damit eine
Entscheidung, keine Zeile Code: Rolle mit Aktionssemantik, ein echter Knopf in der
Zeile, oder die Aktion selbst bereitstellen. Erst messen, was GTK4 in dieser
Konstellation überhaupt an AT-SPI meldet, dann entscheiden.

**Nachweis danach:** `actions != []` im AT-SPI-Baum **und** eine Aktivierung, die
tatsächlich navigiert — nicht nur eine gemeldete Aktion.

---

## #404 — Spaltenkopf ohne Aktion

**Gemessen:** Die Kopfzeile (`Title Artist Album Year Length Rating`) trägt keine
barrierefreie Aktion; Sortieren per Spalte ist für Assistenztechnik unerreichbar.
2 Läufe, 1 Mission, beide Seeds, 52 Vorkommen.

**Einstieg:** `crates/reprise-gnome/src/ui/…/track_list_columns.rs:276`
(`ColumnViewColumn::builder().title(title)`), angehängt per `append_column`.

**Zusammenhang:** Dieselbe Krankheit wie #403 — GTK baut ein Widget, das optisch
und per Zeiger bedienbar ist, aber der Assistenztechnik keine Aktion anbietet.
Wer #403 löst, hat hier vermutlich das Muster schon in der Hand. **Reihenfolge
deshalb: #403 zuerst, #404 direkt hinterher.**

---

## #405 — Rating-Sterne ohne unterscheidbaren Namen

**Gemessen:** Alle Sterne einer Zeile melden denselben zugänglichen Namen (`★`
bzw. `☆`). „Setze drei Sterne" ist damit nicht adressierbar. 2 Läufe, 1 Mission,
beide Seeds.

**Einstieg:** `crates/reprise-gnome/src/ui/…/rating.rs:265` (`build_star`) — je
Stern ein `gtk4::Button` mit `Label` als Kind; der Tooltip trägt bereits den
richtigen Text (`strings::rate_n_stars(star)`, Zeile 277).

**Das macht es klein:** Der unterscheidbare Text **existiert schon** als Tooltip.
Er muss nur zusätzlich als zugänglicher Name am Knopf hängen, statt dass das
Sternzeichen aus dem Label durchschlägt. Von den sechs ist das der billigste
Fix. Achtung: `set_tooltip_text` kostet auf X11 einen Display-Rundlauf — der Name
gehört an die Barrierefreiheits-Eigenschaft, nicht an einen zweiten Tooltip.

---

## #406 — Hover ohne sichtbare Rückmeldung

**Gemessen:** 14 Vorkommen über beide Seeds von `hover-affordance-sweep`, 7 je
Lauf: Vorher/Nachher-Bild identisch.

**Warum das Issue es als Produktbefund führt:** Der Preflight **beweist** vor
jedem Aktionsbudget, dass der echte X11-Zeiger sich bewegt hat (sitzungsfreie
`get_cursor_position`, gegen `xdotool` geeicht, 3 px Toleranz); erreicht er das
Ziel nicht, bricht der Lauf laut ab statt dem Produkt die Schuld zu geben.

**Der Vorbehalt, den das Issue noch nicht hat — bitte zuerst prüfen:** Der
Preflight beweist die **Zeigerposition**, nicht dass GTK ein Crossing-Event
bekommen und den Hover-Zustand gesetzt hat. Und Hover-CSS ist vorhanden
(`ui/style/interactions.rs`, `style/buttons.rs`, `browse_bar.rs` u. a.), es ist
also nicht so, dass es keine Hover-Gestaltung gäbe. Damit stehen drei Erklärungen
nebeneinander, und die Messung trennt sie nicht:

1. GTK sieht den synthetischen Zeiger nicht als Hover (Harness-Artefakt),
2. die Hover-Änderung ist zu fein für die Bildschwelle des Orakels (Artefakt),
3. die betroffenen Steuerelemente haben tatsächlich keine (Produktbefund).

`rating.rs:229-260` ist die einzige Stelle mit eigenem `EventControllerMotion` —
die Sterne sind also gerade **nicht** der Kandidat für „nichts passiert". Erst
diese drei Fälle trennen, dann Code anfassen. Eine Handprobe an **einem** Ziel
genügt dafür.

---

## #407 — `Search all fields` / `Add filter` ohne Wirkung

**Gemessen:** Zeigeraktivierung lässt das Fenster im Vorher/Nachher unverändert —
`Search all fields` 8×, `Add filter` 6×, beide über beide Seeds von
`pointer-layout-reachability`.

**Einstieg:** `ui/window.rs:108` (`SearchEntry`, Rolle `SearchBox`, Label gesetzt)
und `ui/browse/browse_bar.rs:128` (`MenuButton "+ Add Filter"`, Label gesetzt).

**Der Vorbehalt steht schon im Issue und ist der Grund, hier nicht sofort zu
coden:** `Search all fields` sammelt in **anderen** Missionen 14
`driver-action-undelivered`-Sätze — dort meldete der Treiber die Eingabe als
nicht zugestellt. Für die Läufe hinter diesem Issue galt der Klick als zugestellt
und das Wirkungs-Orakel sah trotzdem nichts. Die Überschneidung macht das zum
einzigen der sechs, das **zuerst eine Handprobe** verdient. Denk daran: Exit-Code
0 heißt bei diesem Treiber nicht Erfolg, und die Fault-Dateien führen das Feld
als `stdout_head`, nicht `stdout`.

---

## #411 — kein Busy-Indikator bei mehrsekündiger Suche

**Gemessen:** 14 `wait`-Schritte mit ausdrücklich gesetztem `expect_status`, in
**beiden** Seeds an denselben Stellen (`state-91/96/101/106/111/116`), 2000 ms
plus 4 × 5000 ms, ohne Spinner/Fortschritt/Statustext im AT-SPI-Baum. Missionen
`large-library-stress` (6+6) und `offline-recovery` (1+1), Profil
`mixed-sources-128`.

**Einstieg:** `ui/…/track_list_builder.rs:43-86` baut Stack + ColumnView +
Overlay — **kein** Spinner, kein Fortschritt. Suche und Sortierung laufen in
`track_list_sort.rs` und `track_list_reload.rs`, ebenfalls ohne sichtbare
Rückmeldung. Als Vorbild, wie es in diesem Haus richtig aussieht:
`ui/scan/scan_progress.rs:150`.

**Vorbehalt (steht im Issue):** Belegt ist das Fehlen eines Busy-Widgets **im
AT-SPI-Baum**, nicht das optisch eingefrorene Fenster. Beides ist es wert, aber
das Zweite ist nicht beobachtet.

**Bevor hier ein Spinner hineinkommt:** #284 ist offen — „Fortschrittsbalken
erscheinen abrupt und verschieben das Layout". Ein neuer Indikator in der
Trackliste sollte nicht dieselbe Kerbe schlagen.

---

## Wie man das nachmisst, ohne sich selbst zu betrügen

- **Kein App-Fenster von Hand öffnen.** Verifikation läuft headless; grüne Tests
  beweisen keine Oberfläche. Für optische Abnahme den Screenshot-Harness nehmen.
- **Der Lauf:** `REPRISE_EXPLORE_NO_SYNC=1 REPRISE_EXPLORE_WORKTREE=<Worktree>
  REPRISE_EXPLORE_EVIDENCE=<frisch> REPRISE_EXPLORE_REPO=<Worktree>
  reprise-explore-night`. `REPRISE_EXPLORE_REPO` **nicht vergessen**, sonst
  scheitert die Reporterzeugung nach zwei Stunden Lauf. Der Bericht überschreibt
  sich pro Tag — Evidenz getrennt ablegen.
- **Missionen mit `agent: required`** (`section-search-isolation`,
  `offline-recovery`, `large-library-stress`) lassen sich **nicht** als
  `--click-probe` fahren; dafür `hover-affordance-sweep` mit demselben Profil
  `mixed-sources-128` nehmen. Profil `mixed-128` hat keine Podcasts/YouTube/Radio.
- **Harness-Suite:** `bash scripts/tests/cua-explore.sh`, Stand **447 Tests in
  19 Dateien**, Exit 0. Immer die **Dateizahl** mitprüfen — eine ausgeschlossene
  Suite sieht sonst aus wie ein grüner Lauf.
- **`dev` ist derzeit rot** an `crates/reprise-android-ffi` (`browse_surface_*_in_core_order`,
  readdir-abhängig, mal 2 mal 4 Tests). Das ist **nicht** die eigene Arbeit;
  eigene Linie: `docs/plans/dev-green-android-ffi-scan-order.md`.

## Vorschlag für die Reihenfolge

1. **#403 + #404** zusammen — dieselbe Krankheit, der beste Beleg, und die
   Antwort auf „wie meldet man in GTK4 eine Aktion an AT-SPI" trägt beide.
2. **#405** — billig, der Text existiert schon.
3. **#407** und **#406** — je **eine Handprobe zuerst**, bevor Code fällt. Bei
   beiden ist die Messung mehrdeutig, und beide Male ist der Verdacht
   „Harness-Artefakt" nicht ausgeräumt.
4. **#411** — zusammen mit #284 denken, sonst erzeugt der Fix den nächsten Befund.
