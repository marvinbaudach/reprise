# Die Farbwolke driftet in zwei Schichten

Ersetzt den `CoverShimmer` im rechten Now-Playing-Panel durch eine driftende
Farbwolke aus zwei Schichten. Quelle: Design-Canvas `Cover Varianten.dc.html`,
Variante 2c, in der zweiten, auf das Desktop-Panel gemünzten Fassung.

## Entschieden

- **Farbquelle: der verwischte Cover, nicht extrahierte Swatches.** Die Spec
  verlangt drei dominante Coverfarben. Genau das hat dieses Repo schon einmal
  gemessen und verworfen (`cover_shimmer.rs:1-10`): die Hälfte der Cover ist
  grau oder nahezu schwarz und liefert keine Palette, der Rest ist meist
  monochrom — der Sweep wurde unsichtbar. Die Wolke erbt deshalb die honesty
  rule: die beiden Schichten malen den 32-px-Blur des Covers durch die radialen
  Masken der Spec. Form, Drift, Radien, Ausblendung und die 1-s-Überblendung
  bleiben wörtlich wie spezifiziert; nur die Füllung ist ehrlich.
- **Die Wolke ersetzt den Shimmer.** Der Shimmer ist der "cover-palette sweep" —
  dieselbe Rolle. Der Bloom bleibt und trägt die Audio-Kopplung weiter.
- **Das Panel-Layout bleibt.** Cover 168 px, Band 280 px. Die Spec-Maße sind für
  ein 240-px-Cover geschrieben und werden proportional übernommen: jede
  Wolkenlänge ist ein Vielfaches von `NOW_PLAYING_COVER_SIZE`, so wie der
  Shimmer seinen Durchmesser als `520/168` führt. Kein Eingriff in Bloom,
  CoverLift oder den Podcast-Texturcache.

## Maße, relativ zur Covergröße

Alle Spec-Werte durch 240 geteilt, damit das Layout sie tragen kann:

| Spec (240er Cover) | als Faktor | bei 168 px |
|---|---|---|
| Fläche 440 px hoch | 440/240 | 308 px |
| Überstand oben 60 px | 60/240 | 42 px |
| Überstand links 40 px | 40/240 | 28 px |
| Überstand rechts 90 px | 90/240 | 63 px |
| Unschärfe Schicht 1: 48 px | 48/240 | 34 px |
| Unschärfe Schicht 2: 54 px | 54/240 | 38 px |

Der Überstand ist rechts am größten: der Schwerpunkt liegt nach außen, weg von
der Trackliste.

## Die beiden Schichten

Schicht 1 (hinten), Unschärfe 48 px:
- Farbe 1 @ 40 %/35 %, Deckkraft 0.60, Radius 50 % auf transparent
- Farbe 2 @ 82 %/55 %, Deckkraft 0.55, Radius 50 % auf transparent
- Drift 16 s

Schicht 2 (davor), Unschärfe 54 px:
- Farbe 3 @ 75 %/25 %, Deckkraft 0.45, Radius 45 %
- Farbe 2 @ 30 %/80 %, Deckkraft 0.40, Radius 45 %
- Drift 20 s, rückwärts, um 10 s versetzt

"Farbe 1/2/3" sind die Punkte, an denen die Masken sitzen — gefüllt wird jede
mit dem verwischten Cover, nicht mit einer extrahierten Farbe.

## Die Drift

Eine Periode ist ein voller Hin-und-Zurück-Weg, nicht ein Weg. 16 s heißt also
16 s für die ganze Reise, nicht 32.

    p(t) = smoothstep(Dreieckswelle(t / Periode + Versatz))
    translate x: -10 %  ->   8 %
    translate y:  -6 %  ->   6 %
    scale:       1.30   ->  1.45
    rotate:         0°  ->    6°

`ease-in-out` je Hälfte wird als `smoothstep` (3u²-2u³) genähert; der Unterschied
zu `cubic-bezier(.42,0,.58,1)` liegt unter einem Pixel Weg.

16 s und 20 s haben das kleinste gemeinsame Vielfache 80 s — das Gesamtbild
wiederholt sich also alle 80 s, nicht früher. Der 10-s-Versatz auf Schicht 2 ist
eine halbe Periode und hält die Schichten dauerhaft gegenläufig.

## Ausblendung

Vertikaler Verlauf in der Panel-Hintergrundfarbe über der ganzen Wolkenfläche:

    0 % Höhe  -> Deckkraft 0.00
    40 % Höhe -> Deckkraft 0.15
    55 % Höhe -> Deckkraft 1.00

Ab 55 % der Fläche ist die Wolke vollständig gedeckt. Titel, Interpret, Lyrics
und Segment-Control liegen damit auf ruhigem Grund. Dark und Light fahren
dieselben Werte; nur die Ausblendfarbe wechselt, woraus im Lightmode von selbst
ein Farbschleier statt eines Leuchtens wird.

## Der Titelwechsel

Die Wolke schneidet nicht auf das neue Cover, sie dreht die Farbe: das
abgelöste Raster-Paar blendet unter dem neuen über eine Sekunde aus. Beide
Paare driften dabei auf derselben Uhr, so dass die Farbe überblendet und nicht
die Bewegung.

Linear, nicht geeast — die Paare liegen übereinander, und ein geeastes Paar
wäre in der Mitte beidseitig halb draußen, wo das Licht dann einbräche.

Ein Titelwechsel erreicht die Wolke in **zwei** Aufrufen: das Panel löscht das
Cover, sobald der Track wechselt, und die dekodierte Textur folgt, wenn der
Loader sie hat. Das Löschen ist es, was das ausgehende Paar zum Ausblenden
bereitstellt — der zweite Aufruf darf dieses Paar deshalb nicht seinerseits als
sein ausgehendes weiterreichen. Genau das tat die erste Fassung, warf damit das
Paar weg, von dem überblendet werden sollte, und ließ einen harten Schnitt
zurück. Die Entscheidung darüber ist als eigene Funktion `fade_step`
herausgezogen, damit sie ohne Widget prüfbar ist; die 22 Tests der ersten
Fassung sahen den Fehler nicht, weil sie nur die Arithmetik der Überblendung
prüften und nie die Zustandsmaschine dahinter.

Steht die Uhr (Panel eingeklappt oder Animation aus), gibt es keinen Frame, der
eine Überblendung tragen könnte: dann wird hart gewechselt, statt das
eintreffende Cover bei Deckkraft null hängen zu lassen. Das ausgeblendete Paar wird freigegeben, sobald es
nicht mehr gezeichnet wird — eine lange Warteschlange soll nicht pro Titel ein
totes Feld behalten.

## Kosten

Die Hausmethode des Blooms und des Shimmers: das Raster wird einmal pro Cover
gebacken, ein Frame kostet einen Transform und ein `paint_with_alpha`. Kein
`gsk::RadialGradientNode` — dafür gibt es im Repo keinen Präzedenzfall; die
Masken werden wie beim Shimmer als Cairo-Pattern in den Puffer gebacken.

## Bewegung aus

`crate::ui::motion::animations_enabled()` vor jedem Drift-Frame, wie
`scan_edge_line.rs:65`. Ist sie aus, friert die Uhr statt der Wolke: die
Komposition bleibt stehen, die Bewegung hört auf.

## Verdrahtung

`CoverShimmer` wird an fünf Stellen angefasst:
- `now_playing.rs:11,54,175,180,318` — Feld, Konstruktion, Overlay-Reihenfolge
- `now_playing_effects.rs:85,92,99,135,138,162` — `set_cover`
- `now_playing_light.rs:50,51,74,75,107` — `set_light`, `set_frame_time`,
  `set_pinned`

Die Wolke übernimmt `set_cover`, `set_frame_time` und `set_pinned`. `set_light`
entfällt: die Spec lässt die Bewegung ausdrücklich nur aus der Uhr kommen.

## Tests

Reine Funktionen, ohne Display, wie `bloom_falloff` und `shimmer_mask`:
- `drift_at(elapsed_s, period_s, offset_s, reverse)` — Endpunkte, Symmetrie,
  Periode als volle Reise, kein Sprung am Umlauf, keine Präzisionsdrift nach
  einem Tag
- die beiden Schichten halten nie gleichzeitig dieselbe Pose
- `scrim_alpha(y)` — die drei Stützstellen und die Monotonie dazwischen
- die Maßfaktoren gegen die Spec-Zahlen
- der `include_str!`-Test des Shimmers wandert mit: die Wolke darf keine
  Farbextraktion einführen
