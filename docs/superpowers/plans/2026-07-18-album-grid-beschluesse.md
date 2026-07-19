# Album-Grid — Beschlussdokument (Grilling 2026-07-18)

Normativer Kontext fuer den Umbau des Album-Grids nach Design **24**. Dieses
Dokument haelt die gemeinsam bestaetigten Produktentscheidungen und deren
Begruendung fest. Waehrend der Umsetzung werden die Regeln in
`docs/ux-rules.md` ueberfuehrt; ab diesem Zeitpunkt gewinnt bei Abweichungen
immer das UX-Regelwerk.

## Regel-IDs und append-only Migration

Die im ersten Entwurf genannten IDs `ALB-1` bis `ALB-5` koennen nicht
wortgleich verwendet werden: `ALB-1` und `ALB-2` existieren bereits. Die
bestaetigte kanonische Zuordnung ist deshalb:

- `ALB-1` wird `[ersetzt durch GRID-2/GRID-4]`. Sein bisheriger Hover-,
  Aktivierungs- und Kontextmenue-Inhalt geht in die praeziseren Regeln auf.
- `ALB-2` bleibt unveraendert geplant; die Album-Detailansicht ist nicht Teil
  dieser Stufe.
- `NAV-9` wird `[ersetzt durch NAV-9a/GRID-5]`: `Ctrl+L` behaelt den
  Track-Ursprung-Sprung, Cover/Titel in Playerleiste und Now-Playing-Panel
  erhalten die neue Album-Grid-Semantik.
- Neue Regeln sind `GRID-1` bis `GRID-5`. Die im Auftrag genannten
  Testnamen werden entsprechend als `grid_*` statt `alb_*` angelegt, damit
  `scripts/check-ux-traceability.sh` die richtigen IDs prueft.

## Verbindlicher Regelwortlaut fuer `docs/ux-rules.md`

Die Implementierung uebernimmt diesen Inhalt zunaechst als `[geplant]` und
flippt jede Regel erst atomar mit Verhalten und regelbenanntem Test auf
`[aktiv]`.

- **GRID-1** `[geplant] [gtk]` — Persistenter Playing-Zustand: Das geladene
  Album zeigt unabhaengig von Hover und Fokus oben links auf dem Cover das
  gemeinsame EQ-Badge und einen 1.5-px-Innenring um das Cover. Beides nutzt
  `@reprise_player_accent`. Bei Pause bleibt der Ring und die EQ-Bewegung
  friert ein; bei `gtk-enable-animations=false` ist die Glyphe statisch.
- **GRID-2** `[geplant] [gtk]` — Bedienung und Aktionen: Das native
  `GtkGridView` bewegt den Fokus mit Pfeiltasten zweidimensional. `Enter`
  oeffnet die Album-Detailquelle als History-Push, `Ctrl+Enter` ersetzt die
  Queue durch das Album in kanonischer Disc-/Track-Reihenfolge und startet
  bei Track 1. `Space` bleibt global Play/Pause. Menue-Taste und `Shift+F10`
  oeffnen an der fokussierten Kachel dasselbe Menue wie Rechtsklick, exakt mit
  `Play`, `Play next`, `Add to queue`, `Go to artist`, `Edit tags...`.
- **GRID-3** `[geplant] [gtk]` — Sichtbarer Fokus und Zustandskomposition:
  Tastaturfokus zeichnet einen 2-px-Aussenring in `@accent_color` nur um das
  Cover und zeigt dieselbe Play-Affordance wie Hover. Playing, Fokus und
  Hover bleiben getrennte Zustandslayer: Playing innen, Fokus aussen,
  Interaktions-Overlay darueber; kombinierte Zustaende verdecken einander
  nicht.
- **GRID-4** `[geplant] [gtk]` — Bottom-Gradient-Overlay: Hover oder Fokus
  blendet statt einer schwebenden Tooltip-Box einen unten verankerten
  Abdunkel-Gradienten ein. Darin stehen eine duenne Metazeile
  (`13 tracks · 47 min`) und unten rechts ein Play/Pause-Button in
  `@reprise_player_accent`; Album und Artist bleiben unter dem Cover. Die
  Covermitte bleibt frei. Der Kartencontainer hat keinen Metadaten-Tooltip;
  nur tatsaechlich ellipsierte Titel-/Artist-Labels zeigen ihren Volltext.
- **GRID-5** `[geplant] [gtk]` — Spielendes Album aufdecken: Aktivierung
  von Cover oder Titel in Playerleiste oder Now-Playing-Panel wechselt bei
  Bedarf in die Album-Ansicht, leert ein sichtbares Suchfeld samt Albumfilter,
  scrollt per `GtkGridView`/Adjustment zur geladenen Albumkachel, fokussiert
  sie und hebt sie rund 1 s hervor. Der Ortswechsel ist ein History-Push;
  bereits im Album-Grid entsteht kein Duplikat. Fehlt die Albumkachel, greift
  `NAV-9a` ohne Fehlerdialog. `gtk-enable-animations=false` zeigt fuer dieselbe
  Dauer ein statisches Highlight.
- **NAV-9a** `[geplant] [gtk]` — Zum spielenden Track: `Ctrl+L` navigiert
  weiterhin zur Herkunftsansicht des geladenen Tracks, selektiert dessen
  Zeile und zentriert sie; Back kehrt zum vorherigen Ort zurueck.

## Gegrillte Detailbeschluesse

### Playing und Primaeraktion

1. Das EQ-Badge sitzt **oben links** als eigener persistenter Overlay-Layer.
   Es wird nicht in den Hover-Gradienten verschoben oder von ihm geclippt.
2. Ringe umfassen nur das Cover, nie Titel oder Artist: Playing innen 1.5 px,
   Fokus aussen 2 px. Beide bleiben gleichzeitig unterscheidbar.
3. Ring, EQ und Play/Pause-Button nutzen die cover-abgeleitete Farbe
   `@reprise_player_accent`; der Fokus nutzt bewusst das Theme-
   `@accent_color`.
4. Der Overlay-Button ist pointer-only und kein zusaetzlicher Tab-Stopp.
   Seine Tastaturentsprechung ist `Ctrl+Enter`; `Space` wird nie konsumiert.
5. Primaerbutton-Semantik:
   - anderes Album oder Playback `Stopped`: kanonische Albumqueue neu bauen,
     bei Track 1 starten;
   - geladenes Album + `Playing`: pausieren, Queue und Position behalten;
   - geladenes Album + `Paused`: fortsetzen, Queue und Position behalten.
6. `Ctrl+Enter` und Kontextmenue-`Play` sind dagegen immer explizites
   Container-Play: auch beim geladenen Album Queue kanonisch neu bauen und
   Track 1 starten.

### Kanonische Albumreihenfolge

7. Schema v12 fuegt `tracks.disc_no INTEGER NULL` hinzu. Der Scanner liest
   die Disc-Nummer aus Tags und bewahrt sie bei Move/Reconcile und normalen
   Tag-Edits.
8. Reihenfolge: `disc_no` aufsteigend mit `NULL` als Disc 1, dann
   `track_no` aufsteigend mit `NULL` zuletzt, danach stabiler Pfad und ID.
9. Altbestand bleibt kompatibel. Reale Disc-Tags gelangen erst durch einen
   spaeteren normalen Scan in die DB; diese Stufe startet keinen Scan gegen
   Nutzerdaten und fuehrt keine Datei-Migration aus.
10. `disc_no` ist in dieser Stufe ausschliesslich Persistenz-/Ordnungsdaten.
    Es entsteht kein neues sichtbares Feld im Tag-Editor.

### Kontextmenue und Navigation

11. Pointer- und Tastaturmenue sind identisch und enthalten exakt fuenf
    Eintraege: `Play`, `Play next`, `Add to queue`, `Go to artist`,
    `Edit tags...`. Shuffle, Playlist-Untermenue, New Playlist und
    Go-to-folder entfallen aus dem Album-Menue.
12. `Play next` fuegt alle vorhandenen Albumtracks in kanonischer Reihenfolge
    direkt hinter dem aktuellen Track ein. `Add to queue` haengt sie in
    derselben Reihenfolge an.
13. `Go to artist` wechselt gemaess NAV-3 in die Artist-Ansicht, selektiert
    den Album-Artist und legt den Ausgangsort in die History.
14. `Edit tags...` oeffnet den vorhandenen Batch-Tag-Editor fuer alle
    vorhandenen Tracks des Albums; fehlende Dateien werden entsprechend der
    bestehenden CTX-/TAG-Regeln ausgelassen.
15. „Push" meint in der bestehenden Architektur den kanonischen globalen
    `NavHistory`-Push auf `ViewSource::Album` mit anschliessendem Back ueber
    Alt+Links/Maus-Back/Header-Back. Das vorhandene `adw::NavigationView`
    enthaelt nur die Library-Shell und fuehrt derzeit keinen eigenen
    Detailseiten-Stack; fuer diese Stufe wird kein zweiter konkurrierender
    History-Stack erfunden. Die visuelle Hero-Detailseite bleibt `ALB-2`.

### Overlay und Tooltips

16. Der Bottom-Gradient beginnt transparent und dunkelt nur nach unten hin
    merklich ab. Metazeile und Button liegen im unteren Bereich; die Mitte
    traegt keine Box.
17. Es gibt keinen Tooltip am gesamten Cover oder Kartencontainer und keine
    Metadaten-Dopplung. Die bestehende TIP-1a-Ausnahme wird ueber den bereits
    erprobten `query-tooltip`-Ansatz fuer **nur tatsaechlich ellipsierte**
    Titel-/Artist-Labels wiederverwendet.
18. Der Icon-only-Button nennt im Tooltip und Accessible-Text die Aktion samt
    Tastaturalternative (`Play album (Ctrl+Enter)` beziehungsweise im
    Toggle-Zustand `Pause album`/`Resume album`).

### Reveal aus Playerleiste und Panel

19. Playerleisten- und NPP-Cover sowie deren Titel sind fokussierbare,
    link-artige Aktivierungsflaechen. Klick und `Enter` loesen `GRID-5` aus,
    `Space` propagiert zur globalen Transportaktion. Accessible Name:
    `Reveal playing album`.
20. Ist ein Suchtext aktiv, wird er **sichtbar** aus dem Suchfeld entfernt;
    der Albumfilter wird nicht unsichtbar umgangen. Back stellt den vorherigen
    Ort wieder her, aber nicht den geloeschten Suchtext.
21. Der Reveal fokussiert die Zielkachel. Nach dem rund einsekundigen Puls
    bleibt deshalb der normale Fokus-Ring sichtbar.
22. Animationen an: zwei weiche Akzent-Helligkeitszyklen in rund 1 s.
    Animationen aus: statische Akzent-Hervorhebung rund 1 s, danach harter
    Wechsel. Der Puls ersetzt weder Playing- noch Fokus-Ring.
23. Kann kein Album aufgedeckt werden (leeres Album, fehlender Eintrag,
    nicht materialisierbare Kachel), faellt der Ablauf auf `NAV-9a` zurueck.
    Kein Dialog und kein toter Klick.

## Nicht Teil dieser Stufe

- Gestaltung der eigentlichen Album-Detailansicht (`ALB-2`).
- Sichtbares Disc-Feld oder Disc-Editing im Tag-Editor.
- Neue Shuffle-/Playlist-/Dateimanager-Aktionen im Album-Menue.
- Veraenderung der globalen `Space`-, `Ctrl+L`- oder Back-Semantik ausser der
  dokumentierten Aufteilung `NAV-9` → `NAV-9a/GRID-5`.
- Scannen der realen Musikbibliothek oder Zugriff auf die reale Reprise-DB.

## Manuelle Abnahme

- Spielendes Album ohne Hover beim Scrollen: EQ + Innenring bleiben sichtbar;
  Pause friert nur den EQ ein.
- Pfeilnavigation: echter 2D-Fokus, Aussenring und Overlay erscheinen; Playing
  + Fokus zeigt beide Ringe.
- Enter oeffnet Detail, `Ctrl+Enter` startet das Album bei Track 1, `Space`
  pausiert global.
- Hover/Fokus zeigt Bottom-Gradient, Metazeile und petrolfarbenen Button, nie
  eine mittige Tooltip-Box.
- Playerleisten-/NPP-Cover oder Titel aktivieren: Album-Grid wird sichtbar,
  Suche leert sich sichtbar, Karte wird fokussiert und pulst; Back kehrt zum
  Ausgangsort zurueck.
