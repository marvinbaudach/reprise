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
