# UI-Politur & Queue-Semantik — Beschlüsse (2026-07-18)

Sammel-Findings der Session, **gegen den Code geprüft**. Neue Regeln gehen als
**Sektion U** nach `docs/ux-rules.md` (S = STYLE-1, T = Netz-Opt-in auf
`feat/network-opt-in`). Regel-IDs sind append-only.

## Zuschnitt-Korrektur: was bereits erledigt ist

Der Sammel-Prompt listet sechs Blöcke. Vier davon sind heute schon gelandet und
werden **nicht erneut gebaut**:

| Geforderte Regel | Realer Stand |
|---|---|
| SEARCH-2 Streifen + Clamp | ✅ `1694634`, per Pixelmessung belegt |
| SEARCH-3 Chip + teal Lupe | ✅ `b117563` |
| SEARCH-4 Esc zweistufig | ✅ `f235d05` |
| QUE-1/2/3/5/6 | ✅ heute Abend, 9 Tasks |
| **ALB-1** persistenter Playing-Zustand | ✅ **GRID-1** `[aktiv]` — EQ-Badge + 1.5-px-Innenring, hover-unabhängig |
| **ALB-2** Enter/Ctrl+Enter/Space/Menütaste | ✅ **GRID-2** `[aktiv]` |
| **ALB-4** Bottom-Gradient statt Tooltip-Box | ✅ **GRID-4** `[aktiv]` |
| **ALB-5** Scroll zum spielenden Album + Puls | ✅ **GRID-5** `[aktiv]` |
| LYR-2/3 Opt-in + Aktivierungs-Leerzustand | ✅ auf `feat/network-opt-in` |
| STYLE-1 + RELEASING-„Schweben"-Test | ✅ `0d6d3079`, Sektion S |
| LYR-Center (Zentrierung an sich) | ✅ **NPP-6** `[aktiv]` |

Der komplette Album-Grid-Block kam mit `5402f34` (feat/keyboard-nav). Die
IDs `ALB-1`/`ALB-2` sind zudem **anders belegt** als im Sammel-Prompt gemeint
(ALB-1 ist ersetzt, ALB-2 ist die Album-Detail-Ansicht) — die neuen Regeln
bekommen deshalb eigene IDs.

## Die drei entschiedenen Umkehrungen

### 1 · QUE-8 — Reorder im Panel, mit geschärfter Grenze

Die alte Formulierung „Panel überfliegt, verwaltet nichts" war unehrlich:
Remove war im Panel bereits erlaubt und ist ebenfalls ein Verwaltungs-Verb. Die
tragfähige Grenze ist **leichte vs. schwere Verben**.

- **Panel „Up Next"**: Sprung, Remove, Reorder **innerhalb der manuellen
  Sektion**.
- **ColumnView „Queue"**: Multi-Select, Clear, Save-as-Playlist, Kontextmenü.

Drop-Targets existieren ausschließlich in „Next in Queue". Die
„Continuing"-Sektion ist nicht umsortierbar; ein Drag von dort nach oben
bedeutet „früher spielen" und materialisiert genau diesen einen Eintrag in die
manuelle Sektion. Begründung: Reorder ist die erwartete Geste genau dort, wohin
der Nutzer schaut — das Playerleisten-Icon öffnet das Panel, nicht die
ColumnView. Kostet Drop-Targets + Autoscroll.

### 2 · LYR-1 — vertagt, aus diesem Batch heraus

Lokale Songtext-Leserei (LRC aus Tags + `.lrc`-Sidecar) ist neue
Dateiformat-Arbeit, kein Opt-in-Detail. Bleibt als eigener `[geplant]`-Task.

**Konsequenz, die notiert gehört:** Für v1 ist der Lyrics-Tab damit
**netz-only**. Bei ausgeschaltetem Online-Toggle zeigt er konsequent die
StatusPage „In den Einstellungen aktivieren" — es gibt keinen lokalen Pfad, der
daneben noch etwas anzeigen könnte. Die Zusage „eingebettete Songtexte werden
immer angezeigt" steht **erst, wenn LYR-1 gebaut ist** und darf bis dahin
nirgends in der UI auftauchen (das war bereits Korrektur 1 im Netz-Opt-in-Plan).
LYR-Center gilt unabhängig davon für alles, was angezeigt wird.

### 3 · STYLE-3 — zwei Akzent-Rollen statt „alles teal"

Die ursprüngliche Vorgabe („Play-Button auf teal vereinheitlichen") war falsch
und wird verworfen. Der creme Knopf ist `@reprise_player_accent`, also die aus
dem Cover extrahierte Farbe — ein Feature (NPP-3), kein Ausreißer. Ihn
festzunageln hätte den dynamischen Akzent rückgängig gemacht.

Richtig sind **zwei klar getrennte Rollen**, jede in sich konsistent:

- **App-Akzent** (fixes Petrol, `@accent_color`): dauerhafte UI-Bedeutung —
  Selektion, Ratings-Sterne, Toggles `:checked`, Links, Chips, Fokusringe.
- **Playback-Akzent** (dynamisch aus dem Cover, `@reprise_player_accent`):
  alles, was den **laufenden Track** meint — Play/Pause-Button,
  Waveform-Fill/Seek, Playing-Row-Tint + EQ, Now-Playing-Glow, GRID-1-Innenring.

Play-Button und Waveform sind damit korrekt cover-farben. Die einzige echte
Regel: **nicht per Element mischen** — ein Element gehört zu genau einer Rolle.

## Neue Regeln (Sektion U)

- **SEARCH-6** — Lupe und Ctrl+F togglen beidseitig (zeigen ↔ verstecken). Die
  Query wird dabei **nie** gelöscht; beim Verstecken mit Inhalt lebt sie als
  Chip weiter (FIL-1) und die Lupe bleibt im `:checked`-Akzent.
  *Belegter Ist-Zustand:* `shortcuts.rs:192` ruft `set_search_mode(true)` —
  öffnet immer, schließt nie. Echte Lücke.
- **QUE-7** — Up Next = manuelle Queue + **virtueller Kontext-Tail**. Der Tail
  wird nicht als Einzelzeilen materialisiert, sondern als benannter
  Sektion-Header mit Count geführt („Playing from Music · 1.663 tracks").
  Gerendert wird nur das sichtbare Fenster (QUE-6). Die Sidebar-Zeile „Queue"
  zählt **nur die manuelle Queue**; bei 0 steht dort „Queue" ohne Zahl.
  *Belegter Ist-Zustand:* `window.rs:206` speist den Zähler aus
  `queue_pending_len()` — dadurch badge't die App faktisch die halbe Library
  als Warteschlange.
- **QUE-8** — Drag-Reorder ausschließlich in „Next in Queue" (siehe Beschluss 1).
- **LYR-4** — Die Zentrierung der aktiven Zeile wird **nach oben geklemmt**:
  solange nicht genug Kontextzeilen darüber liegen, sitzt der Text oben und
  wandert erst in die Mitte, sobald genug vorausgegangen ist.
  *Grund:* NPP-6 zentriert korrekt, aber am Songanfang gibt es nichts darüber —
  die obere Panelhälfte bleibt leer und liest sich als Layoutfehler.
- **STYLE-2** — Durchgängiges Elevation-System, einmal definiert und überall
  angewandt: Content/Tabelle = `.view` (dunkelster Ton), linke Sidebar und
  rechtes Now-Playing-Panel = Fenster-`.background` (eine Stufe heller),
  1-px-Hairlines an den Innenkanten. Kein Per-Pane-Nachtönen.
- **STYLE-3** — Zwei Akzent-Rollen (siehe Beschluss 3).
- ~~**FMT-1**~~ — **entfällt, bereits erfüllt.** `reprise_core::format::`
  `format_thousands` ist eine einzige geteilte Funktion; sie versorgt
  `status_bar.rs:135`, `browse_filter_strings.rs:59`, `browse_bar.rs:530`,
  `sidebar_presentation.rs:8` und die Up-Next-Fußzeile. Es gibt keinen zweiten
  Pfad.
  *Korrektur eines Fehlbefunds:* Eine frühere Fassung dieses Dokuments
  behauptete einen Widerspruch zwischen „1,638" (Statuszeile) und „1.652"
  (Up Next). Das war falsch — die Punkt-Schreibweise stammte aus der deutschen
  Notation der Anforderung, nicht aus dem Code. Die App läuft auf Englisch,
  Komma ist dort korrekt, und beide Stellen rufen dieselbe Funktion. Der
  Befund war aus der Spezifikation abgeschrieben statt am Code geprüft.
- **NPP-11** — Ansichts-Tabs zentriert als `AdwViewSwitcher`-Title-Widget, mit
  adaptiver Degradierung bei schmalem Fenster (`AdwViewSwitcherBar` unten oder
  `AdwInlineViewSwitcher` icons-only per `AdwBreakpoint`).
  *Kehrt die frühere Links-Entscheidung um.* Der damalige Grund — ein starres
  Center-Widget reserviert symmetrisch `2×max(links, rechts)` und quetscht
  schmale Fenster — ist halb entfallen (die Suche ist jetzt eine SearchBar
  unter der Headerbar, die Mitte ist frei) und für den Rest durch die
  Squeeze-Fähigkeit des Switchers neutralisiert. Der STYLE-1-Mindestbreiten-
  Befund bleibt damit abgedeckt.

## Text-Kontrast (CONTRAST-1..3)

### Ist-Zustand, auditiert — der Befund ist schärfer als gemeldet

Die Statuszeile ist **keine Leiste**. `track_content.rs:10-16` baut ein
`gtk4::Overlay` und hängt das Label per `add_overlay` direkt über die
scrollende Trackliste. Es hat keine Fläche, keinen Hintergrund, keinen
Container — es schwebt über dem Content.

Daraus folgt der eigentliche Defekt: Der Untergrund ist **nicht
unterbestimmt, sondern nicht-deterministisch**. Unter dem Label scrollt
wechselnd normale Zeile, Zebra-Tönung, Selektionsblock und Playing-Row-Tint
durch. Der Kontrast ändert sich beim Scrollen. Ein fester Alpha-Wert kann
gegen einen wandernden Untergrund kein Verhältnis garantieren.

Zweite Folge, im Screenshot belegt: `add_overlay` **reserviert keinen Platz**.
Das Band liegt auf der letzten Trackzeile („Hole Hearted" ist angeschnitten),
statt unter ihr zu stehen. Die unterste Listenzeile ist damit dauerhaft halb
verdeckt — verlorener Inhalt, unabhängig von der Scrollposition und von jeder
Farbwahl. Der Umbau in eine echte Leiste behebt das mit.

Dritte Korrektur: Das Label nutzt `.dim-label` + `.caption`
(`status_bar.rs:56-57`) — keine eigenen Alphas. `.dim-label` ist Adwaitas
**normale** Sekundärstufe, nicht die schwächste Hint-Stufe. Der verstärkende
Faktor ist `.caption`: kleine Schrift bei gedimmter Deckkraft. WCAG verlangt
4.5:1 für normalen Text und lässt 3:1 nur für großen zu — klein *und* dim ist
der ungünstigste Fall, nicht bloß ein zu niedriger Alpha-Wert.

### Beschlossene Regeln

- **CONTRAST-1** — Drei Textstufen, einmal definiert, überall angewandt:
  Primär ~`0.95` (Titel, Track-Namen, Werte), Sekundär ~`0.7` (Artist-Zeilen,
  Statuszeilen, Metadaten, Spaltenköpfe), Hint ~`0.5` (Platzhalter,
  Hinweiszeilen, deaktivierte Sekundärtexte). Kein Per-Element-Nachtönen.
  Wo Adwaita-Named-Colors passen (`@window_fg_color`, `.dim-label`), diese
  nutzen statt eigener Alphas — dann greifen Theme-Kontraste automatisch.
  **Die Stufe gilt zusammen mit der Schriftgröße**: `.caption` + Sekundär
  braucht dieselbe Prüfung wie Hint bei Normalgröße.
- **CONTRAST-2** — Die Statuszeile bekommt **zuerst eine definierte Fläche**,
  dann den Ton. Die Reihenfolge ist keine Kosmetik: solange das Label über dem
  Content schwebt, existiert keine Untergrundfarbe, gegen die sich 4.5:1
  überhaupt behaupten ließe. Also aus dem `Overlay` heraus in eine echte
  untere Leiste mit eigener Fläche und Hairline (STYLE-2), erst danach von
  Hint auf Sekundär heben. Gilt gleichlautend für alle
  „N tracks · Dauer"-Fußzeilen (Library, Playlist, Queue, Album-Detail) —
  eine gemeinsame Komponente, ein Ton.
- **CONTRAST-3** — Nach der Elevation-Umstellung (STYLE-2) alle Dim-Texte
  gegen ihren **neuen** Untergrund gegenchecken: Statuszeilen, Spaltenköpfe,
  Sidebar-Sektionslabels, Metazeilen in Karten. Wo < 4.5:1 → auf Sekundär bzw.
  passende Named-Color.

Test: `contrast_1_secondary_text_meets_ratio` [gtk] misst Alpha bzw.
Named-Color gegen die Surface-Farbe, nicht das Rendering — und ist erst
aussagekräftig, wenn CONTRAST-2 eine Surface-Farbe geschaffen hat. Die
visuelle Endabnahme bleibt `[manuell]` in `RELEASING.md`: Sichtprüfung der
vier Fußzeilen + Sidebar-Labels.

**Abhängigkeit:** CONTRAST-2/3 laufen *nach* STYLE-2, nicht parallel dazu.
Andernfalls wird der Ton ein zweites Mal gegen einen Untergrund getunt, der
sich gleich wieder verschiebt — exakt der Fehler, der den Befund erzeugt hat.

## NAV-10 — Ansichtsübergreifender Kontext-Anker

### Blocker: NAV-5 ist nicht gebaut

**NAV-5 steht auf `[geplant]`** (`ux-rules.md:113`) — das Modus-Gedächtnis für
Scroll und Selektion je Ansicht existiert nicht. NAV-10 Teil 2 beruft sich
darauf wörtlich („bei jedem weiteren Wechsel stellt NAV-5 die gemerkte
Position wieder her").

Ohne NAV-5 gibt es keine gemerkte Position. Damit ist **jeder** Eintritt ein
Ersteintritt, und NAV-10 degeneriert zu hartem Auto-Folgen bei jedem
Ansichtswechsel — exakt das Verhalten, das die Ausgangslage ausschließt. Der
Test `nav_10_subsequent_switch_restores_remembered_position` lässt sich vorher
nicht einmal sinnvoll formulieren, weil es nichts zu restaurieren gibt.

**Folge: NAV-5 ist Vorbedingung, nicht Nachbarregel.** Es wird im selben Batch
zuerst gebaut und auf `[aktiv]` gehoben; NAV-10 setzt darauf auf.

Dazu gehört eine Präzisierung, die im Sammel-Prompt unter NAV-10s
„Abgrenzungen" steht, aber sachlich **NAV-5 spezifiziert**: Der Scroll-Anker
wird als **Track-/Album-ID + Offset** gemerkt, nicht als Pixelwert, damit
Re-Sort und Insert die Position halten (ohne `scrollIntoView`). Das gehört in
NAV-5s Text, bevor NAV-5 implementiert wird — nachträglich ist es ein Umbau.

### Teil 1 („immer markiert") ist ungleich abgedeckt

- **Albums**: ✅ GRID-1 `[aktiv]` — EQ-Badge + Innenring, hover-unabhängig.
- **Tracks**: Markierung vorhanden (Akzent-Row + EQ).
- **Artists**: ❌ ART-1 `[geplant]` — „spielender Artist zeigt nur Mini-EQ"
  ist unbebaut.
- **Playlists**: ❌ keine Regel vorhanden.

Zusätzlich: GRID-1 spricht vom „**gemeinsamen** EQ-Badge", aber es gibt keine
geteilte Komponente — die einzige Implementierung sitzt in `album_card`
(`album_card_tests.rs:134`), der Mini-EQ der Playerleiste ist ein zweiter,
eigener Pfad. „Eine Markierungssprache" (ALB-2) verlangt hier also eine
**Extraktion**, nicht bloß eine Anwendung auf zwei weitere Ansichten. Das ist
der eigentliche Aufwand in Teil 1.

### Teil 3 ist für Alben bereits erledigt

Das explizite Reveal ist **GRID-5 `[aktiv]`**: Aktivierung von Cover oder
Titel in Playerleiste oder Panel wechselt in die Album-Ansicht, leert Suchfeld
und Albumfilter, scrollt per Adjustment (ausdrücklich ohne `scrollIntoView`),
fokussiert und hebt rund 1 s hervor, mit NAV-9a als Fallback. Der Sammel-Prompt
verweist auf „ALB-5" — das ist die alte Nummerierung derselben Sache.

Offen bleibt hier nur die **Artist-Richtung** („Go to artist") und der
Kontextmenü-Einstieg, sofern er nicht schon über GRID-2 abgedeckt ist.

### Beschlossene Regel

- **NAV-10** — Drei Teile wie spezifiziert: persistente Markierung in allen
  Ansichten (Playback-Akzent-Rolle nach STYLE-3, cover-dynamisch);
  Auto-Scroll auf den laufenden Kontext **nur beim ersten Betreten** einer
  Ansicht in der Session, danach NAV-5-Restauration ohne Yank; explizites
  Reveal (Now-Playing-Cover/Titel, „Go to album/artist") springt immer und
  deterministisch. Selektion folgt nie der Wiedergabe; der Kontext eines
  geklickten, nicht spielenden Songs ist ausschließlich über „Go to
  album/artist" erreichbar. Playing-Marker und Selektions-Highlight bleiben
  getrennte Treatments in allen Ansichten.

Tests: `nav_10_first_entry_lands_on_playing_context`,
`nav_10_subsequent_switch_restores_remembered_position`,
`nav_10_playing_marked_in_all_views`, `nav_10_reveal_always_jumps` [gtk].

**Reihenfolge im Batch:** NAV-5 (inkl. ID+Offset-Anker) → Badge-Extraktion →
ART-1/Playlist-Markierung → NAV-10.

## Bugfix ohne eigene Regel

**Scroll-Sprung bei Tabellen-Aktivierung.** Doppelklick auf eine Zeile scrollt
die Tabelle an den Listenanfang.

*Ursache (diagnostiziert, nicht vermutet):* `invalidate_window_at`
(`track_list_model.rs:380`) feuert `items_changed(position, 1, 1)` und erzeugt
damit das fokussierte Zeilen-Widget neu. GTKs Fokus-Wiederherstellung scrollt
daraufhin selbsttätig. Für den zentrierenden Pfad ist das gelöst (synchrones
Zentrieren im selben Frame — der Kommentar in `current_track_selection.rs:310`
beschreibt es), der **unterdrückte** Pfad kehrt aber vorher zurück
(`if suppress_scroll { return; }`). Damit fällt der Fokus auf den Listenanfang.
Das erklärt, warum es *immer* nach ganz oben springt statt zum spielenden Track.

*Fix:* Im unterdrückten Pfad die Scroll-Position vor dem Invalidieren sichern
und danach zurückschreiben — also nicht „nicht zentrieren", sondern „aktiv da
bleiben, wo man war". Regressionstest mit exakter Viewport-Position, nicht mit
„kein Scroll-Aufruf" (STYLE-1: bei Geometrie zählt das Ergebnis).

## Abnahme

Ctrl+F togglet zu und auf, Query überlebt als Chip · Sidebar „Queue" zeigt die
manuelle Queue (meist ohne Zahl), Up Next führt den Kontext als benannte Zeile
statt als 1.649 Einträge · DnD nur in „Next in Queue" · Lyrics starten oben
statt in der Mitte · Panel ↔ Tabelle durch Ton + Hairline getrennt · App-Akzent
und Playback-Akzent nirgends im selben Element gemischt ·
Doppelklick in der Tabelle bewegt den Viewport nicht ·
Statuszeile auf eigener Fläche klar lesbar, Kontrast beim Scrollen konstant ·
kein Dim-Text unter 4.5:1 nach der Elevation-Umstellung.
