# Die Eule neu bauen: ein Vektor-Master, drei Zeichnungen, vier Flächen

Das App-Icon ist eine Eule, die Vorlage für das neue Logo ist eine Eule mit
Over-Ear-Kopfhörern. Beide existieren heute nur als Raster, und die
Ein-Farb-Fassung ist ein automatischer Trace. Das Ergebnis: bei 16 px steht ein
grauer Klumpen, auf Android steht gar nichts, und für eine Website gibt es
nichts in voller Qualität.

Diese Spec baut das Logo als **handgezeichneten Vektor** neu — einmal, in drei
größenabhängigen Zeichnungen, aus denen GNOME, Android und Web bedient werden.

## Befund

Der Ausgangszustand ist unübersichtlicher, als er aussieht.

**Das aktuelle Eulen-Icon ist nirgends committet.** Die PNGs unter
`data/icons/hicolor/{48,64,128,256,512}x…`, das gelöschte
`scalable/apps/org.reprise.Reprise.svg` und das überschriebene Symbolic sind
uncommittete Arbeit im Haupt-Checkout, vermischt mit rund 25 unbeteiligten
Änderungen. Auf `origin/dev` liegt noch das ursprüngliche blaue Vinyl-SVG.

**Das Symbolic ist ein potrace-Trace.** Ein einziger Pfad mit mehreren tausend
Knoten, `viewBox="-23.70 -94.20 521.40 521.40"` bei deklarierten 16×16. Ein
Symbolic-Icon wird auf einem 16-px-Raster gezeichnet; ein auf ein Sechzehntel
heruntergerechneter Trace kann dort nicht funktionieren.

**`data/meson.build` installiert nur noch PNGs.** Die Begründung im Kommentar
lautet, das Artwork sei Raster mit Verläufen. Mit einem Vektor-Master entfällt
dieser Grund.

**Android hat kein Icon.** `android/` existiert nur auf `dev` — nicht auf
`main`. `android/app/src/main/res/` enthält ausschließlich `values/`, und
`AndroidManifest.xml` deklariert weder `android:icon` noch `android:roundIcon`.
`minSdk = 26` bedeutet: Adaptive Icons sind garantiert, Legacy-PNG-Buckets sind
nicht nötig, und ab API 33 greift der `<monochrome>`-Layer für Themed Icons.

## Was die Studien gezeigt haben

Drei Silhouetten-Studien (Recraft V4.1, Vektor-Modus, eine einzige Farbe),
gerendert bei 512, 48 und 16 px. Belegt wurde:

1. **Ohrbüschel sind das Eulen-Signal.** Die Studie ohne Büschel liest sofort
   als Affe. Sie sind nicht dekorativ, sie tragen die Wiedererkennung.
2. **Der Bügel überlebt klein, die Cups nicht.** Die Ohrmuscheln verschmelzen
   ab etwa 48 px mit der Kopfform; der Bügelbogen bleibt als eigene Masse
   lesbar.
3. **Padding tötet kleine Größen.** Die Studie mit Luft um die Marke wurde bei
   16 px zu einem gleichmäßigen Grau.
4. **Keine Zeichnung funktioniert über die ganze Spanne.** Genau deshalb
   größenabhängiges Artwork statt einer skalierten Datei.

## Entscheidungen

| # | Entscheidung |
|---|---|
| E1 | Das farbige App-Icon bleibt farbig. Monochrom ist eine abgeleitete Fassung, die tragen muss — kein Ersatz. |
| E2 | Größenabhängiges Artwork statt einer skalierten Zeichnung. |
| E3 | Handgezeichneter Vektor als Master. Generierte Bilder dienen ausschließlich als visuelle Referenz; **es wird nichts getract**. |
| E4 | **`full` bleibt nah an der Vorlage** — Verläufe, lange Ohrbüschel, geriffelte Ohrmuscheln. Die geometrische Vereinfachung greift erst ab `reduced`, wo die Größe sie erzwingt. Alle Stufen randfüllend. |
| E5 | Website bekommt Bildmarke **und** horizontale Wortmarke, hell und dunkel, als SVG. |
| E6 | Wortmarke in einer editorialen Schrift — Kontrast zur Eule als Wiedererkennungsstrategie. |

E4 ersetzt eine frühere Fassung, die auch die große Stufe geometrisch
vereinfacht hätte. Die Vorlagentreue der hochauflösenden Fassung wurde
nachträglich als Anforderung nachgereicht und wiegt schwerer: Vereinfachung
findet jetzt nur noch dort statt, wo die Pixelzahl sie erzwingt.

## Vorlage

Die verbindliche Vorlage liegt im Repo. Zwei Dateien:

- `owl-headphones-template.png` — wie eingereicht, 1163×929. **Ohne echte
  Transparenz**: das Karomuster ist eingebrannte Bilddaten, die Datei ist ein
  Screenshot eines transparenten Bildes.
- `owl-headphones-template-clean.png` — dieselbe Marke mit entferntem
  Karomuster (Flood-Fill vom Rand, danach Entfernung eingeschlossener
  Karo-Komponenten nach Bimodalitätstest, damit die schmalen Silberkanten am
  Bügel erhalten bleiben). **Das ist die Referenz für V8.**

Sie ist keine Stimmungsreferenz, sondern das Ziel für `full`.

Gemessene Geometrie der bereinigten Marke, normiert auf ihre Bounding-Box
(1147×920, Seitenverhältnis 1,2467):

| Landmarke | Wert |
|---|---|
| Breiteste Stelle (Ohrmuscheln) | y = 0,674, volle Breite |
| Augenzentrum links | (0,358 / 0,757) |
| Augenzentrum rechts | (0,641 / 0,758) |
| Augenbreite / -höhe | 0,078 / 0,09–0,11 |
| Unterkante Kopf | y = 0,999, mittig |

Ihre tragenden Merkmale, in absteigender Wichtigkeit:

1. **Lange, geschwungene Ohrbüschel**, die nach außen-oben auslaufen und **vor
   dem Bügel** liegen — sie durchstoßen ihn optisch. Das ist die auffälligste
   Eigenheit der Vorlage.
2. **Winkelbrauen** als dunkle V-Masse zwischen den Augen, die den Blick trägt.
3. **Bügel mit Verlauf** von tiefem Violett an den Enden nach Blau/Teal im
   Scheitel, mit heller Glanzkante.
4. **Geriffelte Ohrmuscheln** — konzentrische Ringe mit Teal-Akzent, die wie
   Schallwellen lesen.
5. **Augen mit Verlauf** von Teal nach Gelbgrün, dunkle Pupille, ein weißer
   Lichtpunkt oben rechts.
6. **Heller Gesichtsverlauf** gegen den dunklen Kopf.

## Formsprache und Konstruktion

Eulenkopf frontal, symmetrisch zur Vertikalachse.

**`full` folgt der Vorlage.** Verläufe sind ausdrücklich erlaubt — SVG
beherrscht sie, und die Behauptung im heutigen `data/meson.build`, Verläufe
erzwängen Raster, ist schlicht falsch. Merkmale 1–6 oben werden gehalten.

**Ab `reduced` greift geometrische Disziplin**, weil die Größe sie erzwingt:
Kreisbögen und gerade Kanten, Verläufe fallen zu Flächen zusammen, die
Muschelriffelung entfällt. Das ist keine andere Marke, sondern dieselbe unter
Druck — die Identitätsträger bleiben.

Zwei Elemente tragen die Identität und werden in **jeder** Zeichnung gehalten,
auch in `micro`:

- **Ohrbüschel** — zwei spitze Dreiecksformen, die aus der oberen Kopfkante
  herauswachsen. Sie sind Teil der Kopfsilhouette, kein aufgesetztes Detail.
- **Winkelbrauen** — eine durchgehende Masse über den Augen, die zur Mitte hin
  nach unten kippt. Sie erzeugt den wachen Eulenblick und ist das Element, das
  klein am längsten lesbar bleibt.

Die **Kopfhörer-Cups sind Teil der Kopfaußenkante**, keine separaten Scheiben.
Nur der **Bügel** ist eine eigenständige Form: ein Bogen über dem Kopf, der die
Ohrbüschel durchstößt. Augen und Schnabel sind **Negativraum**, keine eigenen
Farbflächen — das ist die Bedingung dafür, dass die Monochrom-Ableitung ohne
zweites Design auskommt.

Die Marke füllt ihre Fläche. Auf jeder Stufe belegt die Zeichnung mindestens
70 % der Kantenlänge der Live-Fläche.

## Die drei Zeichnungen

Statt sechs Artworks zu pflegen, kollabiert die Staffel auf drei Zeichnungen
mit klarer Zuständigkeit:

| Zeichnung | Inhalt | Bedient |
|---|---|---|
| **full** | Vorlagentreu: lange Ohrbüschel vor dem Bügel, Verlaufsbügel, geriffelte Muscheln, Verlaufsaugen mit Lichtpunkt, Gesichtsverlauf | 512 / 256 / 128 px, `scalable`-SVG, Website-Marke, Lockups, Apple-Touch-Icon |
| **reduced** | Eule + Bügel + Ohrbüschel; Muschelriffelung entfällt, Cups gehen in die Kopfform auf, Verläufe werden Flächen | 64 / 48 px, Android-Foreground, Android-Monochrome |
| **micro** | Kopf, Ohrbüschel, Brauen, Augen als Negativraum; kein Bügel; Kanten aufs Pixelraster gehintet | 24 / 16 px, GNOME-Symbolic, **Favicon** |

Der Favicon-Fall ist bewusst zugeordnet: Browser-Tabs rendern 16 px. Dorthin
gehört `micro`, nicht die volle Marke.

## Farbsystem

Die Palette ist aus der Vorlage gemessen, nicht aus dem heutigen Icon — dort
fehlt der Teal-Verlauf des Bügels vollständig, weil das aktuelle Icon gar keine
Kopfhörer hat.

| Rolle | Wert |
|---|---|
| Tinte / Kontur | `#1F1056` |
| Kopf dunkel | `#2B155E` |
| Kopf mittel | `#452674` |
| Körper hell | `#5F2F8A` |
| Gesicht hell | `#8A679C` |
| Bügel Teal | `#4F93C8` |
| Bügel Aufhellung | `#8292D5` |
| Muschel-Akzent | `#5798BD` |
| Auge Teal | `#1C698A` |
| Auge tief | `#114A71` |
| Metall / Glanz | `#A09EB4` → `#E0E0E1` |

Die Grundfläche des GNOME-App-Icons bleibt bei `#1B082D` aus dem heutigen Icon,
damit der Launcher-Eindruck nicht springt.

**Monochrom-Ableitung:** dieselben Pfade, eine einzige Füllung, Augen und
Schnabel bleiben Aussparung. Es entsteht keine zweite Zeichnung. Damit können
GNOME-Symbolic und Androids Themed-Icon nicht auseinanderlaufen.

## Liefermatrix

### GNOME — `data/icons/`

```
scalable/apps/org.reprise.Reprise.svg      128×128 viewBox, full
48x48/apps/org.reprise.Reprise.png         aus reduced
64x64/apps/org.reprise.Reprise.png         aus reduced
128x128 | 256x256 | 512x512 …png           aus full
symbolic/apps/org.reprise.Reprise-symbolic.svg   16×16, micro
```

Das App-Icon trägt eine Grundfläche: abgerundetes Rechteck 112×112 bei (8,8)
auf dem 128er Raster, Eckradius 26 — die Maße des zuvor im Repo vorhandenen
SVG.

Das Symbolic ist ein einzelner `<path>` auf 16×16 viewBox, Live-Fläche 14×14
bei (1,1), `fill="#222222"`, ohne `stroke`, ohne `transform`, ohne Verläufe.

`data/meson.build`: die PNG-Schleife bleibt — sie ist jetzt durch echtes
größenabhängiges Artwork begründet — und die Installation des
`scalable`-SVG kommt zurück.

### Android — `android/app/src/main/res/`

```
mipmap-anydpi-v26/ic_launcher.xml          <adaptive-icon>
mipmap-anydpi-v26/ic_launcher_round.xml    identisch, Rundmaske
drawable/ic_launcher_foreground.xml        VectorDrawable, reduced
drawable/ic_launcher_monochrome.xml        VectorDrawable, reduced einfarbig
values/ic_launcher_background.xml          Farbe #1B082D
```

Alle Layer auf 108-dp-Viewport; die Marke liegt vollständig in der
**72-dp-Safe-Zone** (Inset 18 dp je Seite), weil Launcher-Masken alles
außerhalb beschneiden dürfen.

`AndroidManifest.xml` bekommt `android:icon="@mipmap/ic_launcher"` und
`android:roundIcon="@mipmap/ic_launcher_round"`.

Da Adaptive Icons Vektoren sind, entfallen Dichte-Buckets. Für den
Store-Eintrag entsteht zusätzlich ein 512×512-PNG.

### Web — `data/brand/`

```
mark.svg                      full farbig, freistehend, für helle Gründe
mark-on-dark.svg              full farbig, Körperwerte angehoben, für dunkle Gründe
mark-mono.svg                 full monochrom, fill="currentColor"
lockup-horizontal.svg         Marke + Wortmarke nebeneinander
lockup-vertical.svg           Marke über Wortmarke
lockup-horizontal-outlined.svg  Wortmarke als Pfade (Fallback)
lockup-vertical-outlined.svg    Wortmarke als Pfade (Fallback)
favicon.svg                   micro, 16×16
favicon-32.png
apple-touch-icon-180.png      full auf Grundfläche
```

Hell/Dunkel wird zweistufig gelöst. Die **Mono-Fassungen** brauchen keine
Dupletten: `fill="currentColor"` übernimmt die Textfarbe der Umgebung. Die
**farbigen** Fassungen brauchen sie, weil die Körperwerte `#34125C`–`#481D70`
auf dunklem Grund zu wenig Abstand haben — `mark-on-dark.svg` hebt sie an und
behält den Augen-Akzent als hellsten Wert. Das Apple-Touch-Icon liefert seine
Grundfläche mit und ist deshalb von der Umgebung unabhängig.

## Typografie der Wortmarke

**Fraunces** (SIL OFL, variabel) als primäre Schrift für „Reprise". Begründung:
die Achsen `opsz`, `SOFT` und `WONK` erlauben, den Schriftzug gezielt auf
Display-Größe zu justieren statt eine Textschrift zu vergrößern; der warme,
editoriale Ton steht im Kontrast zur streng geometrischen Eule, und genau diese
Spannung erzeugt Wiedererkennung. Der Name ist ein Musikbegriff — eine Schrift
mit Verlagscharakter trägt das.

Fallback-Stack in den Live-Text-SVGs: `Fraunces, "Instrument Serif", Georgia,
serif`.

Weil Live-Text ohne geladene Schrift bricht, entstehen die Lockups **zusätzlich**
in einer Pfad-Fassung (`*-outlined.svg`) für Kontexte ohne Webfont — README,
GitHub, Fremdeinbettung. Die Live-Text-Fassung bleibt die primäre.

## Nicht-Ziele

- Keine Animation und kein 3D. Verläufe und Glanzkanten sind in `full`
  dagegen ausdrücklich erwünscht — sie sind Teil der Vorlage.
- Keine Legacy-PNG-Buckets für Android (minSdk 26).
- Kein separates Rund-Artwork — die Adaptive-Maske erledigt das.
- Keine Änderung der App-Farbwelt über das Icon hinaus.
- Kein Rebranding des Namens oder der Wortmarke selbst.

## Verifikation

Gemessen wird headless, nicht betrachtet.

**Live-Fläche** heißt je Fläche etwas anderes und wird hier einmal festgelegt:

| Fläche | Live-Fläche |
|---|---|
| GNOME App-Icon | die Grundfläche, 112×112 auf 128er Raster |
| GNOME Symbolic | 14×14 bei (1,1) auf 16er Raster |
| Android | 72×72 dp Safe-Zone auf 108-dp-Viewport |
| Web-Marke | die volle viewBox |

| # | Kriterium | Messung |
|---|---|---|
| V1 | Randfüllung | Gilt nur für Icon-Flächen, nicht für Lockups. Stufe via `rsvg-convert` gerendert; die Bounding-Box der Marke spannt **in beiden Achsen** ≥ 70 % der Live-Fläche. |
| V2 | Negativraum überlebt | 16-px-Render, Alpha bei 50 % binarisiert, 4er-Nachbarschaft: ≥ 2 getrennte Hintergrund-Zusammenhangskomponenten (Außenraum + Augenaussparung). Ein Klumpen hat genau 1. |
| V3 | Monochrom trägt | Alle Füllungen auf einen Wert abgeflacht; danach gilt V2 unverändert. |
| V4 | Kontrast der **farbigen** Marke | Der hellste Körperwert von `mark.svg` ≥ 4,5:1 gegen `#FFFFFF`, der von `mark-on-dark.svg` ≥ 4,5:1 gegen `#1B082D`. Gilt **nicht** für Symbolic und Themed Icon: beide werden von GNOME beziehungsweise Android zur Laufzeit umgefärbt, der literale Füllwert wird nie angezeigt. Dort trägt allein die Silhouette, also V2. |
| V5 | Kein Trace | Kein einzelnes `d`-Attribut überschreitet 400 Pfadbefehle, und die Gesamtzahl der Pfade bleibt bei `full` ≤ 60, `reduced` ≤ 20, `micro` = 1. Ein Trace erzeugt genau das Gegenteil: einen Riesenpfad mit Tausenden Befehlen. Der Test greift also auf die Signatur des Tracens, nicht auf Detailreichtum — `full` darf beliebig fein sein, solange es aus benannten Formen besteht. |
| V8 | Vorlagentreue | `full` bei 512 px gegen `docs/assets/brand-reference/owl-headphones-template.png` gestellt: die sechs Merkmale aus dem Abschnitt „Vorlage" sind einzeln nachweisbar vorhanden. |
| V6 | Android real | Auf `pixel10xl_api37` installiert und mit **eingeschalteten Themed Icons** geprüft: `<monochrome>` sichtbar, nichts von der Maske beschnitten. |
| V7 | Symbolic-Hygiene | Symbolic-SVG: genau ein `<path>`, viewBox `0 0 16 16`, kein `transform`, kein `stroke`, kein Verlauf. |

V2 ist der eigentliche Test. Er ist genau das Kriterium, an dem das heutige
getracte Symbolic scheitert: bei 16 px läuft dessen Augenaussparung zu, und es
bleibt eine einzige Hintergrundkomponente.

Die Messungen V1–V5 und V7 laufen als Skript, damit sie bei jeder Iteration
wiederholbar sind statt einmalig begutachtet.

## Risiken

- **VectorDrawable ist nicht SVG.** Der unterstützte Pfadumfang ist kleiner.
  Die Konvertierung wird geprüft, nicht angenommen.
- **`micro` bei 16 px ist echte Handarbeit.** Mehrere Iterationen einplanen;
  V2 entscheidet, nicht der Eindruck.
- **`dev` bewegt sich.** Der Branch liegt 52 Commits vor `main` und ist aktiv.
  Vor dem Merge rebasen.
- **Live-Text-Lockups brechen ohne Webfont** — abgefangen durch die
  Outlined-Fassungen.

## Reihenfolge

Alles hängt an den drei Zeichnungen; die vier Flächen sind reine Ableitungen
davon und untereinander unabhängig. Daraus folgt eine Stufe und danach eine
Fläche pro Strang:

1. **`full`, `reduced`, `micro` zeichnen** und gegen V1–V5, V7 messen. Erst
   wenn `micro` V2 besteht, geht es weiter — an dieser Stufe scheitert das
   heutige Icon, und sie ist die schwerste.
2. Danach parallel und ohne Reihenfolge untereinander: **GNOME**, **Android**,
   **Web**. Keiner der drei Stränge liest Dateien eines anderen.

V6 kann erst laufen, wenn der Android-Strang steht.

## Arbeitsort

Worktree `.worktrees/owl-logo`, Branch `feat/owl-logo`, Basis `origin/dev` —
der einzige Baum, der `data/icons/` **und** `android/` enthält.

Die Vorlage liegt committet unter `docs/assets/brand-reference/`. Das
uncommittete Eulen-Material aus dem Haupt-Checkout (PNGs ohne Kopfhörer,
potrace-Symbolic) ist ausschließlich im Sitzungs-Scratchpad gesichert und
damit flüchtig; es ist für die Umsetzung nicht erforderlich.

## Änderungen aus der Umsetzung

Diese Spec wurde vor der ersten Zeichnung geschrieben. Was danach an
gerenderten Bildern und an Messwerten sichtbar wurde, steht hier — mit dem
Grund, nicht nur mit dem neuen Wert. Wo ein Punkt oben und hier verschieden
sind, gilt dieser Abschnitt.

### Grundfläche und Palette

- **Die Grundfläche des App-Icons ist kein flaches `#1B082D` mehr, sondern
  eine Verlaufsplatte `#5798BD → #8570CB`.** Gemessen: Kopf `#2B155E` gegen
  `#1B082D` ergibt **1,31:1**. Die Palette hat zu wenig Helligkeitsspanne,
  um eine dunkle Eule auf einem dunklen Grund zu tragen; hellere Platten
  retten den Kopf, killen aber die helle Gesichtsscheibe. Die Verlaufsplatte
  war eine von drei durchgerechneten Auswegen und ist die entschiedene.
- **Der Kopf-Verlauf endet bei `#33195F` statt `#452674`.** Die Kinnkante ist
  der einzige Rand, der gegen das violette Ende der Platte antritt: mit
  `#452674` sind das 2,49:1, mit `#33195F` 3,58:1.
- **`Metall / Glanz #A09EB4 → #E0E0E1` entfällt.** `#E0E0E1` erreicht gegen
  Weiß 1,32:1. Der Schnabel trägt jetzt `#A7B7CE → #2B3B69`, aus der Vorlage
  gemessen; sie geht dort als einzige Stelle ins Marineblau.
- **Die Augen haben keinen grünen Ring um eine teale Iris.** In der Vorlage
  ist das ein Verlauf Teal → Gelbgrün innerhalb der Iris. Ein Ring zerfällt
  bei kleiner Größe zu einem grünen Fleck.

### Konstruktion des Gesichts

- **Die Braue ist keine eigene Form.** Sie ist die Oberkante der
  Gesichtsscheibe: eine Gerade mit Steigung 0,589 von (316|596) zum Scheitel
  (623|777). Der dunkle Kopf darüber *ist* die Braue. Die erste Fassung legte
  eine dunkle Fläche über runde Augen und drückte sie damit von 139 auf 99
  Einheiten Höhe — bei 128 px las die Marke als Katze.
- **Gemessene Augen der Vorlage:** sichtbar 148 × 139 Einheiten, Mitten
  (472|734) und (769|733), Pupille ⌀ 74 bei (473|726), Glanzpunkt ⌀ 16 bei
  (450|704). Die Zeichnung trifft das auf wenige Einheiten.

### Zuständigkeit der Stufen

- **`reduced` zeigt nur den Kopf, ohne Kopfhörer.** Gemessen: in der vollen
  Komposition nehmen Bügel und Muscheln zwei Drittel der Breite; bei einem
  48-px-Icon bleibt der Kopf 19 px breit und jedes Auge 2 px. Ohne
  Kopfhörer trägt derselbe Kopf 36 px. Die Kopfhörer bleiben ab 128 px, im
  Lockup und im Web.
- **`micro` bedient 24 und 32 px**, nicht 16 und 24. Das GNOME-Symbolic wird
  aus `micro` **erzeugt** statt ein zweites Mal gezeichnet — zwei Hände an
  derselben Silhouette lassen sie auseinanderlaufen.
- **`reduced` steht im Raster der vollen Zeichnung** (`0 0 1247 1000`) und
  benutzt dieselben Koordinaten. Ein eigenes quadratisches Raster sitzt
  anders auf der Platte, und dann zeigt dieselbe App bei 48 px ein sichtbar
  anderes Logo als bei 128 px.

### Kriterien

- **V4 ist neu gefasst.** Alt: „der hellste Körperwert ≥ 4,5:1 gegen Weiß
  beziehungsweise `#1B082D`". Das war aus drei Gründen falsch. Der Test lief
  nie — die Funktion stand als toter Code im Skript. Er misst einen einzelnen
  Hex-Wert, also entscheidet ein Glanzpunkt über eine Marke, die auf ihrer
  ganzen Fläche im Grund versinkt. Und 4,5:1 ist die Schwelle für Fließtext.
  Neu: **flächengewichtet am Saum der gerenderten Marke gegen den
  tatsächlichen Grund** — Median ≥ **3,0** (WCAG 1.4.11, grafische Objekte)
  und höchstens 2 % der Randfläche unter 1,5:1. Für die Platte wird Pixel
  gegen Pixel gemessen, weil ein Verlauf keinen einzelnen Wert hat.
- **V2 und V3 gelten nur für Silhouetten** — `micro`, Symbolic, die
  Mono-Fassungen. Die farbigen Stufen haben keine durchsichtigen Löcher;
  ihre Augen sind gefüllte Flächen. Der Versuch, „bleiben zwei Augen übrig"
  über zusammenhängende Farbflächen zu messen, war auf der Verlaufs-Iris
  nicht stabil (2 bis 9 Flächen je nach Rendergröße) und wurde verworfen,
  statt eine schwankende Zahl grün zu nennen.
- **V5 zählt Formen, nicht Pfade.** `<ellipse>` und `<circle>` tragen Fläche;
  sie nicht zu zählen ließ 29 % der Formen von `mark.svg` unsichtbar fürs
  Budget. Neue Budgets: `full` ≤ 34, `reduced` ≤ 20, `micro` = 1.
- **V9 neu: Androids garantierte Fläche ist der 66-dp-Kreis**, nicht die
  72-dp-Zone. Nur er ist auf jeder Maskenform sichtbar.
- **V8 neu gefasst:** nicht „Vorlagentreue nach Augenschein", sondern die
  gemessene Deckung von farbigem Layer und Silhouette ≥ 0,90. Sonst zeigt
  derselbe Launcher je nach Einstellung zwei verschiedene Eulen.

### Erzeugung statt Pflege

- **`scripts/build-brand-assets.sh` erzeugt jede abgeleitete Datei** aus den
  Zeichnungen: App-Icon-Stufen, Symbolic, Android-Layer, Web-Set, Lockups,
  Fassung für dunkle Gründe. Vorher war das App-Icon eine Kopie der Marke,
  und die kleinen Stufen bekamen die Platte nie — zwei verschiedene Icons
  für dieselbe App.
- **`--check` erzeugt daneben und vergleicht.** Damit ist beweisbar, dass
  der Baum aus den Zeichnungen stammt und niemand eine abgeleitete Datei von
  Hand nachgebessert hat.
- **Androids Hintergrundebene ist ein Drawable mit Verlauf**, kein
  `@color`-Wert mehr, und wird aus derselben Plattendatei erzeugt wie die
  GNOME-Platte.

### Nachtrag: der Winkel der Ohrbüschel entscheidet über die Tierart

Nach der ersten Runde las die Marke weiterhin als Katze. Ich hatte die
Ursache bei den Augen vermutet — gekappte Ellipsen wirken wie schräge
Katzenaugen — und vier Fassungen gebaut: gefegte Büschel, runde Augen ohne
Büschel, runde Augen mit Büscheln, dasselbe mit flacher Braue. Am Bild
entschieden: **nur die Fassung mit gefegten Büscheln und gekappten Augen
liest als Eule.** Die runden Augen kippen die Marke zum Lemur.

Damit ist die Regel: **steil nach oben zeigende Dreiecke auf einem runden
Kopf sind Katzenohren, egal wie die Augen aussehen. Nach außen gefegte
Federspitzen auf einem oben schmalen Kopf sind Ohrbüschel.** Daraus folgen
drei Änderungen gegenüber der ersten Runde:

- **Kopf eiförmig**, Scheitel bei y=372 statt y=402 und oben schmaler. Ein
  oben breiter Kopf verschluckt die Büschel.
- **Büschel flach nach außen**, Enden bei (261|392) und (985|392) statt
  Spitzen bei (228|264). In `mark.svg` sind sie auf die Kopfbreite
  gestaffelt: weiter außen stoßen sie an die Ohrmuscheln, und Kopf plus
  Büschel werden zu einer Fläche, die als Flügel liest.
- **Schnabel größer und gehakt**, 134 × 226 statt 102 × 177. Er ist nach den
  Augen das zweitstärkste Eulensignal und trägt bis 32 px.
- **Gesichtsscheibe eingezogen.** Auf dem schmaleren Kopf lag ihre Kante auf
  der Silhouette; gemessen versanken dort 25 % des Markenrandes unter 1,5:1
  gegen die Platte. Nach dem Einzug sind es 0 %.

**V8 vergleicht jetzt Umrisse statt Flächen** (Schwelle 0,97 statt 0,90).
Der rohe Flächenvergleich bestrafte genau das, was an der Silhouette Absicht
ist: Augen und Schnabel sind dort Löcher und in der farbigen Fassung Flächen.
Gemessen wird, ob der Launcher dieselbe Eule zeigt — nicht, ob beide Fassungen
gleich viel Fläche haben.

### Nachtrag: eine Zeichnung statt drei, und ohne Kopfhörer

Der Stufenbau ist aufgelöst. `full`, `reduced` und `micro` sind verschwunden;
es gibt nur noch `mark.svg` und ihre einfarbige Fassung `mark-mono.svg`.
Die Kopfhörer sind ganz entfallen.

**Warum überhaupt drei Zeichnungen?** Weil die vorlagentreue Eule mit
Kopfhörern bei 48 px zerfiel. Nimmt man die Kopfhörer heraus und zeichnet
bewusst grob — Vorbild ist der Löwe von Brave —, trägt eine einzige Zeichnung
von 512 bis 16 px. Drei Zeichnungen zu pflegen war die Antwort auf ein
Problem, das die Vereinfachung gar nicht erst entstehen lässt.

Was der Verzicht kostet: die Kopfhörer waren das einzige Musiksignal im Bild.
Das trägt jetzt der Name.

**Die Zeichnung liegt auf dem 16er Raster**, und jede tragende Kante liegt auf
einer ganzen Rasterlinie: Augen als Kreise mit Radius 2 um (5|9) und (11|9),
Pupillen mit Radius 1, Schnabel von x=7 bis x=9. Damit sind 32, 48, 64, 128
und 512 px glatte Vielfache — die Zeichnung wird vergrößert, nicht
interpoliert. Auf halben Rasterlinien gezeichnet, wie im ersten Wurf, wird bei
16 px jede Kante zu einem grauen Verlauf über zwei Pixel.

**Die Augen sind Ringe, keine Löcher.** Das ist der Befund, an dem die
vorigen Fassungen gescheitert sind: einfarbig ist ein Auge, das nur ein Loch
ist, ein Katzenauge. Erst die Pupille darin macht es zum Eulenauge. In der
Farbfassung ist dieselbe Form eine Fläche mit Pupille, damit beide Fassungen
dieselbe Zeichnung zeigen — V8 misst das und ergibt 1,0000.

**Die Marke ist flach gefüllt.** Die Tiefe kommt von der Platte. Ein Verlauf
in der Marke hätte zwei Kosten: Androids VectorDrawable kennt keine
Verlaufsverweise, und bei 16 px ist ein Verlauf über drei Pixel Matsch.

#### Zwei Korrekturen am Messverfahren

- **V2 zählt Hintergrundflächen jetzt in 8er-Nachbarschaft.** Vordergrund und
  Hintergrund brauchen entgegengesetzte Nachbarschaften, sonst widersprechen
  sich die Zählungen: mit der 4er-Regel zerfiel ein einen Pixel dünner
  Augenring in vier Eckstücke, die einzeln unter die Mindestgröße rutschten.
  Die Messung meldete „keine Aussparung" für eine Zeichnung, deren Augen im
  Bild klar offen sind. Der Selbsttest (Klumpen = 1, Ring = 2) kalibriert
  weiterhin richtig.
- **V1 wird bei 128 px gemessen, nicht bei 16.** Die Frage ist geometrisch —
  füllt die Zeichnung ihr Raster aus? Bei 16 px entscheidet darüber die
  Kantenglättung: eine Federspitze von einer halben Rasterzeile Höhe landet
  unter der Alphaschwelle und fehlt in der Messung.
