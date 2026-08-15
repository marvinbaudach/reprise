# Handover — Bewerbungsmappe; der Showcase-Strang ist geschlossen

**Stand:** 14.08.2026, 20:55 CEST. Diese Datei ersetzt ihre eigene 18:16-Fassung.
Die beiden Vorgänger (`…HANDOFF-2026-08-14.md`, `…-2.md`) bleiben gültig für
Hintergrund und **Zahlen (§2 der 15:55-Fassung)** — deren Showcase-Teile sind
erledigt oder hinfällig, siehe §1.

---

## 0 Was sich am 14.08.2026 geändert hat

- **`marvinbaudach/reprise` ist öffentlich.** Damit war das vorgelagerte
  README-Schaufenster überflüssig.
- **`reprise-showcase` steht auf privat** (Nutzerentscheid, ausgeführt per
  `gh repo edit --visibility private`). Die Pages-Seite
  `marvinbaudach.github.io/reprise-showcase/` ist vom Netz.
- **Reprise bekommt stattdessen eine eigene Marketing-Seite.** Die existiert
  noch nicht.
- **Der Relaunch überlebt nur lokal:** `~/Projects/reprise-showcase`,
  Branch `feat/showcase-relaunch` @ `45a0b87`, **ungepusht** — zweisprachiger
  Umbau, vier Clips, Pages-Gate, Evidence-Vertrag. Nichts davon liegt auf
  GitHub. Wer den Inhalt je wieder braucht, holt ihn dort.
- **Die Bewerbungsmappe ist entlinkt und gepusht**, siehe §2.

---

## 1 Was aus den Vorgängern hinfällig ist

| Vorgänger-Schritt | Status |
|---|---|
| Showcase-Gates (`check-showcase.sh`, `readme-evidence.sh`, `build-pages.py`) | hinfällig — das Repo ist abgeschaltet |
| Beide Sprachfassungen gegeneinander lesen | hinfällig |
| Montag die vier Clips neu aufnehmen und tauschen | hinfällig |
| `.clip .pending` aus `pages/template.html` entscheiden | hinfällig |
| Prüfen, ob die Pages-Seite ausliefert | beantwortet: sie liefert nicht mehr |
| Gegenlesen der Bewerbungsdokumente | **steht weiter offen**, siehe §3 |
| Maßstab auf den aktuellen dev ziehen | **steht weiter offen**, siehe §4 |

**Das alte Arbeits-Scratchpad ist weg.** `/tmp/claude-1000/…/d807356c-…/scratchpad`
lag auf tmpfs und existiert nicht mehr — `ARBEITSSTAND.md`, `design-system.md`
und **`measurements/M1…M5` sind verloren**. Die Zahlen stehen noch in §2 des
15:55-Handovers; jeder Beleg lässt sich aus dem Repo neu zählen. Wer eine Zahl
gegenliest, liest sie also gegen eine **Neuzählung auf `604677322e`**, nicht
gegen eine Messdatei.

---

## 1b Stand 22:10 — was seit 20:55 dazugekommen ist

- **PR #4 der Bewerbung ist gemerged.** `bewerbung/main` trägt die entlinkte
  Mappe und die neuen PDFs (4 / 5 / 3 Links). Der Branch ist gelöscht. Ein
  Review-Durchgang fand genau einen Rest: ein CSS-Kommentar zeigte noch auf den
  Showroom, nachgezogen in `77708f6`.
- **Der Showroom ist gebaut:** `reprise` PR #490, Branch `feature/showroom`,
  Worktree `~/Projects/reprise-showroom`. Vite/React/TypeScript unter
  `showroom/`, vorgerendert, drei Kapitel, Theme mit drei Zuständen
  (System/hell/dunkel, Inline-Skript vor dem ersten Frame), Farben nur aus
  `data/brand/palette.toml`. Das ist **Strang 2 von fünf** aus
  `docs/superpowers/specs/2026-08-14-showroom-relaunch-design.md`.
- **Die Zahlen im Showroom sind getippt**, in `showroom/src/data/measurements.ts`,
  gezählt auf `604677322e`. Die Fußzeile sagt das ausdrücklich. Strang 1
  (Messung in CI) ersetzt das Modul.
- **Fünf Repo-Gates laufen mit dem npm-Verzeichnis grün** (architecture,
  ai-hygiene, frontend-thinness, gnome-idioms, ux-traceability) — das war die
  offene Frage des Entwurfs.
- **Der Weg live, Stand 22:4x:** PR #490 ist **gemerged** (Auto-Merge,
  20:20:32Z, alle drei Checks grün — Quality gate 59m55s, Android-Suite 8m6s,
  Showroom-Build 21s); `showroom/` liegt auf `dev`, das 32 Commits vor `main`
  steht, `main` nichts vor `dev` (Fast-Forward). Offen ist der
  **Promotions-PR #492** `dev`→`main` und die **Pages-Quelle**.
  **Reihenfolge ist wichtig:** die Quelle erst *unmittelbar vor* dem Merge von
  #492 von `legacy` auf `workflow` umstellen —
  `gh api -X PUT /repos/marvinbaudach/reprise/pages -f build_type=workflow`.
  Vorher: `actions/deploy-pages@v4` scheitert, solange die Quelle `legacy` ist,
  der erste Push auf `main` liefe garantiert rot. Zu früh: die Lücke zwischen
  Umstellung und Deployment wäre so lang wie die Gate (≈1 h). Nach dem Merge
  baut und veröffentlicht der `Showroom`-Workflow in einem Lauf.
  Nachgeprüft am 14.08.2026: `build_type: "legacy"`, `source.branch: "main"`;
  der Token hat `repo`, die Umstellung geht ohne Browser.
- **#492 steht auf Entwurf und wartet auf grünes `dev` — nicht am Showroom.**
  Der ist überall grün. `dev` selbst fällt seit 20:23:59Z durch die Quality
  gate: #493, #495 und #496 wurden gemerged, **ohne ihre eigene Gate
  abzuwarten**, die jeweils ~40 s später rot wurde. #493 (Boden) habe ich mit
  **#494** nachgezogen. Offen ist #496:
  „test references replaced rule CONC-7 / NR-22 — re-point it".
  **Kein Umbenennen:** nur `conc_7_filter_changes_refresh_badge_dependents`
  passt sauber (reprise-gnome, NR-35 ist `[gtk]`). Die drei `nr_22_*` liegen in
  **reprise-core**, NR-22 trug `[core] [gtk]`, NR-37 trägt **nur `[gtk]`** — ein
  Umbenennen hängt Core-Zusicherungen unter eine GTK-Regel, und
  `nr_22_failed_refresh_preserves_the_previous_successful_age` prüft ein
  Verhalten, dessen Anzeige NR-37 ausdrücklich abschafft. Entscheidung gehört
  dem Autor von #496. **Beschluss vom 15.08.2026: warten, nicht mitreparieren.**
  Wenn `scripts/check-ux-traceability.sh` gegen `origin/dev` grün ist, braucht
  #492 nur einen neuen Lauf — dann Schritt 2 und 3 oben.
  `dev` bewegt sich schnell (fünf Merges zwischen 20:23 und 23:53), die Gate
  läuft ~57 min; jeder Versuch kostet also eine Stunde und erbt, was zuletzt
  kaputtging.
- **Stand 15.08.2026, `origin/dev@980af8edd5`: UX-Traceability ist erledigt,
  die Gate bleibt rot — an einer neuen Stelle.** Nachgemessen: die fünf
  `check-*.sh` (ux-traceability, frontend-thinness, architecture, ai-hygiene,
  gnome-idioms) sind lokal gegen dev alle grün. Blocker ist jetzt **#497**, das
  `scripts/tests/` in `ci-quality.sh` gezogen hat („with no exception list").
  **`scripts/tests/worktree-gc.sh` kann in der Quality gate nicht bestehen:**
  Zeile 507 baut den Löschfehler-Fall per `chmod 555`, der Job läuft aber in
  `container: archlinux:latest` und damit als **root** (Beleg: pacman ohne
  `sudo`, HOME `/github/home`). Root ignoriert das Schreibbit, `rm -rf` räumt
  alles ab, und `du -sk .../delete-failure/target` (Zeile 535) bricht ab.
  Gegenprobe: **lokal als uid 1000 grün** (exit 0, ~4 min) — root-spezifisch,
  nicht generell kaputt. Die naheliegende Reparatur (`EUID == 0` überspringen)
  ist genau die Ausnahmeliste, die #497 abschaffen wollte; ohne Ausnahme ginge
  ein read-only Mount oder ein offener Deskriptor statt `chmod`.
  Beides — #496 wie #497 — gehört den jeweiligen Autoren. #492 steht wieder auf
  Entwurf. Siehe [[scripts-tests-run-in-no-gate]].
- **Stand 15.08.2026 08:00, `origin/dev@0ea3a4e73e`: der worktree-gc-Blocker ist
  weg (#502, fremde Reparatur) — und darunter lag der eigentliche Grund.** Seit
  dem 14.08.2026 21:32 Uhr ist im ganzen Repo **kein einziger CI-Lauf mehr grün
  geworden**; die letzten sechs starben bei exakt `60m0Xs`. Das ist die
  `timeout-minutes: 60` des `quality`-Jobs, kein Test.
  Was es verdeckt hat: die Stufe `== All ignored display tests ==` fährt jeden
  ignorierten Test einzeln und **puffert die Ausgabe**. Nach
  `Running tests/gnome_conformance.rs` steht 43 Minuten lang keine Zeile im Log,
  dann kippen 500+ `== display test: … ==` in einer Sekunde heraus. Der **letzte
  grüne** Lauf (#494, 20:35–21:32) hat dieselbe Lücke — 44m55s Display-Stufe von
  56m13s Schritt, also rund eine Minute Luft. Der Gate erreicht die Stufe jetzt
  vier Minuten später (#497 zieht `scripts/tests/` mit, #502 lässt
  `worktree-gc.sh` durchlaufen statt nach 2 min umzufallen); genau diese Minute
  fehlt.
  Härtester Beleg: **#503 und #504 ändern nur Markdown und starben gleich.**
  Reparatur: **#509** hebt die Decke auf 90 und schreibt die Messung daneben.
  `DISPLAY_TEST_JOBS: 1` bleibt — die Serialisierung ist Absicht, im Rudel ist
  die Suite flaky; ein eigener paralleler Job für die Display-Suite wäre der
  strukturelle Weg und gehört in einen eigenen PR.
  **Merkregel:** bei einem scheinbar hängenden Lauf zuerst die Laufzeit gegen
  `timeout-minutes` halten, nicht das Log lesen. `gh run view --log` liefert bei
  abgebrochenen Läufen nichts — das Zip über
  `gh api /repos/…/actions/runs/<id>/logs` schon.
  Siehe [[quality-gate-killed-by-its-own-timeout]].
- **`reprise-showcase` ist gelöscht** (22:15, Nutzerentscheid „alles soll nur in
  reprise stattfinden"). Nachgeprüft: GitHub löst den Namen nicht mehr auf, die
  Pages-URL antwortet 404. **Der lokale Klon `~/Projects/reprise-showcase` ist
  jetzt die einzige Kopie** von `main` (`373a951`) und dem nie gepushten
  `feat/showcase-relaunch` (`45a0b87`) — wer das Verzeichnis aufräumt, wirft die
  Arbeit weg.
- **Offen aus dem Entwurf:** wenn die Seite live ist, kommen die Links in CV,
  Anschreiben und Steckbrief zurück und die drei Link-Schwellen gehen wieder
  hoch (CV 4→5, Steckbrief 5→6, Cortec 3→4).

---

## 1c Showroom gegen den Entwurf — was fehlt

Gefragt wurde: „das was wir designt hatten?" Die ehrliche Antwort ist *teilweise*.
Gebaut ist der Rahmen, nicht die Zulieferung.

**Deckt sich mit `docs/superpowers/specs/2026-08-14-showroom-relaunch-design.md`:**
eine Bühne statt zwei; Messprotokoll als Rahmen mit randabfallenden Bühnen als
Bruch; Farbe ausschließlich aus `data/brand/palette.toml`, Grautöne aus
`reprise_plate` in OKLCH abgeleitet; Archivo breit/fett für Überschriften,
schmal/leicht für Fließtext, Martian Mono nur für Datenzeilen; Grain über
radial ausmaskiertem Raster; die Signaturgeste (Haarlinie mit glühendem Kopf,
Kapitelmarken beim Berühren); vorgerendert; scroll-getriebene CSS-Animation
statt Scroll-Listener; `prefers-reduced-motion` schaltet alles ab; kein
Download-Block, weil es keinen Release gibt.

**Fehlt, jeweils weil die Zulieferung fehlt:**

| Aus dem Entwurf | Warum nicht |
| --- | --- |
| Abbildung A „Kern und Kanten" (elf Crates, Abhängigkeitsrichtung) | Zahlen je Crate liefert erst Strang 1 |
| Abbildung B in fünf Segmenten (Kern/GNOME/Android/Adapter/Plattform) | Gebaut mit den vier Werten, die belegt sind |
| Explorations-Bot als Kreislauf mit echten Befunden | Nur Prosa |
| Galerieband in CH.03, Oberfläche für Oberfläche | Alle sieben Screenshots sind tot (Oberflächen nach 09.08.2026 neu gebaut) |
| 2–3 Desktop-Clips | Keine Aufnahmen |
| Zahlen, die der Build misst | Getippt; die Fußzeile sagt das |
| View Transitions, hochzählende Zahlen, Parallaxe, laufender Spektrum-Balken | Nicht gebaut |

**Eine bewusste Abweichung:** CH.01 heißt „One core, four frontends." statt
„Two native apps. One core." — der Entwurfstitel wiederholte fast wörtlich die
Kopfzeile darüber. Steht als Kommentar an der Stelle im Code. **Achtung:**
`.github/workflows/pages.yml` prüft den Prerender mit `grep -q 'Two native
apps'` — der Satz steht in der Kopfzeile (`Hero.tsx:9`), nicht mehr im Kapitel;
wer die Kopfzeile umformuliert, muss das Gate mitziehen.

---

## 2 Was steht

| Repo | Branch | Stand |
|---|---|---|
| `bewerbung` | `main` | `63ef599` — synchron mit `origin/main` |
| `bewerbung` | `chore/drop-showcase-links` | `c8cf2a1` — **gepusht**, PR #4 offen |
| `reprise-showcase` | `feat/showcase-relaunch` | `45a0b87` — nur lokal, Repo privat |

**PR:** https://github.com/marvinbaudach/bewerbung/pull/4

### Was der PR tut

Fünf Verweise auf den Showcase sind **ersatzlos** raus — CV-Projektkarte,
Reprise-Erwähnung im Cortec-Anschreiben, drei Stellen im Steckbrief (Kopfzeile,
„Showroom", „Beide Sprachen"). Ersatzlos, weil der Link erst zurückkommt, wenn
die Marketing-Seite steht.

Jedes Dokument saß **exakt auf seiner Link-Schwelle**, deshalb mussten alle drei
mitwandern:

| PDF | Links vorher | Schwelle vorher | Links jetzt | Schwelle jetzt |
|---|---|---|---|---|
| `cv.pdf` / `cv-ohne-verfuegbarkeit.pdf` | 5 | 5 | 4 | 4 |
| `steckbrief-reprise.pdf` | 8 | 6 | 5 | 5 |
| `anschreiben-cortec-fullstack.pdf` | 4 | 4 | 3 | 3 |

Die drei Verträge **pinnten** die Showcase-URL vorher; jetzt **verbieten** sie
sie, und `tests/html_cv_test.sh` tut das über *alle* Dokumente der Mappe.

### Die Gates

```
bash tests/html_cv_test.sh             # HTML document contract: OK
bash tests/anschreiben_cortec_test.sh  # Cortec cover letter: OK
bash tests/steckbrief_test.sh          # Projektsteckbrief: OK
```

Dazu der Gegenprobe-Lauf, von Hand zu starten, nicht Teil der Gates. Er
verweigert den Start auf einem schmutzigen Baum und stellt jede Mutation selbst
zurück:

```
bash tests/mutation-run.sh             # Mutationslauf: jeder Fall hat sich verhalten
```

**Nach jeder Änderung an Gates, Vorlage oder Dokumenten diesen Lauf fahren.**
Er war heute rot, weil er noch den alten Bauvertrag `1 6` mutierte und sein Ziel
nicht mehr fand — genau die Sorte Fehler, die grüne Gates nicht zeigen. Er trägt
jetzt drei zusätzliche Fälle: ein wieder eingesetzter `reprise-showcase`-Link in
CV, Steckbrief und Cortec-Anschreiben muss die bewachenden Verträge rot machen.

### Nachweis vom 14.08.2026

- Drei Verträge grün; `./build.sh` grün.
- PDF-Links neu gezählt: 4 / 5 / 3, Seitenzahlen unverändert (2/1/1), kein
  `reprise-showcase` mehr in irgendeinem PDF.
- Steckbrief und CV-Seite 1 per Bild kontrolliert: nichts unten abgeschnitten,
  kein Loch im 2×2-Belegraster, Kopfzeilen sitzen ohne den weggefallenen
  rechtsbündigen Link.

---

## 3 Gegenlesen — erledigt am 14.08.2026

Schritt 7 des Plans ist durch. Der vollständige Befund hängt als Kommentar an
PR #4.

**Links:** alle sechs verbliebenen anonym mit HTTP 200 geprüft (vier
`604677322e`-Permalinks, `github.com/marvinbaudach`, LinkedIn). Beide
Showcase-URLs antworten 404 — das Entlinken war nötig, nicht optional.

**Zahlen:** jede gegen eine frische Zählung auf `604677322e`; die tragenden
zusätzlich von Hand nachgezählt statt einem Agentenbericht geglaubt (zwei
Berichte trugen je einen Fehler: ein `419` aus einem fremden Test als „Beleg",
und meine eigene erste `#[ignore]`-Zählung fand null, weil das Muster die Form
`#[ignore = "…"]` nicht traf). Bestätigt: 347'842 (327'165 Rust + 20'677
Kotlin), 45,82 % Testcode, 177'661/149'504, 5'986 + 334 Tests, 5'280 Kern-Tests
(= 5'986 − 706 `#[ignore]`), 676 Display-Tests, 21 Gates, 370 aktive UX-Regeln
und 98,92 % mit gleichnamigem Test, 6'066 Zeilen Regelwerk, 250 Dateien unter
`docs/`, 24 MCP-Werkzeuge, 11 Workflows, 6 Missionen, vier Frontends.

**Zwei Befunde:**

1. Die Spannung `45,8 % × 347'842` gegen `177'661 + 149'504` löst sich: die
   **30'922 Zeilen der zweiten Plattform sind 20'677 Kotlin plus 10'245
   `reprise-android-ffi`**, und die 45,8 % zählen die 9'884 Kotlin-Testzeilen
   mit, während die 149'504 nur Rust sind. Beides ist richtig, aber es steht
   nirgends dabei.
2. **„21 deterministische Anomalieklassen" liess sich nicht belegen** und ist
   raus (`c8cf2a1`). Keine Stelle im Repo nennt oder listet 21; je nach
   Abgrenzung zwischen Produktbefund und Prüfstandsfehler ergeben sich 18, 22
   oder 28 Codes aus den Orakeln unter `scripts/cua-explore`. Die Aussage
   blieb, die Zahl ging; der Vertrag verbietet sie, der Gegenprobe-Lauf hat
   einen Fall dafür.

**Performance-Werte:** trotz verlorener Messdateien belegt — sie stehen im
selben Commit in `docs/assets/reprise-performance.svg` (53.605 → 1.333 ms,
8.125 → 0.298 ms, 2'379'776 Bytes, +9,85 %) und in
`docs/plans/idle-frame-clock.md` (110 → 64 ms/s, 419 → 0 Tag-Lesevorgänge).
**Beschrieben, nicht erzwungen** — nur die drei Budgetwerte (acht SQL-Fenster,
1'600 Zeilen, 100'000 Titel) hält ein Test fest. Eine echte Neumessung bräuchte
den 100k-Prüfstand und ist ein eigener, schwerer Lauf.

**Eine Zahl bleibt rekonstruiert, nicht selbstbelegt:** die *9
Pointer-Abläufe* — `scripts/ptr-e2e/` enthält 10 `run_*_flow`-Funktionen, und
`artist-news.sh` fällt laut eigenem Kommentar heraus, weil es den Smoke-Pfad
statt echter Zeigereingabe fährt. Plausibel, aber die Abgrenzung steht nirgends
geschrieben. Wer die 9 anfasst, zieht dieselbe Prüfung wie bei den
Anomalieklassen.

---

## 4 Danach

1. **Maßstab auf den dann aktuellen dev ziehen** — Zahlen, CV, Anschreiben,
   Steckbrief und alle `604677322e`-Permalinks. Der Commit steht in der
   Bewerbung nur an **einer** Stelle: `src/shared/profile.js`
   (`repriseBaselineCommit`); `tests/html_cv_test.sh` erzwingt, dass der
   Steckbrief denselben nennt.
2. **Wenn die Marketing-Seite steht:** Link zurückholen. Das sind je eine Zeile
   in CV, Cortec-Anschreiben und Steckbrief plus die drei Schwellen aus §2
   wieder hoch — und die Sperre in den Verträgen auf die neue URL umstellen,
   nicht ersatzlos löschen.

---

## 5 Ein Befund, der nicht wiederkommen darf

Ein früheres Handover schrieb, die Klammer zwischen Showroom und CV sei: „der
Verlauf läuft Blau → Violett → Magenta, und das Violett bei 58 % ist `#9184d9` —
exakt der CV-Akzent." **Beides ist falsch.**

Nachgerechnet aus `crates/reprise-view/src/spectral_colour.rs@604677322e`
(OKLCH, fallende Hue, Ottosson-Matrizen aus `reprise-view/src/colour.rs`):

- Die Achse läuft **`CORAL #FF6F5E` → Magenta → Violett → Blau → `TEAL #4FDBD4`**,
  also genau andersherum.
- Bei t = 0,58 steht `#99adff`. Der dem CV-Akzent `#9184d9` nächste Punkt der
  **ganzen** Rampe ist `#a2a8ff` bei t ≈ 0,55, sRGB-Abstand 55 — sichtbar eine
  andere Farbe.

**Die echte Klammer**, die auf dem Steckbrief steht: `CORAL` und `TEAL` sind die
beiden Konstanten aus `spectral_colour.rs`. Die Farbachse auf dem Blatt ist Wert
für Wert aus der Funktion gerechnet, nicht nachempfunden; `tests/steckbrief_test.sh`
pinnt beide Enden **und verbietet die falsche Behauptung ausdrücklich**.

Zweiter Befund, für jeden künftigen Prüfstand: **eine Mutation, die nur das erste
Vorkommen ersetzt, beweist nichts.** Ein überlebendes Vorkommen hielt einen Pin
grün und liess zwei intakte Gates blind aussehen. Verwandt: wo eine Fläche eine
Zahl zweimal nennt — in der Prosa und in einer eingebetteten Abbildung — erfüllt
die Abbildung den blossen Zahlen-Pin allein. Deshalb ist dort zusätzlich der Satz
gepinnt.

---

## 6 Eine offene Kleinigkeit

**Die Medienlautstärke des Handys steht auf 0.** Für die Clip-Aufnahme
stummgeschaltet; `adb shell cmd media_session volume --stream 3 --set 7` greift
ohne aktive Audio-Route nicht. Der Nutzer stellt sie mit der Wippe wieder hoch —
nicht erneut per adb daran herumprobieren.

---

## 7 Umgebung

- **Wake-Lock `showcase-relaunch` läuft** (`wake-lock release showcase-relaunch`,
  wenn die Arbeit endet).
- **`git archive` liefert `docs/` unvollständig** (`export-ignore` auf
  `docs/plans`). Wer dort etwas nicht findet: `git show 604677322e:<pfad>`.
- **Die Planungsdokumente im Hauptcheckout sind ungetrackt** und verschwinden
  dort erfahrungsgemäss — diese Datei eingeschlossen.
- **Der Load-Governor-Hook** blockt Bash-Kommandos, die nach schweren
  Einstiegspunkten aussehen — auch reine Textsuchen mit einem Skriptnamen im
  Muster. Dann `HEAVY_RUN_DISABLE=1` voranstellen.
- **Bash-Ausgabe kappen**, sonst frisst der Kontext den Rest der Sitzung.
- **PDF per Bild kontrollieren:** `./build.sh`, dann
  `pdftoppm -png -r 110 -f 1 -l 1 out/<datei>.pdf <ziel>` und ansehen. Der
  Steckbrief lief in zwei Entwürfen unten aus dem Blatt, ohne dass ein Vertrag
  das gemerkt hätte — die Seitenzahl blieb 1, weil `overflow:hidden` den Rest
  abschneidet.
- **`text-transform` ändert die Glyphen im PDF**, nicht nur die Darstellung:
  `pdftotext` liefert Versalien. Prüfungen auf Überschriften deshalb ohne
  Rücksicht auf Gross- und Kleinschreibung — `grep -i`.
