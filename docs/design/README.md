# Die Design-Vorlage des Showrooms

`reprise-showroom.design.html` ist der Entwurf „Reprise Showroom" aus dem
Claude-Design-Projekt `476e3050-d03c-4e6b-b376-448b2c137b03`, ausgepackt aus
dem Browser-Export vom 18.08.2026 (`__bundler/template`). 1666 Zeilen, alles
inline: Stile, Markup, Skript.

Sie liegt hier, damit sie **ohne MCP** lesbar ist — auch aus dem Codex-Sandkasten.
Sie ist Vorlage, nicht Quelltext: die Seite wird als React-Komponenten unter
`showroom/src/` gebaut, nicht als Kopie des Monolithen.

## Wie man sie liest

| Zeilen | Inhalt |
|---|---|
| 1–93 | Kopf, Schriften (Archivo, Martian Mono), Grundstile |
| 94–102 | alle `@keyframes` |
| 106–139 | Wurzel, Grund, Öl-Licht, Grain, Fortschritt, Kopfzeile |
| 140–190 | **Hero** samt Seek-Streifen |
| 191–209 | Tempo-Band |
| 210–316 | CH.01 |
| 317–434 | CH.02 |
| 435–599 | CH.03: Seek-Sektion und Mosaik |
| 600–644 | CH.04 |
| 645–701 | CH.05 samt Verfügbarkeit |
| 702–725 | Fußzeile |
| 729–748 | Lightbox |
| 750–1666 | das Skript: Lebenszyklus, Reveal, Zähler, Grund, Tilt, Lightbox, Seek, Viz |

Die Zeilenangaben gelten für **diese** getrackte Fassung. Wird sie ersetzt,
gelten sie neu — dann diese Tabelle mit nachziehen.

## Die Verweise auf Betriebsmittel

Der Entwurf adressiert Bilder und Schriften über UUIDs. Die elf Aufnahmen sind
dieselben, die der Showroom schon ausliefert — zugeordnet über den `alt`-Text
und die Dateigröße:

| UUID im Entwurf | Datei unter `showroom/public/` |
|---|---|
| `79f0c057-…` | `media/showroom/gnome-library.webp` |
| `0e7fb6f1-…` | `media/showroom/android-visualizer.webp` |
| `cb3b83b7-…` | `media/showroom/gnome-podcasts.webp` |
| `6b3f609f-…` | `media/showroom/android-library.webp` |
| `db1cd57e-…` | `media/showroom/gnome-youtube.webp` |
| `a17578d3-…` | `media/showroom/gnome-radio.webp` |
| `49e922b2-…` | `media/showroom/gnome-library-doctor.webp` |
| `aa3e5be6-…` | `media/showroom/android-cover.webp` |
| `709860f9-…` | `media/showroom/gnome-device-sync.webp` |
| `ebd67e6f-…` | `media/showroom/gnome-layout-controls.webp` |
| `37183eb8-…` | `media/showroom/gnome-listening-stats.webp` |
| `8dc5a95b-…` | `brand/reprise-mark.svg` |
| `faa2dd42-…` | `brand/favicon.svg` |

Die Schriften kommen im Showroom über Google Fonts (`index.html`), nicht als
eingebettete `woff2`. Das Skript-Betriebsmittel `9f3257c1-…` ist die Laufzeit
des Design-Werkzeugs und gehört **nicht** in den Showroom.

## Wenn sich der Entwurf ändert

Neu exportieren (Browser → HTML), dann:

```
node -e 'const fs=require("fs");fs.writeFileSync("out.html",JSON.parse(fs.readFileSync("template.json","utf8")))'
```

wobei `template.json` die Zeile des `<script type="__bundler/template">` ist.
Danach diese Datei ersetzen und den Unterschied ansehen — sie ist getrackt,
`git diff` zeigt, was der Entwurf verändert hat.

---

# Der Entwurf: Fortschritt beim Laden von Online-Inhalten (Android)

`android-download-progress.design.html` ist die Leinwand „Download Fortschritt"
aus dem Claude-Design-Projekt `532864f3-da4a-415d-9c4c-4b8513785766`, gezogen am
25.08.2026 über die Design-MCP (`DesignSync get_file`). Die Laufzeit-Verweise
(`support.js`, `_ds/…/_ds_bundle.js`, `styles.css`) sind beim Ablegen entfernt
worden — sie gehören zum Design-Werkzeug, nicht zum Entwurf. Übrig bleibt
reines Markup mit Inline-Stilen, das ohne MCP lesbar ist, auch aus dem
Codex-Sandkasten.

## Wie man sie liest

Zwei Abschnitte, der zweite ist der ältere.

| Abschnitt | Inhalt |
|---|---|
| Runde 2 (oben) | **verbindlich** — Variante 1c in allen vier Zuständen |
| Runde 1 (unten) | die drei zur Wahl gestellten Varianten `1a`, `1b`, `1c` |

Gewählt wurde **1c „Leiste mit Balken"**; `1a` (Aktivitäts-Chip) und `1b`
(zweite Kopfzeile) sind verworfen und stehen nur noch als Vergleich da. Wo sich
Runde 1 und Runde 2 unterscheiden, gilt Runde 2.

Die vier Zustände in Runde 2, jeweils als Handy-Rahmen von 300 px Breite:

| Reihenfolge | Zustand | Beschriftung | Zähler | Balken |
|---|---|---|---|---|
| 1 | Anzahl noch unbekannt | `Preparing artist photos` | keiner | unbestimmt |
| 2 | Laufend | `Downloading artist photos` | `128 / 412` | 31 % türkis |
| 3 | Abgeschlossen | `Artist photos complete` | `412 / 412` | 100 % türkis |
| 4 | Mit Fehlschlägen | `Artist photos complete` | `397 / 412` | 96 % türkis, Rest lila, dritte Zeile `15 without a photo` |

## Die Zahlen des Entwurfs

Der Rahmen ist eine Handy-Attrappe im Maßstab 300 px — die Werte **innerhalb**
der Karte sind als dp zu lesen, die Rahmenwerte (Radius 26, Breite 300) nicht.

| Ding | Wert im Entwurf |
|---|---|
| Karte, Außenabstand | `margin: 0 12px 8px` |
| Karte, Radius | `10px` |
| Karte, Fläche | `#272d38` (eine Stufe heller als der Screen-Grund `#22252e`) |
| Karte, Rahmen | `1px solid #333b48` |
| Karte, Innenabstand | `11px 12px` |
| Kopfzeile, Abstand zum Balken | `gap: 8px` |
| Beschriftung | `12px`, `#d3dae4` |
| Zähler | `12px`, `#5bd6b4` |
| Schließen `×` | `13px`, `#8f96a3` |
| Balken | Höhe `4px`, Radius `999px` |
| Balken, Spur | `#333b48` (= Rahmenfarbe) |
| Balken, Füllung | `#5bd6b4` |
| Balken, Fehlschlag-Rest | `#9184d9` (dieselbe Farbe wie die Favoriten-Herzen `♥` in den Listenzeilen) |
| Dritte Zeile | `11px`, `#8f96a3` |

Auf „Online sources" (Runde 1, `1c`, rechter Rahmen) sitzt dieselbe Karte
unterhalb des Toggles und oberhalb des Erklärungstexts, dort ohne
Außenabstand (der Screen hat schon `padding: 0 16px`) und mit `padding: 12px`,
`gap: 9px`.

Die Farbwerte sind **Beschreibung, nicht Vorgabe**: sie benennen, welches
vorhandene Theme-Token gemeint ist. Neue Farbwerte kommen nicht ins Theme.

## Wenn sich der Entwurf ändert

Neu ziehen statt neu tippen:

```
DesignSync get_file --project 532864f3-da4a-415d-9c4c-4b8513785766 \
  --path "Download Fortschritt.dc.html"
```

Danach diese Datei ersetzen, die Laufzeit-Verweise im `<head>`/`<helmet>`
wieder entfernen und `git diff` ansehen.
