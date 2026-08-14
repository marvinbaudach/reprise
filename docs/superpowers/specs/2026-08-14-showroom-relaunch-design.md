# Showroom-Relaunch — Design

**Stand:** 2026-08-14 · Abgestimmt im Brainstorming, noch nicht umgesetzt.
**Ersetzt:** `docs/plans/showcase-und-bewerbung-relaunch.md` und dessen HANDOFF.

## Auftrag

Reprise so darstellen, dass es als Bewerbungsartefakt trägt. Vier Flächen:
Produkt-README, eine öffentliche Showroom-Seite, die CV-Karte und das
Anschreiben.

**Zielrollen** (bestimmen jede Gewichtung in diesem Dokument):

1. KI-gestützte Entwicklung / Agenten-Engineering
2. Native App / Mobile Engineering

Der Bewerber kommt beruflich aus React/TypeScript. Der Wechsel zu Native ist
belegbedürftig — der Showroom muss diese Lücke aktiv schließen, nicht umgehen.

## Kernentscheidungen

### Eine Bühne, nicht zwei

`reprise-showcase` wird **archiviert, nicht gelöscht**. Alles zieht nach
`marvinbaudach/reprise`; die Showroom-Seite lebt unter
`https://marvinbaudach.github.io/reprise/`.

Gründe:

- Die Existenzberechtigung des Zweitrepos ist entfallen. Seine GitHub-
  Beschreibung behauptet bis heute *„Public showcase; the source is private"* —
  widerlegbar in zehn Sekunden, weil `reprise` öffentlich ist.
- Zwei Repos bedeuten vier Orte, an denen dieselben Zahlen driften. Genau das
  ist zweimal passiert (Showcase und CV zeigen beide Stand `18000adcbe`).
- Belege sollen anklickbar sein. Repo-intern sind das Links, die nie brechen;
  über Repo-Grenzen hinweg brechen sie bei jeder Umbenennung.
- Eine Adresse in einer Bewerbung ist stärker als zwei.

**Nicht löschen**, weil in bereits verschickten Bewerbungen
(`bewerbung/src/bewerbungen/cortec-fullstack.html`) die alte URL steht. Vor dem
Archivieren erhält das Repo eine weiterleitende `index.html` und eine
korrigierte Beschreibung.

### Die vier Flächen

| Fläche | Aufgabe | Leser |
|---|---|---|
| `reprise/README.md` | Was ist das, warum ist es gut gebaut, wo geht's zum Beweis | Entwickler auf dem Repo |
| Showroom-Seite | Das Bewerbungsartefakt | Recruiter, Hiring Manager |
| CV Seite 1, Reprise-Karte | Der Anreißer, der zum Klick führt | PDF-Stapel-Leser |
| Anschreiben (DE) | Brücke von Web zu Native/Agenten | Ein Mensch, eine Stelle |

**Sprache:** Englisch für README und Showroom. `README.de.md` und alle
`-de.svg`-Varianten entfallen. Nur das Anschreiben bleibt Deutsch.

## Der Showroom

### Positionierung

Ergebnis vor Methode, beide gleichgewichtig:

> Zwei native Apps — GNOME und Android — auf einem Rust-Kern.
> Gebaut mit KI-Agenten. Was gemergt wird, entscheiden die Gates, nicht der Agent.

Das beantwortet den Einwand, den ein Agenten-Profil auslöst: Ob das Ergebnis
trägt. Die Antwort ist nicht rhetorisch, sondern zwei ausgelieferte Apps.

### Drei Kapitel, Rest ausklappbar

**CH. 01 — „Two native apps. One core."**
Öffnet mit dem Artefakt, nicht mit Text. Danach zwei Abbildungen: *Kern und
Kanten* (elf Crates, Abhängigkeitsrichtung, per `cargo tree` erzwungene
Kernreinheit) und *Codeverhältnis* (ein Balken über die volle Breite, segmentiert
nach Kern / GNOME / Android / Adapter / Plattform). Die Pointe steht als
Bildunterschrift: *Das Android-Frontend kostete N Zeilen, weil es M Zeilen Kern
nicht noch einmal schreiben musste.* Am Kapitelende der Download-Block.

**CH. 02 — „Built by agents. Merged by gates."**
Zwei Abbildungen. Die *Verifikationstreppe*: fünf Stufen, jede beschriftet mit
„kann beweisen / kann nicht beweisen", die oberen beiden agentengetrieben. Der
*Explorations-Bot* als Kreislauf: AT-SPI-Baum lesen → selbst klicken →
Hauptthread-Stalls messen → Anomalie melden → Triage → Task mit Regel-ID →
Test, der nach der Regel heißt → Gate. **Mit echten Befunden beschriftet**
(Zeile mit 0×0-Ausdehnung, verschluckte Escape-Taste, Ruckler), nicht mit
Platzhaltern.

**CH. 03 — „Two frameworks. One visual signature."**
Öffnet mit dem randabfallenden Band (siehe Galerieform), darunter jede
Oberfläche einzeln mit der Zeile, welches Problem sie löst: MyStats,
Geräte-Sync, Library Doctor, Song-Visualisierung, Android.

Dieses Kapitel muss eine Sache explizit sagen, sonst wird sie falsch
verstanden: **Die beiden Apps sehen absichtlich verschieden aus.**
GNOME-Konventionen auf dem Desktop, Material auf dem Handy. Wer sie angleicht,
zeigt fehlende Plattform-UX-Reife. Gleich ist die Signatur — und die ist das
technisch Schwierigere: zwei Rendering-Stacks (GSK gegen Skia), zwei
Layout-Systeme, zwei Sprachen, dieselbe Visualisierung und dieselbe Physik.
Keine geteilte Komponente, sondern eine geteilte Spezifikation.

Danach die Kette am Beispiel Spectral Seek: Entscheidung (die Seek-Leiste zeigt
die Struktur des Tracks statt einer leeren Rinne) → Umsetzung (portable
Visuals-Schicht) → Ergebnis (nachgemessene Physik). Abschließend die UX-Regeln
als Verfassung und Accessibility als Gate.

**Ausklappbar dahinter:** Performance (Index-Optimierung, Leerlauf-Frametakt),
Crate-Tabelle, Funktionsumfang, Build und Verifikation.

### Visuelle Richtung

**Messprotokoll als Rahmen, Produktbühne als Bruch.** Sichtbares Raster,
technische Beschriftung, der gebaute Commit im Seitenkopf — aber jedes Kapitel
öffnet mit einer randabfallenden, beleuchteten Bühne, für die das Raster
aufreißt. Der Kontrast ist die Aussage: gemessen *und* schön.

**Farbe** ausschließlich aus `data/brand/palette.toml`, der einzigen gepflegten
Farbquelle des Projekts: `reprise_teal #4FDBD4` als Signalfarbe,
`reprise_coral #FF6F5E` als Zweitfarbe, sparsam. Keine erfundenen Töne.

**Typografie:** *Archivo* als variable Breitschrift — Headlines extrem breit und
fett wie eine Pegelanzeige, derselbe Font schmal und leicht für Fließtext.
*Martian Mono* für Datenzeilen. Kennzahlen werden in der Displayschrift gesetzt,
nicht in Monospace versteckt. Kein `system-ui`, kein Inter, kein Space Grotesk.

**Textur:** feines Grain über der ganzen Seite, darunter ein radial
ausmaskiertes Wellenraster.

**Signaturgeste:** Eine zwei Pixel hohe Haarlinie am oberen Rand füllt sich beim
Scrollen und trägt einen glühenden Kopf. Bei Hover/Fokus wächst sie kurz zum
Spektrum und zeigt die Kapitelmarken, dann zieht sie sich zurück. Damit zitiert
die Navigation Spectral Seek, ohne Aufmerksamkeit vom Aufmacher zu nehmen —
eine Entdeckung, keine Ansage.

**Galerieform in CH. 03:** randabfallendes Band aus überlappenden Fenstern in
Licht und Tiefe, Handy vorn, seitlich scrollbar. Darunter jede Oberfläche
einzeln in einer eigenen Zeile mit Erklärung. Überflieger sehen die Wirkung,
Leser die UX-Begründung. Ausdrücklich **kein** gleichmäßiges Karten-Raster.

### Bewegtbild

2–3 stumme Desktop-Clips (Live-Spektrum, Scrollen durch die virtualisierte
Tabelle, Geräte-Sync mit Fortschritt), 6–12 s, Endlosschleife, ≤ 2 MB.

**Android bewusst ohne Video.** Die Emulator-Aufnahme auf diesem Rechner löst
keine 60 Hz auf und die App ist auf 60 Hz gedeckelt — das Video würde ein
Ruckeln zeigen, das nicht existiert. Bei einer Native-App-Bewerbung ist das ein
Eigentor.

**Alle sieben vorhandenen Screenshots sind tot** (Stand 09.08.). Seither wurden
genau die fotografierten Oberflächen umgebaut: Geräte-Karten neu gebaut (#431),
Sort-Chip entfernt (#442), Kopfband-Text von der Grafik genommen (#440),
pausierter Visualizer (#432), Seitenleiste viermal (#441, #422, #423, #450),
Android Now Playing mit neuem Visualizer. Sie werden vollständig neu
aufgenommen.

## Technik

**Stack:** Vite + React + TypeScript, statisch vorgerendert. Der Gewinn geht
über „aktueller Stack" hinaus: Der CV führt React/TypeScript als Fundament, das
Eigenprojekt belegt bisher nur Rust und Kotlin. Die Seite schließt die Lücke —
drei Stacks statt zwei, und der dritte ist der berufliche.

Vorgerendert, weil eine Präsentationsseite ohne JavaScript lesbar sein muss.

**Animation:**

- CSS scroll-driven animations (`animation-timeline: view()`) für den Aufbau der
  Diagramme statt JS-Scroll-Listener — läuft im Compositor, kostet keinen
  Hauptthread. Dasselbe Prinzip, das die App verteidigt.
- View Transitions API für die Kapitelnavigation.
- Zahlen zählen beim Eintreten hoch, das Screenshot-Band mit leichter
  Parallaxe, der Spektrum-Balken der Seek-Demo läuft live.
- `prefers-reduced-motion` schaltet alles ab. **Das ist der Beweis, nicht die
  Einschränkung** — reiche Bewegung baut jeder, sauber abschaltbar machen
  unterscheidet von einem Template.

**Ort:** `showroom/` im `reprise`-Repo, eigenes `package.json`, vom
Rust-Workspace unberührt.

**Deployment:** Pages-Quelle auf „GitHub Actions" umstellen. Der Workflow misst
die Zahlen gegen den gebauten Commit, schreibt sie als typisiertes JSON, Vite
baut, Pages deployt. Rust misst, TypeScript rendert.

**Vor der Umsetzung zu prüfen:** ob Arch-Lint und `ci-quality` über ein
npm-Verzeichnis im Repo stolpern; ob `node_modules` und Lockfile sauber
behandelt sind.

## Zahlen und Belege

**Keine Zahl wird getippt.** Der Build misst gegen den Commit, den er baut, und
rendert das Ergebnis. Die Fußzeile sagt das wahrheitsgemäß:

> Jede Zahl auf dieser Seite wurde beim Bauen dieser Seite gemessen, gegen
> Commit `<sha>`. Keine ist getippt.

Dieselbe Messung speist `profile.js` im Bewerbungs-Repo. Eine Messung, drei
Flächen.

**Methodik:** strenge `#[cfg(test)]`-Trennung per `syn`-AST, gezählt mit `cloc`.
Der bisher veröffentlichte Wert (217'778 Produkt / 89'042 Test) zählte
inline-Testmodule als Produktcode. Die korrigierte Trennung ergibt für denselben
Commit 169'458 / 137'362. Die Umstellung braucht eine Fußnote und muss an das
Traceability-Argument gekoppelt werden — „45 % der Rust-Codebasis sind Tests"
lädt sonst zur Rückfrage ein, ob das nicht schlicht Redundanz ist.

**Exakt zu erheben, nicht zu schätzen:**

- Aktive UX-Regeln. `docs/ux-rules.md` hat 606 Listeneinträge, ein Teil trägt
  Ersetzt-/Withdrawn-Marker, teils in Erklärtexten. Das README nennt bisher
  „more than 340 active" — als Untergrenze robust, aber die exakte Zahl fehlt.
- Anzahl Gate-Stufen. Das alte README nennt 18; `ci.yml` hat drei Jobs, die
  Stufenzahl kommt aus der dokumentierten Zählung. PR #471 hat Android-Builds
  und -Tests ergänzt.
- Produkt/Test-Aufteilung pro Crate. Der Analyzer liefert bisher nur Summen; er
  braucht eine Gruppierung nach Pfadpräfix.

**Anklickbarkeit:** Neben jeder harten Behauptung ein Permalink in den
Quellcode. Der Generator kennt den gebauten Commit und schreibt die SHAs selbst
— es kann keinen toten Beleglink geben.

**Die Seite misst sich selbst.** In der Fußzeile ihre eigenen Werte —
Lighthouse, Bundle-Größe, kein Layout-Shift, geprüfte Kontraste — erhoben im
selben CI-Lauf.

## Downloads

Die Seite bietet Flatpak-Bundle und APK an. Wer die Apps installieren kann,
glaubt nicht mehr, er sehe Mockups.

**Stand:** Das Flatpak ist vorbereitet (`io.github.marvinbaudach.Reprise.yml`,
`flatpak/cargo-sources.json`, zwei Prüfskripte). Es existiert **kein einziger
Release** — `gh release list` ist leer. Das APK wird im Release-Block mit dem
Debug-Keystore signiert (`signingConfig = signingConfigs.getByName("debug")`),
dem Platzhalter aus dem Projektgerüst.

**Vorgehen:** Der Download-Block wird von Anfang an gebaut. Die Release-Pipeline
läuft als eigener Strang daneben — Release-Keystore als GitHub-Secret,
Release-Workflow für Flatpak-Bundle und signiertes APK — und schaltet den Block
scharf. Der Showroom darf nicht auf ein Keystore-Setup warten.

**Verhalten ohne Release:** Der Block liest dieselbe generierte JSON wie die
Zahlen. Findet der Build keinen Release, wird der Block **nicht gerendert** —
weder als leerer Rahmen noch als „demnächst". Ein Showroom, der Downloads
verspricht und keine liefert, widerlegt seine eigene Behauptung, dass hier
nichts behauptet wird.

**Textfolge:** Sobald Downloads angeboten werden, stimmt der README-Satz
*„active alpha. Reprise is not a public release yet"* nicht mehr. Ersatz:
„Alpha — installierbar, aber noch nicht in Flathub."

## Bewerbung

**Kein Projektsteckbrief-PDF.** Die Showroom-Seite ist der Steckbrief. Die
CV-Karte auf Seite 1 wird dafür verdichtet und korrigiert:

- Tauri-Badges raus („KDE · Tauri 2", „Windows · Tauri 2") — dieselbe Zusage
  wurde im Showcase bereits gestrichen; ein Leser, der beides sieht, findet
  einen Widerspruch.
- Zahlen aus der gemeinsamen Messung statt aus `18000adcbe`.
- Link auf `marvinbaudach.github.io/reprise/` statt auf das Showcase-Repo,
  QR-Code daneben.
- `1 → 2 · Ein Core, zwei native Frontends` bleibt — das Argument steht bereits.

**Anschreiben** bleibt Deutsch, bekommt einen Absatz, der die Brücke schlägt:
Web-Hintergrund, native Apps im Eigenprojekt, Agenten als Arbeitsweise.

## Übernommen aus dem Bestand

Im Arbeitsverzeichnis von `reprise-showcase` liegt uncommittet eine
abgeschlossene Änderung: **Tauri 2 vollständig entfernt** — README EN und DE,
beide Architektur-SVGs (die verbleibenden zwei Frontend-Pillen wurden neu
vermessen, 600 px statt 270/300, kein Loch im Layout), dazu die Gate-Zeilen in
`check-showcase.sh` und `readme-evidence.sh`. Beide Gates laufen darauf grün,
kein Tauri-Rest im Baum.

Diese Arbeit wird portiert, nicht wiederholt. Ebenfalls entfernt wurde ein
`Impact-Site-Verification`-Meta-Tag samt Gate-Zeile.

## Nicht im Umfang

- Android-Videos (Begründung oben).
- Zweisprachigkeit von README und Showroom.
- Ein Projektsteckbrief als PDF.
- Tauri als Roadmap-Zusage — sie entfällt auf allen Flächen.
- Umbenennung oder Löschung von `reprise-showcase`.

## Stränge

Die Arbeit ist zu groß für einen einzelnen Umsetzungsplan. Fünf Stränge, drei
davon parallelisierbar:

1. **Messung** (blockiert 2 und 5) — Analyzer um Gruppierung pro Crate
   erweitern, aktive UX-Regeln und Gate-Stufen exakt auszählen, Ausgabe als
   typisiertes JSON, CI-Schritt.
2. **Showroom-Anwendung** — Vite/React/TS-Gerüst unter `showroom/`, drei
   Kapitel, fünf Abbildungen, Animation, Pages-Workflow.
3. **Aufnahmen** (unabhängig) — sieben Screenshots neu, 2–3 Desktop-Clips.
4. **Release-Pipeline** (unabhängig) — Keystore, Release-Workflow, Flatpak-
   Bundle und signiertes APK.
5. **Textflächen** — README auf Englisch neu, `README.de.md` entfernen, CV-Karte
   und `profile.js`, Anschreiben-Absatz, Showcase-Repo weiterleiten und
   archivieren.

## Risiken

**Das Zahlen-Gate bricht bei der Umstellung.** `readme-evidence.sh` prüft
wörtlich auf `5,541`, `172` und `More than 340 active UX rules`. Die
Zahlenumstellung und die Gate-Anpassung müssen dieselbe Änderung sein, nicht
zwei aufeinanderfolgende.

**Der Showroom ist selbst das erste Exponat.** Sieht die Seite aus wie ein
Standard-Template, ist Kapitel 3 widerlegt, bevor es gelesen wird. Das
Messprotokoll muss zeitgenössisch-technisch wirken, nicht retro-Terminal — der
Grat ist schmal.

**`origin/dev` bewegt sich schnell.** Während dieses Brainstormings sind zwei
Commits gelandet. Jede handgepflegte Zahl ist veraltet, bevor sie
veröffentlicht ist; deshalb misst der Build.
