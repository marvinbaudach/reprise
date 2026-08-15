# Handover — Showroom- und Bewerbungs-Relaunch

**Stand:** 2026-08-13 07:28 CEST · Konzept abgestimmt, **noch nichts gebaut**, Nutzer
hat die Freigabe bewusst zurückgehalten („erst besprechen").

## Auftrag

Reprise-Showcase und die Bewerbungsdokumente überarbeiten, mit diesen Zielen:

- Die Leistung hervorheben, **eine Desktop- und eine Android-App auf einem
  geteilten Rust-Kern** gebaut zu haben.
- **Designfähigkeit zeigen** — aktuelle Screenshots, gern auch kurze Videos.
- **LOC mit externer Software** neu zählen (also `cloc`, kein Eigenbau-Zähler).
- Das **Verhältnis GTK4 / Android / Kern** darstellen.
- Arbeitsweise belegen: TDD, Gates, UX-Regeln, gemessene Performance, und die
  **Bots, die Ruckler und Accessibility-Probleme selbst finden und melden**.
- **SVG-Abbildungen statt langer Texte** — Leser überfliegen in Sekunden.

Das Showcase darf komplett neu gedacht werden. Verwendungszweck: Bewerbungen.

## Betroffene Repositories

| Repo | Pfad | Rolle |
|---|---|---|
| `reprise` | `/home/marvin/Projects/reprise` | Faktenquelle. **Arbeitsstand ist `origin/dev`**, nicht `main`, nicht der lokale Checkout. |
| `reprise-showcase` | `/home/marvin/Projects/reprise-showcase` | Öffentliche Bühne. Remote `marvinbaudach/reprise-showcase`. |
| `bewerbung` | `/home/marvin/Projects/bewerbung` | CV, Anschreiben, PDFs (HTML → PDF über `build.sh`). |

## Getroffene Entscheidungen

1. **Zwei Bühnen.** Das README wird zur scanbaren Kurzfassung (Wordmark, sechs
   Kennzahlen, drei SVGs, Screenshot-Galerie, Link). Dazu eine echte
   GitHub-Pages-Seite mit Videos, großen Diagrammen und Kapitelnavigation.
   Grund: GitHub rendert lokale MP4s im README nicht.
2. **Bewegtbild wird neu aufgenommen** — 3 Desktop-Clips (Live-Spektrum,
   Scrollen durch die virtualisierte Tabelle, Geräte-Sync mit Fortschritt) und
   1–2 Android-Clips (Now Playing mit Spectral Seek, Tabwechsel). Stumm,
   6–12 s, Endlosschleife, ≤ 2 MB, headless aufgenommen. MP4 für Pages,
   WebP-Standbild fürs README. Zusätzliche Screenshots weiterer Oberflächen
   wurden **nicht** beauftragt.
3. **Bewerbung:** CV-Karte auf Seite 1 neu bauen **plus** ein neues einseitiges
   PDF „Projektsteckbrief Reprise" als Beilage.
4. **Zahlen:** Umstellung auf die strenge `#[cfg(test)]`-Trennung (siehe unten).
   Der Nutzer hatte keine Präferenz, die Entscheidung liegt bei der Umsetzung.
5. **Farbe:** Showroom auf **App-Teal**, CV bleibt **Indigo/Violett**. Verbunden
   über ein gemeinsames Bildsystem — gleiche Diagrammsprache, Typografie und
   Zahlendarstellung.
6. **GitHub Pages** ist im Showcase-Repo aktiviert: Quelle `main`, Ordner
   `/ (root)`, HTTPS erzwungen, keine Custom Domain. URL wird
   `https://marvinbaudach.github.io/reprise-showcase/`.

## Zwei Befunde, die den Text verändern

### 1. Die veröffentlichten LOC-Zahlen sind methodisch schief

Das README nennt für Commit `18000adcbe` **217'778 Produkt / 89'042 Test**.
Derselbe Commit, gezählt mit dem `#[cfg(test)]`-bewussten Analyzer, ergibt
**169'458 / 137'362**. Der Unterschied ist kein Wachstum, sondern Methodik:
inline-Testmodule wurden bisher als Produktcode gezählt.

Empfehlung (steht so im Konzept): auf die strenge Trennung umstellen, mit
Fußnote. **45 % der Rust-Codebasis sind Tests** trägt den TDD-Anspruch besser
als eine große Produktzahl. Die Zahl sollte nicht allein stehen, sondern an das
Traceability-Argument gekoppelt werden — sonst lädt sie zur Rückfrage ein, ob
142k Zeilen Test nicht schlicht Redundanz sind.

### 2. `marvinbaudach/reprise` ist öffentlich

Das README behauptet: *„The production source is private to preserve a
commercial option."* Das ist falsch und in zehn Sekunden widerlegbar. Der Satz
muss raus.

Daraus folgt eine Chance: Der Showroom kann jede Behauptung **anklickbar**
machen — Permalink auf `scripts/check-architecture.sh` neben „Architektur wird
maschinell erzwungen", Permalink auf `track_list_model.rs` neben „acht
SQL-Fenster, 1'600 Zeilen", Permalink auf `docs/ux-rules.md` neben der
Regelzahl. Das unterscheidet den Showroom von hübschen Portfolio-Seiten.

Nebenbei: Der Nutzer hat Pages **auch im `reprise`-Repo** aktiviert. Empfehlung
steht aus, es dort wieder abzuschalten — kein Leck (Repo ist öffentlich), aber
es öffnet eine zweite URL für dieselbe Sache und verwässert die Adresse, die in
Bewerbungen genannt wird.

## Gemessener Faktenstand

Alles gegen `origin/dev` = **`5995f70e777ec7c06a01738abf8ac7156d64e01e`**
(Fetch am 2026-08-11). Werkzeug: `cloc 2.08`, Test-Trennung per `syn`-AST.

### Rust und Kotlin

| Größe | Wert |
|---|---:|
| Rust gesamt | 316'794 |
| — Produktcode | 174'259 |
| — Testcode | 142'535 |
| Rust-Dateien | 1'449 |
| Rust-Testfunktionen (Attributzählung) | 5'729 |
| Kotlin (Android) | 17'403 in 126 Dateien |
| Android-Testfunktionen (`@Test`) | 256 |

Vergleich zum bisher veröffentlichten Commit `18000adcbe`, **gleiche Methodik**:
306'820 gesamt / 169'458 Produkt / 137'362 Test. Echtes Wachstum also
+9'974 Zeilen, davon +5'173 Test.

### Zeilen pro Crate (cloc, Rust, Produkt + Test zusammen)

| Crate | Zeilen |
|---|---:|
| `reprise-gnome` | 157'005 |
| `reprise-core` | 110'515 |
| `reprise-mcp` | 11'392 |
| `reprise-platform-linux` | 10'264 |
| `reprise-android-ffi` | 8'529 |
| `reprise-runtime` | 6'919 |
| `reprise-cli` | 4'079 |
| `reprise-view` | 3'777 |
| `reprise-stems` | 1'910 |
| `reprise-runtime-client` | 1'331 |
| `reprise-runtime-protocol` | 1'073 |
| **Summe** | **316'794** |

### Gruppierung für die Abbildung „Codeverhältnis"

| Gruppe | Zusammensetzung | Zeilen |
|---|---|---:|
| GNOME-Frontend | `reprise-gnome` | 157'005 |
| Kern | `reprise-core` + `reprise-view` | 114'292 |
| Plattform / Runtime | `platform-linux`, `runtime`, `runtime-client`, `runtime-protocol`, `stems` | 21'497 |
| Adapter | `cli` + `mcp` | 15'471 |
| Android | `android-ffi` 8'529 + Kotlin 17'403 | 25'932 |
| **Gesamt (Rust + Kotlin)** | | **334'197** |

Die Kernaussage der Abbildung: *Das Android-Frontend kostete 25'932 Zeilen, weil
es 114'292 Zeilen Kern nicht noch einmal schreiben musste.*

### Weiteres Werkzeug im Repo (cloc)

Python 14'541 · Bourne Shell 10'744 · JSON 14'550 · SVG 414 — das ist die
Gate-, Mess- und Bot-Infrastruktur und gehört in die Erzählung.

### Noch nicht exakt bestimmt

- **Aktive UX-Regeln.** 519 Regel-IDs mit Listeneintrag in `docs/ux-rules.md`,
  158 Zeilen tragen einen Ersetzt-/Withdrawn-Marker. Die aktive Zahl ist daraus
  **nicht** sauber ableitbar (Marker stehen teils in Erklärtexten). Muss exakt
  gezählt werden, bevor sie veröffentlicht wird.
- **Anzahl Gate-Stufen.** Das README nennt 18. Gegen `dev` nachzählen.
- **Produkt/Test-Aufteilung pro Crate.** Für die Abbildung nötig, aber der
  Analyzer liefert bisher nur Summen über den ganzen Baum. Lösung: den Rust-
  Analyzer um eine Ausgabe pro Top-Level-Verzeichnis erweitern (siehe unten).

## Werkzeuge und Befehle

**Zahlen reproduzieren:**

```bash
cd /home/marvin/Projects/bewerbung/.skill-staging/update-reprise-code-stats
./scripts/reprise-stats.sh /home/marvin/Projects/reprise 5995f70e77
```

Der Analyzer liegt unter `scripts/reprise-stats/src/main.rs` (215 Zeilen). Er
archiviert alle `.rs`-Dateien des Commits, projiziert per `syn` die Testanteile
(Dateikonventionen **plus** `#[cfg(test)]`-Module und `#[test]`-Funktionen) in
einen zweiten Baum und lässt `cloc` beide zählen. Für die Aufteilung pro Crate
muss dort eine Gruppierung nach Pfadpräfix ergänzt werden.

**Der `dev`-Baum liegt bereits ausgepackt** im Scratchpad unter `dev-tree/`
(43 MB, aus `git archive`), falls die Session noch lebt — sonst neu erzeugen.

**Vorhandene Assets im Showcase:** 7 Screenshots vom 2026-08-09
(Desktop 1920×1200 bzw. 1920×1164, Android 520×1158), zwei SVGs
(Architektur, Performance) je in EN und DE, Wordmark hell/dunkel.

**Bindende Vorgaben:** `docs/showcase.md` im Reprise-Repo hält die Showcase-
Policy fest — SVG statt Mermaid, 1440×900-Canvas, dunkler Grund, Produktfarbe
für Produkt / Mint für belegte Ergebnisse / Amber nur für Kosten, jede Abbildung
mit SVG-Titel und Alt-Text, **nie einen laufenden App-Screenshot erfinden**,
**nie eine Zahl aus dem Gedächtnis veröffentlichen**. Das Dokument ist in Teilen
veraltet (spricht von drei Crates), die Prinzipien gelten weiter.

## Konzept (abgestimmt, nicht freigegeben)

**Leitidee:** *Ein Rust-Kern. Zwei native Apps. Eine Beweiskette, die
entscheidet, was gemergt wird.* Drei Beweisstücke: geteilter Kern (Zahlen),
Verifikation (Bots bedienen die App selbst), Messung (vorher/nachher).

**Fünf SVG-Abbildungen als Rückgrat**, Text wird zur Bildunterschrift:

- **A · Kern und Kanten** — elf Crates, Abhängigkeitsrichtung, per `cargo tree`
  erzwungene Kernreinheit, Tauri sichtbar als geplant. Überarbeitung des
  vorhandenen Architektur-SVG.
- **B · Codeverhältnis** *(neu)* — Balken über die volle Breite, 334'197 Zeilen,
  segmentiert nach Kern / GNOME / Android / Adapter / Plattform; darunter
  derselbe Balken nach Produkt vs. Test.
- **C · Verifikationsstufen** — fünf Stufen als Treppe, jede mit „kann beweisen /
  kann nicht beweisen". Die oberen zwei sind agentengetrieben.
- **D · Explorations-Bot** *(neu)* — Kreislauf: AT-SPI-Baum lesen → selbst
  klicken → Hauptthread-Stalls messen → Anomalie melden (Fokusfalle, Zeile mit
  0×0-Ausdehnung, verschluckte Escape-Taste, Ruckler) → Befundbericht → Triage →
  Task mit Regel-ID → Test, der nach der Regel heißt → Gate. Mit echten
  Befunden beschriftet.
- **E · Performance** — messen/ändern/vergleichen, zwei Fälle: Index-Optimierung
  und Leerlauf-Frametakt.

**Pages-Seite:** dunkles Studio-Layout, editorial statt Karten-Raster, große
Zahlen als Ankerpunkte, Kapitelnavigation, Diagramme bauen sich beim Scrollen
auf. Zweisprachig EN/DE wie das README. Mit einem Dreh: **die Seite hält sich an
dieselben Regeln wie das Projekt** — sichtbare Fokusringe,
`prefers-reduced-motion` schaltet Bewegung ab, geprüfte Kontraste; das steht als
Fußzeile drunter und ist selbst ein Beleg.

**Dateien, die dazukommen:** `index.html`, `de.html`, leere `.nojekyll`, Videos
und SVGs unter `assets/`. Kein Git-LFS nötig bei ≤ 2 MB pro Clip.

**Reihenfolge:** Zahlen festziehen → fünf SVGs → Aufnahmen → README → Pages-Seite
→ CV-Karte und Projektsteckbrief → Gegenlesen (jede Zahl gegen `dev` verifiziert).

## Offene Punkte — hier hakt die Diskussion

1. **Positionierung.** „Von KI-Agenten gebaut" nach vorn stellen ist ehrlich und
   2026 interessant, kann aber als „hat er selbst etwas gekonnt?" ankommen.
   Alternative: geteilter Kern nach vorn, Agenten als Methode dahinter.
2. **Umfang.** Sechs Kapitel sind viel für jemanden, der überfliegt. Kürzen auf
   drei (Kern, Beweis, Design) mit ausklappbarem Rest?
3. **Projektsteckbrief.** Eine Seite ist knapp für drei Diagramme plus
   Screenshots — zwei Seiten oder nur zwei Diagramme.
4. **Die 45-%-Test-Zahl** braucht die Kopplung an Traceability, siehe oben.
5. **Designfähigkeit** ist im Konzept bisher nur durch Screenshots vertreten.
   Ein eigener Abschnitt (Designentscheidung → Umsetzung → Ergebnis) fehlt.
   **Offene Frage an den Nutzer: welche Designentscheidung hält er für die
   stärkste?** Das ist der einzige Punkt, der ohne seine Antwort nicht
   entschieden werden kann.

## Fallen in dieser Umgebung

- **`origin/dev` ist der Maßstab.** Der lokale Checkout hängt zurück
  (`d76689be8c` vs. `5995f70e77`), `main` erst recht.
- **Pages deployt aus `main`.** Arbeit auf einem Branch wird erst nach dem Merge
  live — vorher lokal zeigen.
- **Der Load-Governor-Hook blockt Bash-Kommandos**, die nach schweren
  Einstiegspunkten aussehen (etwa der bloße Dateiname
  `scripts/check-merge-readiness.sh` in einem `grep`). Umformulieren oder
  `HEAVY_RUN_DISABLE=1` setzen, wenn es wirklich nur eine Textsuche ist.
- **Bash-Ausgabe kappen** — lange Läufe in eine Datei, Frage per `grep`/`wc`
  beantworten.
- **Nicht ohne Freigabe pushen.** Weder Showcase noch Bewerbung.
