# Handover — Showroom- und Bewerbungs-Relaunch, Umsetzung

**Stand:** 14.08.2026, 15:55 CEST · Freigabe für den **kompletten Bau (Schritte 1–7)**
liegt vor · Schritte 1, 2, 4 und 5 abgeschlossen, Schritt 6 zur Hälfte,
Schritt 3 begonnen.

Vorgänger: `showcase-und-bewerbung-relaunch.md` (Spezifikation, weiterhin
gültig). Die älteren Handover (`showcase-und-bewerbung-relaunch.HANDOFF.md`,
und die 14:20-Fassung dieser Datei) sind **überholt** — ihre Zahlen und
Statusangaben nicht mehr verwenden.

---

## 0 Das Wichtigste zuerst

**Maßstab ist `dev@604677322e`** (14.08. 13:34). `origin/dev` steht inzwischen
auf `8b87ae8ada`. **Beschluss des Nutzers vom 14.08. 15:15: nicht jetzt
umziehen.** Erst ganz zum Schluss, direkt vor der Freigabe, einmal alle Zahlen
gegen den dann aktuellen dev nachziehen — sonst läuft der Zahlen-Sweep zwei-
oder dreimal, während dev weiterläuft.

**Nicht pushen.** Weder Showcase noch Bewerbung. Der Nutzer hat dem Bau
zugestimmt, nicht der Veröffentlichung.

**Ein Wake-Lock ist gesetzt:** `wake-lock release showcase-relaunch`, wenn die
Arbeit endet.

**Zwei Scratchpads, beide gebraucht:**

- Arbeitsmaterial und Aufnahmekette (Sitzung vom Vormittag):
  `/tmp/claude-1000/-home-marvin-Projects-reprise/d807356c-692f-41c2-8605-e21ac20e5317/scratchpad`
  → im Folgenden `$OLD`
- Kontroll-Renders dieser Sitzung:
  `/tmp/claude-1000/-home-marvin-Projects-reprise/7f964b3b-945a-432e-9f57-c71be9fad7a0/scratchpad`

**Beschlüsse des Nutzers vom 14.08. nachmittags:**

1. Maßstab erst zum Schluss umziehen (siehe oben).
2. **Screenshots und Clips werden Montag neu gemacht.**
3. **Trotzdem jetzt schon Medien erzeugen** — „dann wissen wir schonmal wie es
   in etwa wirkt", am Ende gegen die neueste Fassung tauschen. Das war der
   letzte Auftrag und ist der offene Faden (siehe §4).

**Nichts einem Agentenbericht glauben.** Bei allen sechs Abbildungen haben die
bauenden Agenten „sauber, geprüft" gemeldet, und das Ansehen der PNGs hat
jedes Mal echte Fehler gefunden (§3).

---

## 1 Wo die Arbeitsstände liegen

| Repo | Branch | Stand |
|---|---|---|
| `reprise-showcase` | `feat/showcase-relaunch` | `27f36cd` — sauber. **Nicht gepusht.** |
| `bewerbung` | `fix/reprise-zahlen-604677322e` | `8d3a4ba` — nur `out/*.pdf` uncommitted (Build-Artefakte). **Nicht gepusht.** |
| `reprise` | Worktree `.worktrees/showcase-clips` auf `604677322e` | Release-Binary fertig, siehe §4 |

Commits im Showcase:

- `9ff1105` — sechs Abbildungen, beide READMEs, beide Gate-Skripte
- `27f36cd` — Pages-Seite: Vorlage, Sprachkataloge, Generator, `index.html`,
  `de.html`, `.nojekyll`

**Alle drei Gates sind grün** (14.08. 15:55):

```
bash scripts/check-showcase.sh          # Bilingual showcase contract passed
bash scripts/tests/readme-evidence.sh   # README evidence contract: OK
python3 scripts/build-pages.py --check  # pages are current
```

Wichtige Dateien im alten Scratchpad `$OLD`:

- `ARBEITSSTAND.md` — Zahlen, Befunde, Fallen
- `design-system.md` — **bindend** für jede Abbildung
- `pages-briefing.md` — Gestaltungsbriefing der Pages-Seite (umgesetzt)
- `measurements/M1-loc.md` … `M5-verification.md` — die Belege
- `recording/record.sh` — funktionierende Aufnahmekette, **jetzt gebraucht**
- `recording/MACHBARKEIT.md` — Urteil je Clip, Parameter, Fallen
- `svg/` — die Abbildungen samt Kontroll-Renders

---

## 2 Die gemessenen Zahlen — `dev@604677322e`

**Nie aus dem Gedächtnis weiterverwenden.** Immer von hier oder aus
`$OLD/measurements/`.

| Größe | Wert |
|---|---:|
| Rust gesamt | 327.165 |
| — Produkt | 177.661 |
| — Test | 149.504 |
| Rust-Dateien | 1.501 |
| Rust-Testfunktionen | 5.986 (davon 706 `#[ignore]`) |
| Kotlin gesamt | 20.677 in 144 Dateien |
| Android-Testfunktionen | 334 |
| **Rust + Kotlin** | **347.842** |
| — Produkt | 188.454 |
| — Test | 159.388 = **45,8 %** |

Nach Ort: GNOME 163.879 · Kern (`core`+`view`) 115.916 · Android 30.922
(FFI 10.245 + Kotlin 20.677) · Plattform/Runtime 21.756 · MCP 11.290 · CLI 4.079

| Beleg | Wert |
|---|---:|
| Aktive UX-Regeln | **370** (606 IDs, 170 ersetzt) |
| — mit gleichnamigem Rust-Test | 366 = 98,9 % |
| Merge-Gates | **21** Stufen in `scripts/check-merge-readiness.sh` |
| MCP-Tools | 24, davon 7 hinter Feature `mpris` → sonst 17 |
| `reprise-core`-Abhängigkeiten | 19, keine UI |
| Dateien unter `docs/` | 250 · `docs/plans/` 112 (15 Handover) · `docs/superpowers/` 68 |
| `docs/ux-rules.md` | 6.066 Zeilen |

Verifikationsstufen: Kern 5.280 im Standardlauf · Display 676 (eigenes Gate) ·
Pointer-E2E 9 Flows · Semantische E2E 11 Workflows · Exploration 6 Missionen ·
21 Anomalieklassen.

Performance: 53.605→1.333 µs (−97,51 %) · 8.125→298 µs (−96,33 %) ·
DB +2.379.776 Bytes (+9,85 %) · Leerlauf-CPU 110→64 ms/s · Tag-Reads 419→0 ·
deterministisch gedeckelt auf 8 SQL-Fenster / 1.600 Zeilen.

Falsch veröffentlicht war: „18 merge gates" (gibt es in keiner Zählweise),
„more than 340 active UX rules" (370), „217.8k/89.0k" (Methodikwechsel),
„5,541 Rust / 172 Android" (5.986/334), „13.1k Kotlin in 97 Dateien"
(20.677 in 144), „the production source is private" (ist öffentlich),
„1.24 s median startup" (nicht belegbar, entfernt), „135→55 ms/s" (früher
Prototyp, entfernt), Tauri als Roadmap-Ziel (existiert nicht).

---

## 3 Was in dieser Sitzung passiert ist

### Schritt 2 — die sechs Abbildungen, fertig

Alle zwölf Dateien liegen in `reprise-showcase/assets/`, `xmllint` grün, visuell
abgenommen. Die alten `reprise-architecture*.svg` und `reprise-performance*.svg`
sind entfernt — im Showcase-Repo verwies nur das Gate-Skript darauf. **Das
Reprise-Hauptrepo hat eigene Kopien unter `docs/assets/`, die bleiben.**

Befunde der visuellen Abnahme, alle behoben:

1. **`.figure mono` zerlegte jede große Zahl.** Liberation Mono setzt das
   Tausendertrennzeichen in eine volle Zeichenzelle — `5,280` las sich als
   `5 , 280`, `+9.85%` als `+9 . 85%`. Betraf A, C und E in beiden Sprachen.
   Behoben mit `class="figure sans"`; das Bildsystem verlangt für `.figure`
   keine Monospace. Kleine `.mono`-Tabellenwerte bleiben — dort trägt die
   Monospace-Ausrichtung etwas.
2. **Abbildung C war irreführend beschriftet:** „676 ignored GTK tests" /
   „ignorierte GTK-Tests" liest sich von außen als 676 übersprungene Tests.
   Jetzt „GTK tests, run by their own gate" / „GTK-Tests, eigenes Gate", und
   „nicht ignorierte Testfunktionen" → „Testfunktionen im Standardlauf".
3. Abbildung C, DE: „keinen Ton" → „kein Ton".
4. **Abbildung F:** Der Substrat-Satz klebte 20 px unter der Zahlenzeile und
   las sich als deren Fortsetzung. Jetzt rechtsbündig auf der Kopfzeile des
   Bandes — löst zugleich die tote Fläche dort auf.
5. **Abbildung F:** Die MENSCH-Box war 844 × 130 groß und trug zwei kurze
   Zeilen. Inhaltlich gefüllt.
6. Abbildung F, DE: „PLANFILE" → „PLANDATEI".
7. **DE-Abbildungen schrieben ß**, `README.de.md` durchgehend Schweizer
   Orthografie. Angeglichen; das Gate hält es jetzt fest.
8. **Numerische Entities** (`&#228;`) in vier Abbildungen → UTF-8.

### Schritt 4 — beide READMEs, fertig

`README.md` war schon geschrieben; `README.de.md` ist neu und spiegelt sie
Abschnitt für Abschnitt. Zwei Zeilenumbrüche in beiden Dateien wurden
verschoben, damit gepinnte Phrasen nicht über einen Umbruch laufen.

### Beide Gate-Skripte, neu geschrieben

- `scripts/check-showcase.sh` prüft die **Struktur**: beide Sprachen tragen
  dieselben acht Abschnitte (und genau acht), jede Abbildung erfüllt ihren
  Accessibility-Vertrag (`role`, `aria-labelledby`, `title`, `desc`) und nennt
  den Commit, auf dem ihre Zahlen gezählt wurden, kein README embeddet die
  Abbildung der anderen Sprache, Tauri/iOS/„Quellcode ist privat" bleiben
  draußen, die DE-Seite bleibt bei Schweizer Orthografie, kein SVG enthält
  numerische Entities.
- `scripts/tests/readme-evidence.sh` prüft die **Zahlen**, jede in beiden
  Schreibweisen, plus eine Sperrliste überholter Werte.
- Beide sammeln Fehler, statt beim ersten abzubrechen.

**Mit acht Mutationen geprüft** — Abbildung entfernt, Tauri zurück, ß in einer
DE-Abbildung, Abschnitt umbenannt, `aria-labelledby` zerstört, Zahl verdreht,
alte Zahl zurück, Commit-Bezug entfernt. Jede wurde rot, nach Wiederherstellung
alle grün.

### Schritt 5 — die Pages-Seite, gebaut

Nach `pages-briefing.md`. Aufbau:

```
pages/template.html        Struktur, CSS und JS, mit {{PLATZHALTER}}
pages/strings.en.json      140 Zeichenketten
pages/strings.de.json      dieselben 140, deutsch
scripts/build-pages.py     Generator; --check schlägt fehl, wenn die
                           erzeugten Dateien veraltet sind
index.html · de.html       erzeugt, im Wurzelverzeichnis (Pages liest von dort)
.nojekyll
```

**Warum Vorlage statt zwei Dateien:** Die zwei Sprachen können strukturell nicht
auseinanderlaufen. Eine fehlende Übersetzung ist ein Build-Fehler, kein still
englischer Absatz auf der deutschen Seite.

**Warum die SVGs inline liegen:** die Seite animiert sie. Sechs SVGs in einem
Dokument kollidieren auf ihren `id`s — der Generator stellt deshalb jeder `id`
ein `figA-`…`figF-` voran und schreibt jedes `url(#…)` und `aria-labelledby`
mit. Ihre sechs identischen `<style>`-Blöcke werden verworfen; diese Typografie
steht einmal im Seiten-Stylesheet unter `.fig`.

Gestaltung: dunkles Studio, editorial. Bricolage Grotesque / Instrument Sans /
JetBrains Mono. Palette und 48-px-Raster sind dieselben wie in den Abbildungen,
damit Seite und Bild eine Fläche sind. Spektralverlauf nur als Fortschritts-
leiste oben und als Kapiteltrenner. Kapitelleiste links ab 1080 px.

**Bewegung: zwei Sorten, beide unter `prefers-reduced-motion` aus.** Nichts
versteckt Inhalte ohne JavaScript — Einblendungen starten sichtbar im Markup
und werden per Skript animiert, nie per Skript eingeblendet.

**Eine dritte Bewegung wurde gebaut und wieder entfernt: die hochzählenden
Kennzahlen.** Ein Screenshot mitten in der Animation zeigte **146.567**, wo
347.842 gemessen ist. Auf einer Seite, deren ganzes Argument „jede Zahl ist
gemessen" lautet, darf keine Animation je eine falsche Zahl rendern — auch
nicht für eine Sekunde, und erst recht nicht in einem geteilten Screenshot.
**Nicht wieder einbauen.**

Zwei Befunde aus der Sichtprüfung der erzeugten Seite, beide behoben:
Kapitel 02 trug wörtlich denselben Titel wie die Abbildung darunter; die
Screenshot-Galerie brach bei 1440 px auf vier zu kleine Spalten.

### Schritt 6 — Bewerbung, zur Hälfte

Erledigt und committet (`8d3a4ba`):

- `src/shared/profile.js`: `177'661` / `149'504`, Quellkommentar auf
  `604677322e`. Bewusst die **Rust**-Zahlen, nicht die kombinierten — die
  Anschreiben sagen wörtlich „Zeilen produktiver Rust-Code".
- `src/anschreiben.html` **und** `src/bewerbungen/cortec-fullstack.html`:
  „18 Prüf-Gates und 5.541 Tests" → „21 Prüf-Gates und 5.986 Tests". Das
  zweite Dokument stand nicht im Plan und wäre sonst mit falschen Zahlen
  rausgegangen.
- `src/lebenslauf.html`: Tauri-Satz und Tauri-Plattform-Grid raus. Das Grid
  zeigt jetzt vier **gebaute** Flächen statt drei ausgegrauter Versprechen:
  GNOME · GTK4 · Android · Kotlin · MCP · 24 Tools · CLI · headless.
- Die vier Statistiken: `347'842` Zeilen Rust und Kotlin · `45,8 %` davon
  Testcode, 5'986 Tests · `1 → 4` ein Core, vier Frontends · `21` Gates.
- **Gerendert und angesehen:** `./build.sh`, `out/cv.pdf` Seite 1 stimmt.

---

## 3b Nachtrag 17:35 — die Medien liegen

**Alle vier Clips sind aufgenommen, eingebaut und committet** (`d8a1da3` im
Showcase, weiterhin **nicht gepusht**). Provisorisch, wie besprochen: Montag
werden sie gegen frische Aufnahmen getauscht.

| Clip | Länge | Format | Grösse | Was zu sehen ist |
|---|---|---|---:|---|
| A | 12 s | 1920×1080 | 0,53 MB | Bibliothek → Titel starten → Spektralleiste färbt sich, Legende sichtbar → Panel → Visual-Reiter mit Live-Analyse |
| B | 11 s | 1920×1080 | 0,49 MB | Desktop und Handy nebeneinander, Lorna Shore „Death Portrait", beide bei 0:04 / −5:03 |
| C | 12 s | 1920×1080 | 1,82 MB | nach Titel sortieren, Rad, dann ein Zug am Scrollbalken quer durch alle 100.000 Zeilen |
| D | 12 s | 1422×800 | 0,26 MB | `pointer-layout-reachability`-Mission bedient die App, **Echtzeit**, kein Zeitraffer |

Aufnahmekette und alles Wissen dazu liegen dauerhaft unter
`~/.cache/reprise-showcase-recording/` (eigenes README). Die gewärmten Profile
liegen unter `~/.cache/reprise-showcase-profile`, `~/.cache/reprise-showcase-music`
und `~/.cache/reprise-scratch/reprise-cua-explore-showcase-100k`.

**Was dabei gelernt wurde, kurz:**

1. **Reprise hat keine GSettings** — alle Schalter stehen in der SQLite-Tabelle
   `settings`. Banner lassen sich erst nach dem ersten Lauf abschalten. Genau
   deshalb zeigte der Vormittags-Testclip Banner, leere Metadaten und graue
   Leiste: das Wegwerf-Profil war jedes Mal leer.
2. **Ohne Vorwärmen keine Farbe.** Das spektrale Zentroid stammt aus der
   Bibliotheksanalyse; ohne sie ist die Positionsleiste grau.
3. **Das 100k-Profil bleibt 40–60 s schwarz**, bevor es das erste Bild malt.
   Kein Renderer-Fehler — die Fixture selbst erlaubt dafür 600 s.
4. **cua-explore ist von aussen filmbar.** Das `unshare` umschliesst nur den
   App-Prozess, nicht den X-Server, und die Displaynummer steht in
   `<scratch_root>/display`. Kein Repo-Skript musste angefasst werden.
5. **Die Mission entscheidet über den Clip.** `first-time-exploration` und
   `hover-affordance-sweep` verändern das Bild nur etwa alle 25 s — davon gibt
   es kein lebendiges Video. `pointer-layout-reachability` arbeitet durchgehend.
6. **`-vsync` gibt es in ffmpeg 9 nicht mehr.** Der Aufruf stirbt, die alte
   Datei bleibt liegen — sieht aus wie ein geglückter Lauf.

**Drei Bildunterschriften mussten mitwandern**, weil sie sonst etwas behauptet
hätten, was der Clip nicht zeigt: Clip A sagte „sechs Sekunden" (es sind zwölf),
Clip C behauptete das unbewegte Speicherbudget (richtig, aber anderswo gemessen
und im Clip nicht sichtbar), Clip B nennt jetzt die Sekundengleichheit.

**Offen aus diesem Nachtrag:** Die Medienlautstärke des Handys steht auf 0 — ich
hatte sie fürs Aufnehmen stummgeschaltet und bekomme sie per adb nicht zurück
(`cmd media_session volume --set` greift ohne aktive Route nicht). Bitte mit der
Wippe wieder hochstellen.

---

## 4 Was als Nächstes zu tun ist

### ~~Der offene Faden: Medien jetzt schon erzeugen~~ — erledigt, siehe §3b

Letzter Auftrag des Nutzers, noch nicht begonnen. Vier Clips, stumm, 6–12 s,
Endlosschleife, ≤ 2 MB, MP4 für Pages, WebP fürs README. **Jetzt mit dem
aktuellen Stand aufnehmen, Montag gegen frische tauschen.**

Die Kette funktioniert nachweislich: `$OLD/recording/record.sh`, Parameter im
Kopf der Datei, Urteil je Clip in `$OLD/recording/MACHBARKEIT.md`.

- **Release-Binary ist fertig:**
  `/home/marvin/Projects/reprise/.worktrees/showcase-clips/target/release/reprise`
  (14.08. 15:16). `REC_PROFILE=release`.
- **Echte Musik ist freigegeben** — eine Handvoll getaggter Dateien mit Cover
  aus der Bibliothek des Nutzers in ein **isoliertes** Testprofil kopieren
  (`REC_MUSIC_SRC`). Echte Bibliothek und Datenbank bleiben unangetastet;
  `record.sh` bootet ohnehin eigene XDG-Wurzeln und eigenen D-Bus.
- **Android-Gerät `59100DLCQ006SB`** ist freigegeben: App bauen, installieren,
  per `adb exec-out screenrecord` aufnehmen.

| Clip | Stand |
|---|---|
| A · Live-Spektrum Desktop | machbar, Binary und Musik stehen bereit |
| B · Desktop + Handy, derselbe Track | Desktop-Hälfte wie A; Synchronisation offen |
| C · Scrollen durch 100.000 Zeilen | `stress-100k`-Fixture in `scripts/cua-explore/fixtures.py`; „sichtbarer Maßstab" = Trackzähler oben rechts plus Scrollbalken |
| D · Der Bot bedient die App | `ffmpeg x11grab` zeichnet den echten X11-Zeiger mit. Offen: `cua-explore` startet seine eigene Xvfb im `unshare`-Namespace und verlangt einen sauberen Worktree |

**Beim Aufnehmen unbedingt beseitigen:** das DEBUG-Badge (fällt mit dem
Release-Build weg), das Onboarding-Banner („Reprise can now follow podcasts…"),
und leere Metadaten. Der Testclip vom Vormittag zeigt alle drei Probleme.

**Wenn die Clips liegen:** In `pages/template.html` die vier
`<div class="pending">`-Blöcke gegen
`<video muted loop playsinline preload="metadata" poster="…">` tauschen, die
Dateien nach `assets/` legen, `python3 scripts/build-pages.py` laufen lassen.
Ohne `autoplay` bei reduzierter Bewegung — dann steht das Standbild mit
Abspieltaste. Das WebP-Standbild aus Clip A gehört zusätzlich unter die
Badge-Zeile beider READMEs.

### Danach

1. **Das Gate deckt die Pages-Seite noch nicht ab.** `check-showcase.sh` sollte
   `python3 scripts/build-pages.py --check` aufrufen und dieselben Zahlen in
   `index.html`/`de.html` pinnen wie `readme-evidence.sh` in den READMEs.
   Danach wieder mutieren und prüfen, dass es rot wird.
2. **Schritt 6 zu Ende:** einseitiges PDF „Projektsteckbrief Reprise", eigener
   `build.sh`-Eintrag samt `pdfinfo`-Erwartungswerten (Seitenzahl,
   Mindestzahl Hyperlinks). Die Klammer Showroom-Teal ↔ CV-Indigo ist noch
   nicht sichtbar gemacht: Reprise färbt die Positionsleiste nach dem
   spektralen Zentroid, der Verlauf läuft Blau → Violett → Magenta, und das
   Violett bei 58 % ist `#9184d9` — exakt der CV-Akzent.
3. **Schritt 7 — Gegenlesen:** jede Zahl gegen `$OLD/measurements/`, jeder
   Beleglink aufgerufen, beide Sprachfassungen gegeneinander.
4. **Zum Schluss:** Maßstab auf den dann aktuellen dev ziehen — Zahlen,
   Abbildungen, beide READMEs, beide Sprachfassungen der Seite, CV, und alle
   `blob/604677322e/`-Permalinks.
5. **Erst dann Freigabe einholen und pushen.**

---

## 5 Fallen in dieser Umgebung

- **`git archive` liefert `docs/` unvollständig.** `.gitattributes` markiert
  `docs/plans` und `docs/superpowers/plans` als `export-ignore` — im
  ausgepackten `dev-tree` fehlen 160 Dateien. Wer dort etwas nicht findet, muss
  `git show 604677322e:<pfad>` nehmen. Zwei Agenten sind darüber gestolpert.
- **Der Load-Governor-Hook** blockt Bash-Kommandos, die nach schweren
  Einstiegspunkten aussehen — auch reine Textsuchen, die einen Skriptnamen als
  Muster enthalten. Dann `HEAVY_RUN_DISABLE=1` voranstellen.
- **Bash-Ausgabe kappen.** Lange Läufe nach `$SCRATCH/<name>.log`, Frage per
  `grep`/`wc`.
- **Kontrolle von Abbildungen und Seite immer per Bild.** Für die Seite:
  `chromium --headless=new --disable-gpu --hide-scrollbars --window-size=1440,22000
  --virtual-time-budget=15000 --screenshot=… file://…/index.html`, dann mit
  `magick` in Streifen schneiden und als Kontaktbogen montieren. Der Sprung per
  `#anker` taugt nicht — die Seite scrollt weich und lazy-geladene Bilder
  verschieben die Höhe, das Ergebnis war ein schwarzes Bild.
- **Pages deployt aus `main`.** Arbeit auf `feat/showcase-relaunch` wird erst
  nach dem Merge live.
- **`origin/dev` läuft weiter.** Beim Fetchen nicht versehentlich den Maßstab
  wechseln.
- **cloc dedupliziert nur innerhalb eines Aufrufs** — die Aufschlüsselung je
  Crate weicht deshalb um ±4 Zeilen (0,003 %) von der Gesamtmessung ab.
