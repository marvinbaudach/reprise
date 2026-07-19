# Reprise — UX-Regelwerk (verbindlich)

Dieses Dokument ist die einzige UX-Wahrheitsquelle des Projekts. Bei Konflikt
schlägt es den Bestandscode — für `[aktiv]`-Regeln sofort (Abweichung = Bug),
für `[geplant]`-Regeln bindet es jede künftige Arbeit an diesem Bereich.
Akzeptanztests referenzieren Regeln über ihre IDs.

## Prozessregeln

**Status.** Jede Regel trägt genau einen Status:
`[aktiv]` = einklagbar — der Code ist konform, ein regelbenannter Test ist
grün und Merge-Blocker. `[geplant]` = beschlossenes Zielbild, noch nicht
einklagbar. Eine Regel wechselt auf `[aktiv]` **im selben Commit**, der das
Verhalten implementiert bzw. den Test nachweist — nie nachträglich. Halb
umgesetzte Regeln werden sofort in Teilregeln gesplittet (a/b), damit kein
Test je gegen eine halbe Regel geschrieben wird.

**IDs.** IDs sind append-only: nie umnummerieren, nie wiederverwenden. Ändert
sich die Bedeutung, ersetzt eine neue (Teil-)Regel die alte; die alte bleibt
als `[ersetzt durch <ID>]` stehen — als Wegweiser, nie als löschbarer
Ballast. Tests gegen ersetzte Regeln werden im selben Commit umgehängt.

**Test-Ebenen.** Jede Regel trägt ein Ebenen-Tag: `[core]` (reprise-core,
Workspace-Suite), `[gtk]` (Widget-/Logik-Tests in reprise-gnome), `[e2e]`
(cua-e2e-Harness gegen die echte App), `[manuell]` (RELEASING.md-Checkliste,
die dieselben Regel-IDs referenziert). Getestet wird auf der **niedrigsten
Ebene, die die Regel widerlegen kann**. Timing-Zahlen (100 ms, 150 ms, …)
sind Design-Intent, keine Assertions: das *Was* (Feedback existiert) wird
automatisiert, das *Wie-schnell* manuell geprüft. Wird eine `[manuell]`-Regel
später automatisierbar, wechselt nur ihr Tag, nie ihre ID.

**Traceability.** Ein Test trägt **genau eine primäre Regel-ID im Namen**
(Rust: `fn play_1a_…`, cua-e2e-Szenario: `play-1a-…`). Deckt ein Szenario
nebenbei weitere Regeln ab, zählt das nicht — die Zweitregel braucht ihren
eigenen Test. `#[ignore = "UX <ID> [geplant] — …"]` ist nur auf
`[geplant]`-Regeln erlaubt. `scripts/check-ux-traceability.sh` (Teil des
Merge-Gates) erzwingt: jede `[aktiv]`-Regel hat ≥ 1 Test — bei `[manuell]`
stattdessen eine wörtliche ID-Referenz in `RELEASING.md` · weder Test noch
Checkliste referenzieren eine unbekannte oder ersetzte ID · kein
Deaktivierungs-Ignore auf `[aktiv]` · jedes Deaktivierungs-Ignore auf einem
regelbenannten Test hält das Format oben ein. Als Abdeckung zählen nur
echte `#[test]`-Funktionen bzw. ausgeführte cua-e2e-Zeilen — eine
gleichnamige Helper-fn oder ein Kommentar greent das Gate nicht. Der
Display-Runner-Marker `#[ignore = "requires a display; run via xvfb-run"]`
ist kein Deaktivierungs-Ignore: solche Tests laufen als Merge-Blocker über
`scripts/check-display-tests.sh --rule-named` und zählen als Abdeckung auch
für `[aktiv]`-Regeln.

**Erreichbarkeit.** Für jede Aktion, deren Sichtbarkeit an einen Zustand
gebunden ist, gilt die Prüffrage: **Wie kommt der Nutzer in den Zustand, der
sie zeigt?** Ist die Antwort „über genau diese Aktion" oder „gar nicht", ist
die Regel unvollständig — unabhängig davon, wie korrekt jede Einzelbedingung
ist. Ein regelbenannter Test muss den Weg **vom Startzustand aus** gehen, nicht
den Zielzustand herstellen und dann prüfen. Zwei Befunde haben das erzwungen:
„Hide" war nur im Digest erreichbar, der nur bei Überlauf erschien; und New
Releases konnte sich nie befüllen, weil ✦ Einträge voraussetzt, „Fetch now"
hinter ✦ liegt und kein Start-Abruf existierte (NR-8). Beide Male waren alle
Einzeltests grün — der Fehler saß zwischen den Regeln, weil jeder Test den
Zielzustand vorab herstellte.

**Sprache.** Dieses Dokument und die Design-Docs sind Deutsch — die
Arbeitssprache des Projekts. Tests und Skripte sind Code und damit Englisch
(AGENTS.md); Regel-IDs und Status-Token werden dort wörtlich zitiert.

**Änderungen.** Begegnet dir beim Implementieren oder Testen ein Fall, den
keine Regel deckt: **Regel ergänzen, nicht lokal entscheiden.** Agenten legen
dazu einen `[geplant]`-Entwurf mit der nächsten freien ID im betroffenen
Abschnitt an, markiert mit `<!-- REVIEW: Regelvorschlag -->` — der Beschluss
fällt beim Menschen. Begründungen für Änderungen leben in der Git-Historie.

## A. Grundprinzipien

- **P-1** [geplant] [manuell] — Jede Feedback-Rolle hat genau einen
  Mechanismus: Ankündigung eines Ereignisses = Toast · Zustand einer View =
  StatusPage/Inline · laufender Prozess = Fortschrittskarte · offene Bitte =
  Badge. Ein Ereignis darf mehrere Rollen gleichzeitig bedienen (disconnect →
  Toast + StatusPage), aber nie zwei Mechanismen in derselben Rolle (nie zwei
  Toasts, nie Toast + Dialog als doppelte Ankündigung).
- **P-2** [geplant] [gtk] — Klick reagiert sofort: Jeder Klick erzeugt
  sichtbares Feedback (Zustandswechsel, Spinner im Button, Selektion), Ziel
  < 100 ms. Nie ein Klick ins Leere. Automatisiert wird das *Was* (Zustand
  nach Klick ≠ Ausgangszustand); die 100 ms sind manuelle Checkliste.
- **P-3** [geplant] [gtk] — Hover navigiert nie: Hover zeigt (Tooltip, Pills,
  +3 % Fläche), Klick handelt. Kein Hover-to-open.
- **P-4** [geplant] [manuell] — Nichts verschiebt sich ungefragt:
  Layout-Shifts nur als direkte Folge einer Nutzeraktion oder eines von ihm
  gestarteten Prozesses (Sync-Removals kollabieren, Force-show der
  Filter-Zeile bei eigener Sucheingabe, FIL-2). Einblendungen (Gerätekarte,
  ISSUES, Chip-Inhalte der Filter-Zeile) faden ohne Reflow benachbarter
  Inhalte; für dynamisch erscheinende Elemente (Altwert-Zeile im Tag-Editor)
  ist Platz reserviert. Hintergrundereignisse (Scan, Watcher, Mount)
  verschieben nie sichtbare Inhalte unter dem Cursor.
- **P-5** [geplant] [core] — Die App löscht nie Dateien. „Remove" heißt
  immer: Library-Eintrag. Dialoge benennen Kaskaden (Ratings, Hörhistorie)
  beim Namen.
- **P-6** [aktiv] [core] — Evidenz-Regel: Was beweisbar da ist, wird
  angezeigt/geheilt (Mount-Event, Resurrect); was beweisbar weg ist, wird
  sofort ehrlich markiert (Eject). Vermutungen (unmounted) sind nie
  Lösch-Grundlage.

## B. Navigationsmodell

- **NAV-1** [geplant] [gtk] — Sidebar = Orte, Content = Modus. Sidebar wählt
  den Ort (Music, Queue, Playlists, My Stats, Devices, Issues). Innerhalb von
  „Music" schaltet der Switcher den Modus: Tracks | Albums | Artists.
- **NAV-2** [geplant] [core] — Ein globaler History-Stack über den gesamten
  Content-Bereich, auch über Ortsgrenzen (Queue → Artist-Detail → Back →
  Queue). Content-Klicks (NAV-3) pushen immer; Alt+← / Maus-Back / Header-‹
  poppen. Sidebar-Klick ersetzt den Stack (Orte sind Neustarts, keine
  Stack-Einträge). Die Sidebar-Markierung folgt dem obersten Stack-Eintrag —
  zeigt der Stack Artist-Detail nach Queue-Klick, ist „Music" markiert.
- **NAV-2a** [geplant] [core] — Der Stack überlebt die Session nicht
  (Session-Restore stellt nur die oberste Ansicht wieder her, START-1
  unverändert); Back ohne Stack-Einträge ist disabled, nie ein No-op.
- **NAV-3** [geplant] [e2e] — Klickbare Metadaten überall gleich: In jeder
  Trackliste (Library, Playlist, Queue, Album-Detail, Top Tracks) gilt: Klick
  auf Artist-Namen → Artist-Detail; Klick auf Album-Namen/Cover →
  Album-Detail; beide pushen den globalen Stack (NAV-2). Hover zeigt
  Unterstreichung als Affordance. Gilt auch in der Player-Leiste (dort:
  Artist-/Album-Klick gemäß dieser Regel, Cover/Titel gemäß GRID-5).
- **NAV-4** [geplant] [gtk] — Doppelklick auf Row = abspielen im Kontext der
  sichtbaren Liste (siehe PLAY-2). Einfachklick = selektieren. Enter = wie
  Doppelklick. Ausnahme Queue-View: Doppelklick springt gemäß QUE-3 zum Track
  (Playhead), statt die Queue neu zu bauen.
- **NAV-5** [aktiv] [gtk] — Modus-Gedächtnis (Scroll + Selektion je
  Tracks/Albums/Artists) gilt nur innerhalb der Session; auch Sidebar-/
  Ortswechsel erhalten Scroll + Selektion des verlassenen Modus. Der
  Scroll-Anker besteht aus Track-/Album-ID plus Offset, nie aus einem rohen
  Pixelwert; Re-Sort und Insert halten dadurch den Inhalt an seiner Position.
  START-1 restauriert über Neustarts ausschließlich die zuletzt aktive Ansicht
  samt Scroll-Position; alle anderen Modi starten oben, unselektiert.
- **NAV-6** [aktiv] [e2e] — Suche (Ctrl+F) filtert die aktuelle Ansicht
  live; Esc leert und schließt. Suche navigiert nie selbst.
- **NAV-7** [geplant] [e2e] — Hamburger-Menü: „Scan Library" → startet Scan,
  bleibt in der Ansicht (Karte erscheint). „Preferences" →
  Preferences-Fenster. „Keyboard Shortcuts" → Shortcuts-Overlay. „About
  Reprise" → About-Dialog. Kein Menüpunkt wechselt kommentarlos die
  Content-Ansicht.
- **NAV-8** [geplant] [gtk] — My Stats ist ein Sidebar-Ort wie jeder andere:
  volle Content-Fläche, Headerbar mit Suche bleibt stehen (Suche dort
  disabled/ausgeblendet ist erlaubt, aber die Leiste bleibt).
- **NAV-9** [ersetzt durch NAV-9a/GRID-5] — Ursprünglich teilten Cover/Titel
  der Player-Leiste und Ctrl+L denselben Sprung zur Heimat des spielenden
  Tracks. Aufgeteilt in Track-Ursprung per Ctrl+L (NAV-9a) und Album-Grid-
  Reveal per Player-Oberflächen (GRID-5).
- **NAV-9a** [aktiv] [gtk] — Ctrl+L navigiert zur Herkunftsansicht des
  geladenen Tracks, selektiert dessen Zeile und zentriert sie ohne
  scrollIntoView-Kantenkleben. Der Sprung pusht auf den globalen
  History-Stack; Back kehrt zum vorherigen Ort zurück.
- **NAV-11** [aktiv] [gtk] — Jeder bedienbare Sidebar-Eintrag exponiert
  gegenüber Assistenztechnik eine eigene Bezeichnung, eine interaktive Rolle
  und eine auslösbare Aktion. Sektionsüberschriften bleiben nicht bedienbar,
  werden aber semantisch als Überschriften exponiert.

## C. Abspielen, Queue, Shuffle, Filter

- **PLAY-1** [geplant] [gtk] — Queue-Quelle = sichtbare Trackliste. „Was du
  siehst, ist was spielt": Doppelklick/Play all/Shuffle in einer Trackliste
  bauen die Queue aus der aktuell sichtbaren (gefilterten, sortierten)
  Liste. Für Container-Buttons gilt PLAY-1a.
- **PLAY-1a** [geplant] [core] — Container-Play (Play-Button auf Cover, Play
  all/Shuffle in Hero-Bereichen) baut die Queue ausschließlich aus dem
  Container in seiner kanonischen Reihenfolge (Album: Disc/Tracknummer;
  Playlist: Positionsreihenfolge; Artist „Play all": Alben nach Jahr, darin
  Tracknummer). Der sichtbare Grid-Filter bestimmt nur, welche Container
  erreichbar sind, nie den Queue-Inhalt.
- **PLAY-2** [aktiv] [core] — Doppelklick spielt die Row und hängt den Rest
  der sichtbaren Liste ab dieser Position in die Queue.
- **PLAY-3** [ersetzt durch PLAY-3a/PLAY-3b] — Ursprüngliche Sammelregel
  „Filter schränkt Shuffle ein"; nach der Prozessregel für halb getestete
  Regeln in Treffer-Shuffle (3a) und Filter-Nachträglichkeit (3b) gesplittet.
- **PLAY-3a** [aktiv] [core] — Filter schränkt Shuffle ein — absichtlich.
  Gefilterte Playlist + Shuffle = Shuffle über die Treffer („shuffle my 90s
  tracks"); die Queue ist genau die Treffermenge, kein Track von außerhalb.
- **PLAY-3b** [geplant] [gtk] — Filter nachträglich ändern fasst eine bereits
  gebaute Queue nicht an (Queue ist ein Snapshot; sichtbar in „Queue").
- **PLAY-4a** [aktiv] [core] — Missing in Listen: Listen-Playback und
  Queue-Advance überspringen Missing still.
- **PLAY-4b** [aktiv] [gtk] — Doppelklick auf konkrete Missing-Row: Toast
  „File missing since …" + Button „Show in Missing files". Einreihen (Play
  next/Add to queue) ist für Missing disabled.
- **PLAY-5** [ersetzt durch PLAY-5a/PLAY-5b] — Ursprüngliche
  Queue-Hygiene-Sammelregel; beim Härten in die Teilregeln deleted (5a) und
  unmounted (5b) gesplittet.
- **PLAY-5a** [aktiv] [core] — Deleted-Hygiene: Extern gelöschte Tracks
  verlassen die Queue still; der spielende Track wird dadurch nie gestoppt
  (faultet der spielende Track selbst, gilt FB-6: Skip + ein Toast).
- **PLAY-5b** [aktiv] [core] — Unmounted-Hygiene: Unmountete Tracks bleiben
  grau in der Queue, werden beim Advance übersprungen und heilen beim
  Mount-Event (P-6). Kein Hintergrundereignis (deleted, unmounted,
  Sync-Removal, Watcher) stoppt den spielenden Track — explizite
  Nutzeraktionen (Doppelklick, Play all, OS-Open) wechseln die Wiedergabe
  natürlich.
- **PLAY-6** [geplant] [gtk] — Shuffle/Repeat sind globale Player-Zustände
  (Player-Leiste), keine Ansichts-Zustände. Repeat zyklisch: off → all → one.
- **PLAY-7** [geplant] [gtk] — Die Player-Leiste ist eine strukturelle
  Abgrenzung, kein Overlay: Sie beansprucht ihre eigene Höhe im Layout, und
  kein Content-Element (Trackliste, Sidebar, rechte Info-Spalte) läuft je
  unter oder hinter ihr durch. Ihr Hintergrund ist opak.
  <!-- REVIEW: Regelvorschlag -->

## D. Albums- & Artists-Ansicht

- **ALB-1** [ersetzt durch GRID-2/GRID-4] — Ursprüngliche gemeinsame
  Album-Grid-Regel für Hover-Overlay, Aktivierung, Container-Play und
  Kontextmenü; in Bedienung/Aktionen (GRID-2) und Overlay-Optik (GRID-4)
  aufgeteilt.
- **ALB-2** [geplant] [gtk] — Album-Detail: Hero mit Cover + dominanter
  Farbfläche (Akzent-Pipeline), Play all/Shuffle-Pills (PLAY-1a), Trackliste
  nach Disc/Tracknummer. Spielender Track: Akzent-Row + EQ-Icon + bold —
  identisch in jeder Liste der App (eine Markierungssprache).
- **GRID-1** [aktiv] [gtk] — Persistenter Playing-Zustand: Das geladene
  Album zeigt unabhängig von Hover und Fokus oben links auf dem Cover das
  gemeinsame EQ-Badge und einen 1.5-px-Innenring um das Cover. Beides nutzt
  `@reprise_player_accent`. Bei Pause bleibt der Ring und die EQ-Bewegung
  friert ein; bei `gtk-enable-animations=false` ist die Glyphe statisch.
- **GRID-2** [aktiv] [gtk] — Bedienung und Aktionen: Das native
  GtkGridView bewegt den Fokus mit Pfeiltasten zweidimensional. Enter öffnet
  die Album-Detailquelle als History-Push, Ctrl+Enter ersetzt die Queue durch
  das Album in kanonischer Disc-/Track-Reihenfolge und startet bei Track 1.
  Space bleibt global Play/Pause. Menütaste und Shift+F10 öffnen an der
  fokussierten Kachel dasselbe Menü wie Rechtsklick, exakt mit Play, Play
  next, Add to queue, Go to artist und Edit tags….
- **GRID-3** [aktiv] [gtk] — Sichtbarer Fokus und Zustandskomposition:
  Tastaturfokus zeichnet einen 2-px-Außenring in `@accent_color` nur um das
  Cover und zeigt dieselbe Play-Affordance wie Hover. Playing, Fokus und
  Hover bleiben getrennte Zustandslayer: Playing innen, Fokus außen,
  Interaktions-Overlay darüber; kombinierte Zustände verdecken einander
  nicht.
- **GRID-4** [aktiv] [gtk] — Bottom-Gradient-Overlay: Hover oder Fokus
  blendet statt einer schwebenden Tooltip-Box einen unten verankerten
  Abdunkel-Gradienten ein. Darin stehen eine dünne Metazeile („13 tracks ·
  47 min") und unten rechts ein Play/Pause-Button in
  `@reprise_player_accent`; Album und Artist bleiben unter dem Cover. Die
  Covermitte bleibt frei. Der Kartencontainer hat keinen Metadaten-Tooltip;
  nur tatsächlich ellipsierte Titel-/Artist-Labels zeigen ihren Volltext.
- **GRID-5** [aktiv] [gtk] — Spielendes Album aufdecken: Aktivierung von
  Cover oder Titel in Playerleiste oder Now-Playing-Panel wechselt bei Bedarf
  in die Album-Ansicht, leert ein sichtbares Suchfeld samt Albumfilter,
  scrollt per GtkGridView/Adjustment zur geladenen Albumkachel, fokussiert sie
  und hebt sie rund 1 s hervor. Der Ortswechsel ist ein History-Push; bereits
  im Album-Grid entsteht kein Duplikat. Fehlt die Albumkachel, greift NAV-9a
  ohne Fehlerdialog. `gtk-enable-animations=false` zeigt für dieselbe Dauer
  ein statisches Highlight.
- **GRID-6** [aktiv] [gtk] — Rückkehrfokus: Back aus einem Album-Detail in
  die Album-Übersicht stellt den Tastaturfokus auf genau der zuvor
  aktivierten Albumkachel wieder her und scrollt sie bei Bedarf sichtbar.
- **ART-1** [aktiv] [gtk] — Artist-Liste: Klick selektiert und zeigt Detail
  rechts; Selection folgt NIE der Wiedergabe, spielender Artist zeigt nur
  Mini-EQ.
- **ART-2** [geplant] [gtk] — Artist-Detail: Hero-Glow (vorberechnete
  Textur, 250 ms Crossfade beim Wechsel), Alben-Reihe (Hover wie ALB-1), Top
  Tracks (Doppelklick spielt gemäß PLAY-2 im Kontext „Top Tracks"). „Show all
  N tracks ›" → Tracks-Modus mit gesetztem Artist-Filter-Chip (sichtbar, per
  × entfernbar).
- **FX-1** [geplant] [manuell] — Alle Effekte respektieren
  `gtk-enable-animations=false` (harte Schaltung) und laufen nur GPU-billig
  (Opacity/Transform, vorgerenderte Glows). Keine Live-Blurs in Listen.

## E. MTP / Sync

- **MTP-1** [geplant] [gtk] — Einstecken: Toast „Pixel 8 connected",
  Gerätekarte faded in die Sidebar. Keine Auto-Navigation — der User wird nie
  aus seiner Ansicht gerissen.
- **MTP-2** [geplant] [gtk] — Karte: Klick auf Karte → Device-View (push).
  Klick auf „Sync"-Pill → startet Sync sofort, ohne Navigation
  (stopPropagation). Hover-Tooltip zeigt Details.
- **MTP-3** [geplant] [core] — Sync läuft: Karte und (falls offen)
  Device-View zeigen denselben Fortschritt (ein State). Cancel überall =
  gleiche Aktion: aktuelle Datei sauber beenden, Toast „Sync cancelled · 28
  copied".
- **MTP-4** [geplant] [gtk] — Unmount/Eject in der Device-View: Eject-Klick →
  Button wird Spinner → unmount → Toast „Pixel 8 can be unplugged" → View
  poppt selbst zur vorherigen Ansicht (150 ms Crossfade), Karte verschwindet.
  Eject während Sync: disabled + Tooltip „Sync in progress".
- **MTP-5** [geplant] [gtk] — Kabel gezogen (ohne Eject): Toast „Pixel 8
  disconnected" (+ „— sync incomplete (54 of 82)" falls mitten im Sync). Ist
  die Device-View offen, wechselt sie auf eine StatusPage „Device
  disconnected" mit Button „Back to Library" — sie schließt sich nicht selbst
  (der User soll lesen können, was passiert ist). Karte verschwindet.
  Nächster Sync resumt via .part-Regel.
- **MTP-6** [geplant] [gtk] — Sync-Ende: Toast „Sync complete · 82 copied, 14
  removed" (+ „· 3 failed" mit „Details"). Karte morpht zu Idle („synced ✓"),
  Delta-Karte zeigt „Everything in sync ✓". Kein 100 %-Haltezustand.

## F. Einstellungen & Modale

- **SET-1** [geplant] [gtk] — Preferences = ein Fenster mit vertikaler
  Navigation (Seiten: General, Library, Playback, Audio, Sync, Plugins).
  Klick auf Seite wechselt rechts den Inhalt; kein Tab-Overflow, neue
  Features = neue Seite oder Sektion.
- **SET-2** [geplant] [gtk] — Unterseiten (z. B. Scrobbler-Konfiguration)
  sind Navigation-Pages im selben Fenster mit ‹-Back im Header — keine neuen
  Fenster.
- **SET-3** [geplant] [gtk] — Modal-Ebenen: maximal zwei. Ebene 1 = ein
  Fenster über dem Hauptfenster (Preferences ODER Tag-Editor ODER Shortcuts —
  nie zwei gleichzeitig). Ebene 2 = genau ein Dialog darüber (FileChooser,
  Bestätigung). Ein Dialog öffnet nie einen weiteren Dialog. Esc schließt
  immer die oberste Ebene.
- **SET-4** [aktiv] [gtk] — Settings wirken sofort (kein Apply/OK).
  Destruktiver Umschalter konkret: Wird Auto-clean aktiviert, während die
  Deleted-Gruppe bereits Zeilen jenseits der gewählten Frist enthält,
  erscheint einmalig ein Dialog: „This will remove N tracks now (deleted
  more than 30 days ago) — their ratings and listening history go with
  them. Remove now / Start counting from today." Letzteres speichert das
  Aktivierungsdatum als Stichtag (`auto_clean_armed_at`); gelöscht wird nur,
  was Frist UND Stichtag reißt. Beide Lösch-Dialoge der App (dieser und
  „Remove all N") benennen die Kaskade explizit: Ratings + Hörhistorie gehen
  mit (P-5).

## G. Feedback-Vokabular

- **FB-1** [geplant] [core] — Zwei-Klassen-Toasts (Pill unten zentriert, eine
  Zeile, max. 1 Action-Button, 4 s / mit Undo 10 s; nur für abgeschlossene
  Aktionen oder Ereignisse): Aktionslose Ereignis-Toasts ersetzen einander —
  maximal einer wartet, der neueste gewinnt, kein Backlog-Rauschen. Toasts
  MIT Aktion (Undo) sind unverdrängbar und laufen ihre vollen 10 s;
  Ereignis-Toasts warten solange.
- **FB-2** [ersetzt durch FB-2a/FB-2b] — Ursprüngliche gemeinsame
  Fortschrittskarten-Regel; beim Relink-Ausbau in den voll gelieferten
  Relink-Vertrag (2a) und die noch nicht einheitlich gelieferte Karte der
  übrigen Langläufer (2b) gesplittet.
- **FB-2a** [aktiv] [gtk] — Der Relink-Suchlauf läuft off-thread in der
  bestehenden, mit Scan/Sync stapelbaren Fortschrittskarte im Sidebar-
  Bottom-Slot: Spinner + Titel + % rechts (tabular) + 3-px-Balken +
  ellipsierte Detailzeile. Klick auf die Karte → Missing files; der sichtbare
  Cancel-Button prüft den Abbruch vor jeder Audiodatei.
- **FB-2b** [geplant] [gtk] — Scan, Sync und Playlist-Import verwenden für
  jeden Lauf > ~1 s denselben vollständigen Kartenvertrag aus FB-2a,
  einschließlich sichtbarem Cancel und Navigation zur zugehörigen Ansicht.
- **FB-3** [aktiv] [core] — Fehler: Einzelfehler im Lauf werden gesammelt,
  nie einzeln getoastet. Am Ende EIN Toast mit „N failed · Details" →
  Details öffnet die zuständige View/Dialog. Persistente Probleme leben als
  Badge + ISSUES-Eintrag, nicht als wiederkehrende Toasts.
- **FB-4** [aktiv] [core] — Badges zählen nur Einträge, die neuer sind als
  das letzte Öffnen der jeweiligen View (`last_viewed`-Timestamp je View im
  Settings-Store): Missing zählt `missing_since > last_viewed`, Import-Errors
  zählt `first_seen > last_viewed` — ohne dismissed-Zeilen und ohne
  Hinweiszeilen („imported without metadata"), denn gezählt wird nur, was den
  User um etwas bittet. Reaktivierung einer dismissed-Zeile (Datei geändert)
  startet eine neue Episode: `first_seen = now`, `seen_count = 1` — sie badgt
  also wieder. View öffnen = Badge weg, die Gesamtzahl steht in der View.
- **FB-5** [ersetzt durch FB-5a/FB-5b] — Ursprüngliche StatusPage-
  Sammelregel; bei der Implementierung in den lieferbaren Missing-Leerzustand
  (5a) und den erst mit dem Root-Guard-UI lieferbaren unavailable-Zustand
  (5b) gesplittet.
- **FB-5a** [aktiv] [gtk] — Die leere Missing-files-Ansicht zeigt die
  StatusPage „No missing files ✓" ohne konkurrierende nächste Aktion.
- **FB-5b** [aktiv] [gtk] — Ein nicht verfügbarer Library-Root zeigt die
  StatusPage „Library folder unavailable — Retry" mit genau diesem nächsten
  Schritt.
- **FB-6** [aktiv] [core] — Gelöschte Datei (extern, Watcher): kein Toast
  pro Datei (Rauschen) — Row wird grau/verschwindet gemäß Missing-Regeln,
  ISSUES-Badge zählt hoch. Ausnahme: der gerade spielende Track faultet →
  Skip + ein Toast „Track unavailable — skipped".
- **FB-7** [aktiv] [core] — „Remove from library" löscht nicht, sondern
  setzt `removed_at` (Tombstone); die Zeile mit Ratings, Play-Counts und
  Playlist-Positionen bleibt 10 s vollständig erhalten, Undo setzt nur
  `removed_at = NULL` zurück — deshalb ist die Wiederherstellung exakt
  (gleiche id, keine Race mit parallel laufenden Scans). Der Remove-Toast
  trägt immer Undo (FB-1, 10 s). Nach Toast-Ablauf wird hart gelöscht
  (Kaskade: Playlist-Einträge, Hörhistorie, Sync-Zustand); App-Ende im
  Fenster → Löschung wird beim nächsten Start committed, nie zurückgerollt
  („7 removed" muss wahr bleiben). Auto-clean (opt-in, default off, nur
  deleted-Tracks) löscht hart ohne Toast und ohne Undo — es feuert
  frühestens 30/90 Tage nach dem Verschwinden (SET-4).

## H. Dateiassoziation & OS-Integration

- **OS-1** [geplant] [e2e] — Eine Datei geöffnet (Doppelklick im
  Dateimanager): Hauptfenster öffnet in der zuletzt benutzten Ansicht
  (Session-Restore), Wiedergabe startet sofort, Player-Leiste zeigt den
  Track. Keine Sonder-Ansicht, kein Mini-Player-Autostart.
- **OS-2** [geplant] [core] — Datei in der Library → normaler Track (mit
  Historie/Rating). Datei außerhalb → transienter Track: spielt, erscheint in
  Queue/Player-Leiste mit dezentem „not in library"-Chip, wird NICHT
  importiert, hinterlässt keine DB-Zeile über die Session hinaus.
  Kontextmenü bietet „Add to library…" an.
- **OS-3** [geplant] [e2e] — Mehrere Dateien (Auswahl → „Öffnen mit
  Reprise"): ersetzt die Queue durch die Auswahl in
  Dateimanager-Reihenfolge, spielt die erste, Toast „12 files queued".
  Zweiter Aufruf während laufender Instanz: gleiche Semantik (Queue
  ersetzen, nicht anhängen — vorhersagbar schlägt schlau). Das Ersetzen bei
  laufender Wiedergabe ist eine explizite Nutzeraktion und verstößt nicht
  gegen PLAY-5.
- **OS-4** [geplant] [e2e] — Single-Instance: ein zweiter Start reicht
  Dateien an die laufende Instanz durch und fokussiert deren Fenster.
- **OS-5** [geplant] [e2e] — MPRIS spiegelt immer den Player-Zustand;
  Abspielen aus Dateiassoziation ist dort identisch sichtbar.

## I. Startzustand

- **START-1** [geplant] [e2e] — Normaler Start: letzte Ansicht +
  Scroll-Position, Wiedergabe pausiert auf letztem Track (Position
  restauriert), Startup-Reconcile läuft still (Karte nur bei echter Arbeit).
- **START-2** [geplant] [gtk] — Start mit unavailable Library-Root:
  StatusPage gemäß Root-Guard, keine Missing-Massenmarkierung; Library-Views
  zeigen den letzten bekannten Bestand normal (Root-Guard hat nichts
  markiert), nur die StatusPage/Karte meldet den Zustand. Kein Blank-Screen.

## J. Queue-Ansicht

- **QUE-1** [aktiv] [gtk] — Ein gemeinsames Queue-Modell speist zwei
  Flächen mit unterschiedlicher Tiefe: Die Sidebar-Zeile „Queue" öffnet die
  ColumnView als Verwaltungsfläche mit Sektionen, DnD-Reorder, Rechtsklick,
  Clear und StatusPage. Der Panel-Toggle öffnet „Up Next" als Sichtfläche
  derselben Queue mit Sektionen, Sprung und Remove. Die Playerleiste hat kein
  redundantes Queue-Icon. Keine Fläche führt eine eigene zweite Liste.
- **QUE-2** [aktiv] [gtk] — Das Panel gliedert die Zukunft in genau zwei
  bedingte Sektionen: **Next in Queue** für manuell eingereihte Tracks und
  **Continuing from „<Album/Playlist>"** für den automatischen Kontext aus
  `play_origin`. Ein Header erscheint nur, wenn seine Sektion Einträge hat;
  eine leere manuelle Sektion lässt ausschließlich „Continuing …" stehen.
  Ihre sichtbare Reihenfolge ist zugleich die Abspielreihenfolge; solange
  etwas spielt, zeigt die Queue nie zwei leere Sektionen.
- **QUE-3** [aktiv] [core] — Abgespielte manuelle Einträge verschwinden beim
  Trackwechsel still aus „Next in Queue": kein Durchstreichen und kein
  Verharren. Die Sektion enthält ausschließlich die noch ausstehende Zukunft.
  „Remove" im Panel entfernt genau den Eintrag aus der Queue, nie aus der
  Library.
- **QUE-4** [aktiv] [core] — Die Queue-Fußzeile formatiert Trackzahlen mit
  derselben gemeinsamen Tausendertrenner-Funktion wie die Library; es gibt
  keinen zweiten Formatierungspfad.
- **QUE-5** [aktiv] [core] — Ein Sprung zu einem Queue-Eintrag setzt die
  Abspielposition und konsumiert ausschließlich den geklickten Eintrag.
  Davorliegende manuelle Einträge bleiben in „Next in Queue" und spielen
  danach; es gibt weder stilles Verwerfen noch Dialog oder Queue-Historie.
  „Remove" entfernt aus der Queue, nie aus der Library.
- **QUE-6** [aktiv] [core] — Beide Flächen lesen ein gemeinsames
  Queue-Modell. Metadaten kommen in einer Sammelabfrage über die Queue-IDs,
  nie in einer Abfrage pro Zeile; Row-Recycling und Laden des sichtbaren
  Fensters begrenzen Widgets und Arbeit unabhängig von der Queue-Länge. Bei
  geschlossenem Panel oder einem anderen aktiven Tab aktualisieren
  Trackwechsel und Reorder nur das Modell und rendern keine Panel-Zeilen.

## K. Filter- & Such-Sichtbarkeit

- **FIL-1a** [aktiv] [gtk] — Eine Wahrheit über Einschränkungen
  (Track-Listen): Alles, was die sichtbare Track-Liste einschränkt, steht als
  Chip in der Filter-Zeile direkt über der Liste — auch die Headerbar-Suche
  (Chip ⌕ „falling“ in any field, eigenes ×-Klickziel ≥ 20 px; das × entfernt
  nur die Suche, Esc gemäß NAV-6). Gilt in jeder Track-Quelle (Library,
  Playlist, Smart, Queue, Missing). Die Suche ist global über Track-Quellen
  und reist beim Ortswechsel mit; ihr Chip erscheint überall dort, wo sie
  tatsächlich einschränkt — in Quellen ohne Suchwirkung (Import-Errors:
  eigene Panel-Rows) erscheint kein Chip. Facetten-Chips und „+ Add filter"
  bleiben Library-only. Ein unsichtbarer aktiver Filter ist ein Bug.
  Per-Ort-Scoping der Suche wäre eine eigene künftige Regel, nicht Teil
  dieser.
- **FIL-1b** [geplant] [gtk] — Albums-/Artists-Modus: Die globale Suche
  wirkt dort bereits (Grid-Filterung); dieselbe Chip-Zeile inkl. Zählung und
  „Clear all" folgt dort nach dem Muster von FIL-1a/FIL-2. Bis dahin ist die
  Lücke hier benannt statt still gebrochen.
- **FIL-2** [aktiv] [gtk] — Zählung ist Zustand: Die Filter-Zeile ist
  permanenter Listen-Header jeder Track-Quelle — sie erscheint und
  verschwindet nie (kein Layout-Shift by design, P-4). Idle maximal leise:
  nur die neutrale Gesamtzahl rechts (dim, caption), in der Library
  zusätzlich das „+ Add filter"-Pill; kein „FILTER"-Label. Bei aktiver
  Einschränkung: „FILTER"-Label + Chips + akzentuierte Trefferzahl
  („15 of 1,664 tracks", Trefferzahl Akzentfarbe bold) + „Clear all ×"
  (räumt Suche und alle Filter in einem Klick). Die Ausblende-Preference der
  Leiste regelt nur den Idle-Zustand — bei aktiver Einschränkung erscheint
  die Zeile immer (Force-show; der Shift ist direkte Folge der eigenen
  Eingabe, P-4-konform). Das Status-Overlay unten rechts zeigt immer die
  neutrale Bibliotheks-Statistik; seine „X of Y"-Variante entfällt — die
  Filter-Zeile spricht über die Sicht, das Overlay über die Bibliothek.
  Präzisierung: Außerhalb der Library erscheint das Overlay gar nicht — die
  Filter-Zeile ist dort die einzige Zählung (beschlossen 2026-07-17).
- **FIL-3** [aktiv] [gtk] — Ende-der-Treffer-Zeile: Unter der letzten Row
  einer eingeschränkten Liste (≥ 1 Treffer) steht zentriert „End of results —
  1,649 tracks hidden by search “falling”" + Pill „Show all 1,664 tracks"
  (= Clear all). Sie gehört visuell zum Listenende: direkt unter der letzten
  Row, wenn die Liste kürzer als der Viewport ist; bei längeren Listen
  erscheint sie erst, wenn das Listenende in den Viewport scrollt; sie
  schwebt nie über Rows (nicht sticky). Umsetzung als positioniertes Overlay
  — die Virtualisierung des ColumnView bleibt unangetastet; Input-durchlässig
  außer der Pill; Position wird bei Scroll-, Model-/Filter- und
  Resize-Änderungen neu berechnet.
- **FIL-4** [ersetzt durch SEARCH-3] [gtk] — Suchfeld trägt seinen Zustand: Sobald das Feld
  Text enthält, bekommt es Akzent-Border + getönten Hintergrund — auch
  unfokussiert.
- **FIL-5** [aktiv] [gtk] — Treffer-Highlighting: Der Suchbegriff wird in
  allen durchsuchten, sichtbaren Textspalten hervorgehoben (Title, Artist,
  Album, Genre; Akzent bold, Pango-escaped). Ist die einzige matchende
  Spalte ausgeblendet, bleibt die Row unmarkiert — akzeptierte Restlücke.
  Chip-Wortlaut bleibt „in any field".
- **FIL-6** [aktiv] [gtk] — 0-Treffer-Leerzustand: StatusPage mit genau
  einem Button „Show all 1,664 tracks" (= Clear all) — FB-5-konform; der
  eine Schritt führt garantiert zu Inhalt, nie in einen zweiten Leerzustand.
  „Clear all ×" (Filter-Zeile), „Show all N tracks" (Ende-Zeile,
  Leerzustand) feuern dieselbe Action — zwei kontextgerechte Namen, ein
  Verhalten.
## L. Tag-Editor

- **TAG-1** [aktiv] [gtk] — Save ist navigationsneutral: Speichern ändert
  weder Scroll noch Ansicht der Library (NAV-5 gilt durch den Dialog
  hindurch); ein „Springen zum nächsten Song" gibt es nicht. Nach dem
  Schließen liegt der Fokus auf der Library, Selektion = die **geschriebenen**
  Tracks (bei Teilfehlern die gelungenen; nach Cancel/Discard unverändert) —
  Feedback über die eigene Handlung ist erlaubt, der Sprung zu unbeteiligten
  Tracks nicht. Mechanik an der Wurzel: der Reload sichert Selektion über
  Track-IDs und Scroll über einen Anker (Track-ID + Offset, nie Pixel) und
  stellt beide wieder her — für alle Auslöser (Save, Watcher-Reconcile,
  Sortierung, Rating). Gelöschte IDs fallen still heraus; ein gewollter Reset
  ist explizit, nie Nebeneffekt.
- **TAG-2** [aktiv] [gtk] — Multi-Semantik: Felder mit identischem Wert
  zeigen ihn normal; abweichende zeigen einen Mixed-Platzhalter (kursiv,
  gestrichelte Border) — bei ≤ 2 verschiedenen Werten die Werte selbst
  („Mixed — Ambient, Post-Rock"; leer zählt als eigener Wert), ab 3 die
  Anzahl („Mixed — 8 different values"), daneben der Zähler („2 values").
  Kein Wert wird vorausgefüllt und kein Feld ist gesperrt: das erste
  getippte Zeichen macht es scharf (Akzent-Border, Revert im Feld, „will be
  applied to all N"). Backspace/Entf im Platzhalter macht ebenso scharf — als
  „leeren für alle N", mit voller Review-Behandlung. Nichts wird still
  verschluckt.
- **TAG-3** [aktiv] [gtk] — Per-Track-Felder sind im Multi-Modus read-only:
  Title und Track number zeigen „—" mit Tooltip „Per-track field — edit
  tracks individually". Ein Massen-Titel ist immer ein Unfall.
- **TAG-4** [aktiv] [gtk] — Blättern verwirft nichts: Öffnet der Editor
  mit genau einem Track, blättern ‹ › (Ctrl+Page Up/Down) durch einen
  Snapshot der sichtbaren Liste zum Öffnungszeitpunkt — über Track-IDs, nie
  Indizes, damit „Track 3 of 12" stabil bleibt, während darunter re-sortiert
  wird. Pending Änderungen werden pro Track gehalten; Save schreibt alle
  pending Tracks, Cancel verwirft alle (Bestätigung ab einer Änderung).
  Invalides Zahlenfeld (Year/Track) blockt sowohl Blättern als auch Save.
- **TAG-5** [aktiv] [core] — Der Diff steht am Feld, nicht in einem zweiten
  Dialog: Jedes effektiv geänderte Feld zeigt darunter den Altwert („was: …",
  durchgestrichen, gedimmt), Border in Akzent; der Platz dafür ist immer
  reserviert (P-4). Darüber dem Save-Bereich eine Summary-Zeile („2 fields ·
  30 tracks affected"), im Multi-Modus und bei feldübergreifendem Pending
  zusätzlich ein Expander „Review changes" mit einer Zeile je Feld
  (`Artist: Suicide → Suicide Silence · 30 tracks`). Gezählt werden **nur
  Tracks, deren Wert sich wirklich ändert**; No-op-Writes entfallen (exakter
  Vergleich, kein Trim/Case-Angleich). Alle Zahlen sprechen dieselbe Währung
  — Tracks: Save-Button („Save 30"), Fortschritt („Saving… 12/30") und Toast
  („Tags updated · 30 tracks"). Ohne effektive Änderung ist Save disabled und
  benennt den Grund (P-2).
- **TAG-6** [aktiv] [core] — Autocomplete-Quelle für Artist, Album, Album
  Artist und Genre: distinct-Werte der eigenen Library mit Track-Zahl,
  case-insensitive; Präfix-Treffer vor Substring-Treffern, darin nach
  Track-Zahl absteigend; maximal 6 Zeilen, Dropdown ab 2 Zeichen,
  Sektionstitel „FROM YOUR LIBRARY". Letzte Zeile ist immer „Use ‚X' as new
  artist…" — ein neuer Wert ist nie blockiert.
- **TAG-7** [ersetzt durch TAG-7a/TAG-7b] — Inline-Ghost. Gesplittet, weil die
  Mechanik einklagbar ist, während das tatsächliche Erscheinen des Ghosts
  headless nicht beweisbar ist (TESTING.md: Xvfb belegt Konstruktion, Signale
  und CSS, nicht das finale Rendering) und bis zur Sichtprüfung abgeschaltet
  bleibt. Ein einziges `[aktiv]` hätte für die eine Hälfte gelogen.
- **TAG-7a** [aktiv] [gtk] — Ghost-Mechanik: Der vorgeschlagene Ghost ist der
  beste Präfix-Treffer, in derselben Rangfolge wie Dropdown-Zeile 1 (Tiebreak
  Track-Zahl) — Ghost und Zeile 1 nennen nie verschiedene Werte; ein reiner
  Substring-Treffer wird nie geghostet. Tab übernimmt **nur** einen sichtbaren
  Ghost; ohne Ghost ist Tab reiner Fokuswechsel — eine stille Übernahme der
  ersten Dropdown-Zeile gibt es nicht. Das Tab-Badge rendert nur bei
  sichtbarem Ghost. Der Ghost ist reine Anzeige und wird nie zur Änderung,
  solange ihn niemand übernimmt. Das Popover ankert am Entry und stiehlt nie
  den Fokus: Tippen läuft ununterbrochen weiter. Gilt unverändert, während
  der Ghost abgeschaltet ist (dann ist schlicht nie einer sichtbar).
- **TAG-7b** [geplant] [manuell] — Der Ghost erscheint tatsächlich: gedimmt,
  bündig hinter dem getippten Text, an der Cursor-Position. Headless nicht
  beweisbar (TESTING.md: Xvfb belegt Konstruktion, Signale und CSS, nicht das
  finale Rendering), deshalb bleibt `GHOST_ENABLED = false`, bis eine
  Sichtprüfung auf einem echten Display es bestätigt — „kein halb kaputtes
  Ghost im Release". Das Zielbild ist beschlossen, nur die Auslieferung wartet
  auf die Abnahme; Umschalten kostet dann eine Konstante, keinen Code.
- **TAG-8** [aktiv] [gtk] — Tastatur-Semantik. **Enter:** bei offenem
  Dropdown übernimmt es den markierten Vorschlag (Dropdown zu, Fokus bleibt
  im Feld); bei geschlossenem springt es ins nächste editierbare Feld; im
  letzten Feld fokussiert es den Save-Button, sodass der nächste Enter
  bewusst speichert. Enter speichert **nie** direkt aus einem Textfeld — zu
  leicht ausgelöst, während man durch Vorschläge tippt. Ctrl+Enter speichert
  von überall (Ctrl+S ist derselbe, nur unbeworbene Alias — eine Action für
  beide). **Esc-Kaskade:** erst schließt das Popover (Text bleibt), dann
  revertet das scharfe Feld, dann greift die Dialog-Ebene (Discard-Frage ab
  einer Änderung, sonst schließen) — jede Stufe vernichtet höchstens, was die
  nächste wiederbringen kann; läuft gerade ein Save, ignoriert die
  Dialog-Ebene Esc vollständig (der Batch ist atomar, kein Abbruch). Die
  Discard-Frage zählt Tracks („Discard changes to 3 tracks?") und hat zwei
  Antworten: Keep editing (Default) und Discard (destruktiv). Kein Save im
  Prompt: Speichern ist nie der Ausweg aus einer Schließen-Geste.

## M. Tooltips

<!-- Die Sektionsbuchstaben K (Filter- & Such-Sichtbarkeit) und L (Tag-Editor)
     sind bereits vergeben; Tooltips sind daher Sektion M. -->

Tooltips sind Beschriftung, kein Feedback-Mechanismus — sie tragen nie die
einzige Aussage (TIP-3) und fallen daher nicht unter P-1s Rollenmodell.
Wird ein ganzer Container deaktiviert, gilt TIP-2a/b für die
Container-Aussage, nicht für jedes Kind einzeln (die leere Player-Leiste
ist ihre eigene Aussage).

- **TIP-1a** [aktiv] [gtk] — Existenz folgt der Beschriftung:
  Icon-only-Buttons haben immer einen Tooltip; Buttons mit sichtbarem
  Textlabel bekommen keinen — das Label ist die Aussage, ein
  wiederholender Tooltip ist Rauschen. Ausnahme: ellipsierte/abgeschnittene
  Labels zeigen im Tooltip den vollen Text.
- **TIP-1b** [geplant] [manuell] — Form: Verb + Objekt („Eject Pixel 8",
  „Toggle sidebar"); das Objekt darf entfallen, wenn der Button es selbst
  eindeutig macht („Play", „Shuffle"). Existiert ein Shortcut, steht er in
  Klammern dahinter („Play (Space)").
  <!-- Flip-Kriterium TIP-1b: „Previous"/„Next" im Tag-Editor
       (tag_editor_form.rs, Ownership feat/tag-editor-rework) und „Back" in
       browse_bar (Ownership feat/global-search-rework) sind noch
       Substantive. [aktiv] erst, wenn beide nachgezogen sind. -->
- **TIP-2a** [aktiv] [gtk] — Disabled erklärt sich (icon-only): ein
  deaktiviertes Icon-only-Control behält seinen Tooltip und ergänzt den
  Grund („Eject device — Sync in progress"). Nie ein toter Button ohne
  benannten Grund (Konkretisierung von P-2).
  <!-- Player-Leiste prev/next: KEINE Einzel-Tooltips. Sie werden nur
       deaktiviert, wenn die Queue leer ist — und dann ist auch die ganze
       Leiste deaktiviert (bar_should_be_sensitive), sodass die
       Container-Klausel oben greift: die leere Leiste ist ihre eigene
       Aussage. -->
- **TIP-2b** [geplant] [manuell] — Disabled erklärt sich (gelabelt): ein
  deaktiviertes gelabeltes Control nennt seinen Grund sichtbar per Label,
  Subtitle oder Hint-Zeile („Requires same artist & album across
  selection", „Everything in sync") — nie nur per Tooltip (TIP-3: der
  Grund wäre sonst exklusive Hover-Information).
  <!-- Flip-Kriterium TIP-2b: Save/„Change cover…" im Tag-Editor
       (feat/tag-editor-rework) und der deaktivierte „Add filter"-Zustand
       in browse_bar (feat/global-search-rework) sind noch unbegründet
       tot. [aktiv] erst, wenn beide nachgezogen sind. -->
- **TIP-3** [aktiv] [manuell] — Tooltips sind redundant, nie exklusiv:
  jede Information in einem Tooltip muss auch ohne Hover erreichbar sein
  (View, Dialog, sichtbares Label). Hover-Details (Sync-Karte:
  „28 of 82 · ~2 min left") sind Komfort-Duplikate einer erreichbaren
  Ansicht — Touch-Bedienung sieht Tooltips nie.
- **TIP-4** [aktiv] [manuell] — Menüeinträge bekommen keine Tooltips.
  In Popover-/Kontextmenüs trägt das Label allein; eine feste
  Subtitle-Zeile („M3U · PLS · XSPF") ist erlaubt. Braucht ein Menüpunkt
  einen Tooltip, ist er falsch benannt oder gehört in einen Dialog.
- **TIP-5** [aktiv] [manuell] — GTK-Standardverhalten: keine
  Custom-Delays, keine interaktiven/Rich-Tooltips; dynamische Werte
  (Prozent, Zeit, ellipsierter Volltext) sind erlaubt.

## N. Track-Kontextmenü

- **CTX-1** [aktiv] [gtk] — Ein Builder, ein Kontext-Enum. Alle
  Track-Row-Menüs entstehen aus einer reinen Funktion `build_track_menu(
  context, selection)` (GMenu-Sections), nie aus fünf handkopierten Menüs.
  Kontexte: `LibraryTracks | AlbumDetail | ArtistDetail | Playlist | Queue`.
  Missing-View und Smart-Playlists rendern als `LibraryTracks`.
- **CTX-2** [aktiv] [gtk] — Nur Selektions-Aktionen. Kein globaler Eintrag im
  Track-Menü (kein „Rescan library" — das lebt im Hamburger-Menü). Rechtsklick
  auf eine unselektierte Row selektiert sie zuerst; das Menü gilt immer der
  sichtbaren Selektion. Shift+F10 / Menü-Taste öffnen auf der
  Tastatur-Selektion.
- **CTX-3** [aktiv] [gtk] — Kein „Play"-Eintrag. Primäraktion ist
  Doppelklick/Enter (PLAY-2). Erster Menü-Eintrag ist „Play next" (in der
  Queue: „Move to top").
- **CTX-4** [aktiv] [gtk] — Navigation nur mit eindeutigem Ziel. „Go to
  album"/„Go to artist" entfallen, wenn der Kontext das Ziel IST (Album-Detail
  zeigt kein „Go to album", Artist-Detail kein „Go to artist"). Bei
  Mehrfachselektion aktiv nur, wenn alle Tracks dasselbe Album bzw. denselben
  (Album-)Artist teilen, sonst ausgegraut — nie versteckt, das Menü bleibt
  formstabil. Das Ausgrauen trägt die Bedeutung allein; kein Tooltip (TIP-4).
- **CTX-5a** [aktiv] [gtk] — Destruktiv gehört dem Kontext. Playlist → „Remove
  from playlist", Queue → „Remove from queue" (beide sofort, ohne Dialog).
  „Remove from library…" und „Move to Trash…" existieren NUR in
  Library-Kontexten (LibraryTracks/AlbumDetail/ArtistDetail), nie in Playlist
  oder Queue. „Move to Trash…" ist der einzige rot/destruktiv markierte
  Eintrag.
- **CTX-5b** [geplant] [gtk] — „Remove from library" wird sofort + Undo-Toast
  (FB-7); die Ellipse „…" und der Bestätigungsdialog fallen im selben Commit,
  der den Undo-Toast baut. Bis dahin bleibt der Eintrag „Remove from library…"
  mit Dialog (CTX-5a).
- **CTX-6** [aktiv] [gtk] — Zähl-Währung nur destruktiv. Nur destruktive
  Einträge tragen die Selektionszahl: „Remove 3 from playlist", „Remove 3 from
  queue", „Remove 3 from library…", „Move 3 to Trash…". Alle anderen Einträge
  bleiben unnummeriert; „Edit tags…" öffnet den Multi-Editor, der selbst
  „Editing 3 tracks" titelt.
- **CTX-7** [geplant] [manuell] — Hover neutral (Weiß ~10 %); die Akzentfarbe
  bleibt Selektion und spielendem Track vorbehalten. Das Menü passt ohne Scroll
  ins Fenster (GTK-Popover flippt am Rand).
- **CTX-8** [aktiv] [gtk] — Missing-Rows in der Selektion. „Play next"/„Add to
  queue"/„Move to top" sind deaktiviert (nicht abspielbar = nicht einreihbar,
  PLAY-4b); „Show in Files"/„Move to Trash…" sind deaktiviert (Datei fehlt).
  Ein zusätzlicher Eintrag „Show in Missing files" erscheint, sobald die
  Selektion Missing-Rows enthält und die Ansicht nicht selbst die Missing-View
  ist, und springt zur Issues-View. „Edit tags…" wirkt nur auf vorhandene
  Dateien: bei rein-missing Selektion deaktiviert, bei gemischter auf die
  vorhandenen (der Editor-Titel zählt nur diese). „Remove from
  playlist/library" bleiben aktiv.
- **CTX-9** [aktiv] [gtk] — „Add to playlist ▸". Das Submenu listet Playlists
  alphabetisch, „New playlist…" am Ende. Die aktuell offene Playlist ist
  ausgegraut (kein Duplikat-Einfügen in sich selbst per Menü; DnD bleibt frei).
- **CTX-10** [aktiv] [gtk] — „Show in Files" ist aktiv, wenn alle selektierten
  Dateien vorhanden sind und im selben Ordner liegen (eine
  Nautilus-Mehrfachmarkierung in einem Fenster), sonst ausgegraut.

## O. Motion & Transitions

<!-- Sektionsbuchstabe: M (Tooltips) ist auf main vergeben; N ist durch
     feature/context-menu-unification („N. Track-Kontextmenü") beansprucht.
     Motion nimmt daher O; die Buchstabenlage wurde beim Einfügen dieser
     Sektion gegen den main-Stand verifiziert. -->

Motion illustriert, sie informiert nie exklusiv: jede Transition bestätigt
eine Zustandsänderung, die auch ohne sie vollständig sichtbar wäre —
`gtk-enable-animations=false` ist der Beweis (MOT-7). Animationen folgen
direkten Nutzeraktionen; Hintergrundprozesse schalten hart oder faden an
Ort und Stelle (MOT-2, die Motion-Lesart von P-4).

- **MOT-1** [aktiv] [gtk] — Vier Tokens, keine freien Zahlen: jede von
  Reprise selbst konfigurierte Animation nutzt eines von vier Tokens aus
  `ui/motion.rs`: **Micro** 150 ms ease-out für Control-Zustand
  (Icon-Wechsel Play⇄Pause, Hover-Pills, Chips, Rating, Press-Scale;
  Icon-Crossfades laufen als zwei Micro-Hälften à 75 ms) · **Standard**
  250 ms ease-out-cubic für Flächen (Sidebar-/Panel-Reveal, Toast rein,
  Card-Collapse, Crossfades Cover/StatusPage⇄Liste) · **Ambient** 400 ms
  ease-out-cubic für atmosphärische, nicht-interaktive Übergänge
  (Akzentfarben-Crossfade) · **Spatial** = AdwSpringAnimation mit
  Adw-Default-Springparametern für gerichtete Navigation, im Code angelegt
  ab dem ersten gerichteten Navigationsfall. Ease-in nur für Verlassendes
  (Toast raus, Micro-Dauer); linear nur für echte Fortschrittsbalken.
  Adw-interne Widget-Animationen ohne Dauer-API (OverlaySplitView,
  NavigationSplitView, ToastOverlay, Banner, Dialog, Popover — z. B. die
  Push/Pop-Slides der Einstellungs-Unterseiten) gelten als systemgegeben
  und sind vom Token-Zwang ausgenommen.
  <!-- Flip-Kriterium MOT-1: alle Call-Sites aus dem Audit-Inventar des
       Motion-Plans konsumieren Tokens; scripts/check-motion-tokens.sh ist
       scharf und ohne Restlisten-Allowlist. -->
- **MOT-2** [aktiv] [gtk] — Nutzeraktion animiert, Hintergrund nie:
  Transitions folgen direkten Nutzeraktionen. Scan/Watcher/Mount/Sync
  schalten hart bzw. faden ohne Verschiebung (P-4 in Motion-Sprache).
  Ausnahme: die vom Nutzer gestartete Prozess-Karte darf füllen/pulsieren.
- **MOT-3** [aktiv] [gtk] — Symmetrie: gleiches Muster = gleiches Widget
  + gleiches Token. Konkret: die linke Bibliotheks-Sidebar nutzt exakt das
  Widget und damit exakt die Transition der rechten Info-Spalte
  (`adw::OverlaySplitView`, Position Start — Auslöser dieser Sektion); der
  innere Tracks/Albums/Artists-Wechsel und die StatusPage⇄Liste-Stacks
  crossfaden mit dem Standard-Token wie der äußere
  Library/Stats/Device-Stack.
- **MOT-4** [aktiv] [manuell] — Listen bewegen sich nicht: kein
  Stagger/Fade-in pro Row (windowed Model, 200er-Fenster, Bibliotheken
  jenseits 1 600 Rows). Erlaubt: ein Crossfade der gesamten Fläche beim
  View-Wechsel; benannte Ausnahme: die Queue darf DnD-Drop und
  Einzel-Remove animieren.
  <!-- Die Queue-Ausnahme ist erlaubend, nicht fordernd; ihre Umsetzung
       liegt im Folge-Branch und blockiert den MOT-4-Flip nicht. -->
- **MOT-5** [aktiv] [gtk] — Player-Leiste lebt, aber leise: Play→Pause =
  Icon-Crossfade (zwei Micro-Hälften) + Scale-Puls (1.0→0.92→1.0, Micro);
  Track-Wechsel = Cover/Titel-Crossfade; die Waveform crossfadet zum neuen
  Track statt auf 0 zu fahren; Pause entsättigt den Waveform-Fill leicht
  (zur Draw-Zeit), Play kehrt es um — die Akzent-Pipeline (`cover_accent`)
  bleibt unberührt. Die EQ-Indikatoren (Trackliste, Mini-Player) laufen
  nur während aktiver Wiedergabe; die Idle-Leiste ist statisch — kein
  Dauerloop ohne Wiedergabe.
- **MOT-6** [aktiv] [gtk] — Nichts blockiert: das Modell ändert sich am
  Frame 0, die Animation illustriert nur. Eine zweite Aktion während einer
  laufenden Animation springt per `AdwAnimation::skip()` zum Endzustand und
  startet dann die neue; Animations-Slots (Track-Crossfade, Icon-Crossfade,
  Akzent-Fade) rufen `skip()` statt den alten Handle stillschweigend zu
  droppen.
- **MOT-7** [aktiv] [gtk] — `gtk-enable-animations=false` gewinnt
  ausnahmslos: jedes Token degradiert zentral in `ui/motion.rs` zum
  Hard-Switch (`follow-enable-animations-setting` bzw. der zentrale
  Gate-Helper `animations_enabled()`), nicht an 30 Call-Sites. Gilt auch
  für eigene Tick-Callbacks (Waveform-Positions-Glättung: Position hart
  setzen; Progress-Interpolation) und Pulse-Timer. `gtk::Spinner` und
  GTK-interne CSS-Mechanik sind Systemverhalten und werden nicht gegated.

## P. Now-Playing-Panel

<!-- Sektionsbuchstabe: O (Motion) ist die letzte auf main vergebene Sektion;
     P schließt lückenlos an. Die Regeln stammen aus dem Grilling vom
     2026-07-18 zu Design 21/21a; das Beschluss-Ledger unter
     docs/superpowers/plans/2026-07-18-npp-beschluesse.md hält das Warum und
     die Detailentscheidungen unterhalb der Regel-Ebene. -->

Die rechte Spalte gehört dem **spielenden** Track, nie der Library-Selektion:
sie ist kein Inspektor, sondern die Bühne des laufenden Stücks. Ein pausierter
Track zählt als geladen und bleibt stehen; ohne geladenen Track zeigt das
Panel einen ruhigen Platzhalter, statt sich von selbst zu schließen (P-1 für
die Lautstärke gilt weiter: im Panel lebt kein Volume-Regler).

- **NPP-1** [aktiv] [gtk] — Geometrie ist ein **Pixel**-Vertrag und bewusst
  ungleich: linke Sidebar fix **240 px**, rechtes Panel fix **300 px**, beide
  gepinnt statt als Spanne. Das Panel klappt mit derselben Slide-Transition
  ein wie die Sidebar (MOT-3, Standard-Token). Zwei Fallstricke, beide
  gemessen statt vermutet: `AdwOverlaySplitView` rechnet ohne
  `sidebar-width-unit = Px` in `sp`, und ein Kind ohne `ellipsize` erzwingt
  über seine Textbreite eine Mindestbreite, die `max-sidebar-width` nicht
  unterschreiten kann — ein Statuselement in der Sidebar darf deren Breite
  nie diktieren.
- **NPP-2** [aktiv] [gtk] — Aufbau von oben: Cover 168 px (Radius 12,
  Schatten + 1 px Inset-Hairline) → Titel 15 px bold → „Artist · Album"
  12 px weiß 55 % → **Pill-Toggle** (Segmente, kein Tab-Bar-Widget) →
  Tab-Inhalt → Fußzeile 10.5 px weiß 35 %, deren Inhalt der aktive Tab
  stellt. Kein Panel-Header: Schließen läuft über den App-Header-Toggle,
  ein Retry gehört in den Fehlerzustand des Tabs. **Kein Volume-Regler**
  (P-1).
- **NPP-3** [aktiv] [gtk] — Glow statt Volltint: ein radialer Verlauf aus der
  Cover-Akzentfarbe liegt im oberen Drittel hinter dem Cover und läuft nach
  unten auf neutrales Panel-Dunkel aus. Der Grund ist Lesbarkeit — die
  Grundfläche bleibt neutral, damit der Lyrics-Kontrast über die ganze Höhe
  konstant ist. Fallback ist der Theme-Akzent (Petrol), Idle zeigt keinen
  Glow. Als Verlauf gerendert, nie live geblurrt.
- **NPP-4** [aktiv] [gtk] — Tab-Gedächtnis nur für die Session (NAV-5);
  ein Neustart landet auf Up Next. Die Panel-*Sichtbarkeit* persistiert
  weiterhin über Neustarts hinweg — Tab und Sichtbarkeit sind getrennte
  Zustände.
- **NPP-5** [aktiv] [gtk] — Zeilen-Hierarchie im Lyrics-Tab: aktive Zeile
  15 px bold weiß mit Akzent-Unterstrich (26 × 2.5 px, zentriert, Farbe =
  Cover-Akzent), Nachbarn gestuft weiß 45 % (±1) / 32 % (±2) / 28 %
  (weiter). Alle Zeilen zentriert, 13 px, großzügiger Abstand. Ganze
  LRC-Zeilen, kein Karaoke-Wort-Highlight.
- **NPP-6** [aktiv] [gtk] — Zeilenwechsel: die neue Zeile blendet auf
  weiß+bold, die alte zurück (Micro-Token); gleichzeitig gleitet die Liste
  die aktive Zeile mittig (Standard-Token, ease-out-cubic — kein Spring,
  Lyrics sollen ruhig laufen). Der Unterstrich wandert nicht, er gehört zur
  aktiven Zeile und faded mit ihr.
- **NPP-7** [aktiv] [gtk] — Manuelles Scrollen gewinnt: User-Scroll pausiert
  den Auto-Scroll 4 s und resettet den Timer bei jedem weiteren Ereignis,
  danach gleitet die Liste zur aktiven Zeile zurück; ein laufender
  Rück-Glide wird dabei abgebrochen. Das Highlight läuft während der Pause
  weiter — pausiert ist nur der Scroll. Programmatische Scrolls resetten den
  Timer nie, sonst würde sich das Panel selbst aussperren.
- **NPP-8** [aktiv] [gtk] — Klick auf eine Zeile seekt zu ihrem Timestamp
  (nur synced); Hover hebt auf weiß 65 % mit Pointer. Das ist die einzige
  Klick-Interaktion im Lyrics-Tab, und der Text ist nicht selektierbar. Ein
  Seek — von hier oder aus der Waveform — springt sofort zur neuen aktiven
  Zeile, ohne den 4-s-Timer aus NPP-7.
- **NPP-9** [aktiv] [gtk] — Fallbacks ohne Sackgasse: unsynced → statischer
  scrollbarer Text (weiß 65 %), kein Highlight, kein Auto-Scroll, Fuß
  „lyrics · tags"; keine Lyrics → dezenter Leerzustand ohne Such-CTA;
  Fehler → Inline-Retry im Tab. Instrumental-Gap (> 10 s ohne Zeile) hält
  die aktive Zeile und dimmt sie auf 60 %, statt das Highlight zu verlieren.
- **NPP-10** [aktiv] [gtk] — Trackwechsel ist kein Ortswechsel: Cover,
  Titelblock, Glow und Tab-Inhalt crossfaden **gemeinsam** in einem
  Übergang (Standard-Token, MOT-5), niemals als Slide; die Lyrics starten
  danach bei Zeile 0 und positionieren sie gemäß LYR-4.
  `gtk-enable-animations=false` schaltet auch hier hart (MOT-7).

## Q. Suche

- **SEARCH-1** [aktiv] [gtk] — Im Ruhezustand belegt die Suche in der
  Headerbar nur eine Lupe. Das Suchfeld lebt in einer zweiten, standardmäßig
  eingeklappten Top-Bar und wird nie als permanentes breites Feld dargestellt.
- **SEARCH-2** [aktiv] [gtk] — Ein Klick auf die Lupe, Ctrl+F oder direktes
  Tippen öffnet die Suchleiste und fokussiert das Feld. Sie ist ein
  vollbreiter Streifen bündig unter der Headerbar, hat eine eigene Fläche mit
  unterer Trennlinie und schiebt beim Reveal den Inhalt nach unten; das
  Suchfeld ist darin per Clamp auf ungefähr 450 px zentriert. Die Leiste
  slidet mit der zentralen Standarddauer (MOT-1/3); bei GTK-eigenen Revealern
  gilt deren Default, sofern er dem Standard-Token entspricht.
- **SEARCH-3** [aktiv] [gtk] — Die Lupe ist ein ToggleButton und trägt bei
  offener Suchleiste **oder** aktiver nicht-leerer Query den
  `:checked`-Akzentstil. Eine Query bleibt auch bei eingeklappter Suchleiste
  sichtbar: Ihr Such-Chip bleibt bestehen. Die Lupe bekommt keinen
  Badge-Punkt; Punkte bleiben ausschließlich der Bitte-Rolle vorbehalten
  (FB-4, P-1).
- **SEARCH-4** [aktiv] [gtk] — Esc ist zweistufig und gilt für die ganze
  Suchleiste: Mit Text leert das erste Esc die Query, lässt die Leiste offen
  und das Feld fokussiert; bei leerem Feld klappt Esc die Leiste ein. Eine
  Query wird nie durch Einklappen unsichtbar, ohne dass ihr Chip sie trägt.
- **SEARCH-5** [aktiv] [gtk] — Einklappen beendet nur die Eingabe, nicht den
  Filter. Query, Treffer und Such-Chip bleiben erhalten, bis der Nutzer sie
  explizit über Esc, Chip oder „Clear all" entfernt.

## R. New Releases

- **NR-1** [aktiv] [core] — Eine bibliotheksweite MusicBrainz-Pipeline ist
  die einzige Wahrheit für neue Releases und spätere Artist-News-Ansichten.
  Artist-MBIDs kommen zuerst aus Tags, sonst aus einer persistierten
  Namensauflösung inklusive negativer Ergebnisse; Artists werden nach
  Play-Count priorisiert. Pro Artist bleiben höchstens fünf reguläre Alben
  oder EPs der letzten 90 Tage sowie ausschließlich zukünftige Singles;
  unvollständige Daten gelten nie als zukünftig, Sekundärtypen bleiben draußen.
- **NR-2** [aktiv] [gtk] — Release-Cover laden lazy über Cover Art Archive
  (`/release-group/{mbid}/front-250`). Ein fehlendes Cover ist Normalzustand
  und zeigt sofort eine gleich große Kachel aus gespeicherter Artist-
  Akzentfarbe plus Initialen — niemals ein Loch oder einen Dauer-Spinner.
- **NR-3** [aktiv] [gtk] — Die Header-Lupe ✦ erscheint nur bei vorhandenen
  Einträgen und trägt einen Badge ausschließlich für `seen_at IS NULL`.
  Öffnen stempelt die gelistete Episode als gesehen; sie badgt nie erneut,
  erst ein später neu gefundener Eintrag erzeugt wieder einen Badge (FB-4).
- **NR-4** [aktiv] [gtk] — „See all" öffnet einen echten Digest-Ort mit
  Back/Forward-Historie, aber ohne Sidebar-Eintrag. Releases lassen sich dort
  verbergen; vorhandene Hidden-Einträge halten „See all" erreichbar und die
  Fußzeile „N hidden · Show" macht sie rückholbar. Ein künftiges „Remind me"
  bleibt bis zu einem eigenen Scheduler ausdrücklich außerhalb dieser Regel.
- **NR-5** [aktiv] [gtk] — Das Popover ist transient und verändert den
  Navigations-Stack nie. Erst „See all" navigiert regulär in den Digest-Ort;
  Schließen kehrt ohne Zustandsverlust zur aktuellen Ansicht zurück.
- **NR-6** [aktiv] [gtk] — „Fetch now" ersetzt während des Abrufs sein
  Refresh-Icon durch einen Spinner und zeigt sonst das Alter der letzten
  Aktualisierung. Offline oder Fehler zeigen weiter den letzten Cache samt
  Alter und nur einen dezenten Inline-Hinweis im Fuß — nie ein Fehlerbanner.
- **NR-7** [aktiv] [gtk] — New Releases ist ein Plugin auf der Plugins-Seite,
  standardmäßig aus und mit Privacy-Untertitel „contacts MusicBrainz" sowie
  Auswahl „Top artists only / all artists". Bei ausgeschaltetem Modul gibt es
  weder Fetch noch ✦; Cover-, Portrait- und Lyrics-Module gehören nicht zu
  dieser Regel und werden im Folge-Branch `feat/network-opt-in` geregelt.
- **NR-8** [aktiv] [gtk] — Das Einschalten des Moduls ist die Zustimmung und
  löst deshalb sofort den ersten Abruf aus: `set_enabled(true)` stößt einen
  Fetch an. Solange nie erfolgreich abgerufen wurde, bleibt ✦ **sichtbar** und
  trägt einen Leerzustand („Checking for new releases…" während des Laufs,
  danach „No upcoming releases from your artists"). Erst nach dem ersten
  abgeschlossenen Durchlauf greift NR-5 wieder normal.
  Zwei Kanten: Ein **fehlgeschlagener** erster Abruf (offline) hält ✦ sichtbar
  mit Retry-Leerzustand, statt den Knopf verschwinden zu lassen — sonst
  entsteht erneut „eingeschaltet, aber weg". Und der Erst-Leerzustand trägt
  **keinen** Badge-Punkt: er ist Rückmeldung, keine Bitte (P-1).
  *Grund:* NR-5 wurde formuliert, als Befüllen garantiert war. Opt-in hat den
  Dauerzustand „aktiv, nie befüllt" geschaffen, für den es keinen Einstieg gab
  — ✦ erscheint nur bei Einträgen, „Fetch now" sitzt im Popover hinter ✦, und
  einen Start-Abruf gibt es nicht. NR-8 schließt diese Schleife, ohne NR-5 zu
  kippen. Datenschutzlich unverändert: Netzverkehr entsteht ausschließlich nach
  ausdrücklicher Aktivierung, nur sofort statt nie.

## S. Flächen & Geometrie

<!-- Sektionsbuchstabe: R (New Releases) ist die letzte vergebene; S schließt
     an. Anlass sind vier Fälle an einem Tag (2026-07-18), die alle mit
     grünem Test durchkamen und erst im Screenshot auffielen — Ledger:
     docs/superpowers/plans/2026-07-18-style-explicit-rule.md. -->

Was sichtbar wirken soll, muss explizit gesetzt sein. Geerbte oder
Framework-Defaults zählen nicht als gesetzt: Sie sind der häufigste Grund,
warum eine Property gesetzt ist und trotzdem nichts passiert.

- **STYLE-1** [aktiv] [gtk] — **Wirkung explizit, nicht geerbt.** Jede Fläche,
  die sich vom Inhalt absetzen soll (Headerbar, eingeblendete Leisten,
  Sidebar-Kanten, Panels), trägt Hintergrund **und** Trennlinie ausdrücklich;
  jede verbindliche Geometrie (feste Breiten, Mindesthöhen) wird gegen ihre
  tatsächliche Allokation geprüft. `flat` bleibt genau dort, wo bewusst
  **keine** Abgrenzung gewollt ist. Bekannte Fallen, die diese Regel
  adressiert: `AdwToolbarView` mit `ToolbarStyle::Flat` unterdrückt
  Bar-Hintergründe (auch `@headerbar_bg_color`); eine `AdwHeaderBar` ohne
  Titel-Widget rendert ersatzweise den Fenstertitel (`show-title` muss
  zusätzlich aus); ein `GtkLabel` ohne `ellipsize` meldet seinen vollen Text
  als **Mindest**breite und hebelt jedes `max-width` des Containers aus;
  `AdwOverlaySplitView` rechnet ohne `sidebar-width-unit = Px` in `sp`.
  **Testregel:** Absicht darf geprüft werden, aber bei Flächen und Geometrie
  muss das **Ergebnis** belegt sein — nicht „Property X ist gesetzt", sondern
  „die Fläche hat sichtbaren Hintergrund" bzw. „die Spalte bleibt bei schmalem
  Fenster auf ihrer Breite". Was das Framework garantiert, wird auf Existenz
  getestet; was ausbleiben kann, auf Wirkung (wie TIP-1a/2a und SEARCH-2).
  Ist eine Schnittstelle im Test-Build ausgeblendet (z. B. `SectionModel` per
  `cfg`), zählt nur der E2E-Beleg — „grün" ist dort strukturell bedeutungslos.

## T. Netz-Features opt-in

- **NET-1** [aktiv] [gtk] — Automatische und massenhafte Netzabrufe sind
  opt-in. Cover-Downloads, Artist-Portraits und New Releases starten nur bei
  eingeschaltetem Modul; Online-Lyrics haben ebenfalls einen Schalter, damit
  vollständig netzfreie Nutzung möglich bleibt. Ein Ausschalten wirkt sofort
  und versteckt bereits lokal gecachte Bilder nicht.
- **NET-2** [aktiv] [core] — Updates schützen nachweisbare bisherige Nutzung:
  vorhandene heruntergeladene Cover bzw. Portraits aktivieren ihr Modul,
  bestehende Bibliotheksdatenbanken behalten Online-Lyrics, und ein zuvor
  aktives `artist_news` wird als aktives New-Releases-Modul übernommen.
  Negative Cache-Marker gelten nicht als Nutzung; frische Installationen
  starten mit allen vier Netz-Modulen aus.
- **LYR-1** [geplant] [core] — Lokale eingebettete Songtexte und `.lrc`-
  Sidecars werden unabhängig vom Online-Lyrics-Modul angezeigt. Reprise liest
  diese lokalen Formate heute noch nicht; die Regel bleibt bis zu dieser
  eigenen Formatfunktion geplant.
- **LYR-2** [aktiv] [gtk] — LRCLIB wird ausschließlich bei offenem Lyrics-
  Tab, fehlendem lokalen Text und eingeschaltetem Online-Lyrics-Modul
  kontaktiert. Es gibt weder Prefetch noch Batch-Abruf für kommende Queue-
  Einträge.
- **LYR-3** [aktiv] [gtk] — Bei offenem Lyrics-Tab, fehlendem Text und
  ausgeschaltetem Modul zeigt eine zentrierte StatusPage Icon, Titel
  „Online lyrics are disabled", Untertitel „Enable them to load missing
  lyrics automatically" und „Enable in Settings" als Deep-Link zur kurz
  hervorgehobenen Plugins-Zeile. Solange LYR-1 geplant ist, verspricht dieser
  Zustand keine lokalen eingebetteten Songtexte. Ein eingeschaltetes Modul
  ohne Treffer zeigt stattdessen „No lyrics found".
- **DISCOVER-1** [aktiv] [gtk] — Netz-Features ohne dauerhaft sichtbare
  eigene Fläche erhalten genau einen dezenten, schließbaren Inline-Hinweis am
  Ort der sichtbaren Lücke: Cover ab drei gleichzeitig sichtbaren Fallback-
  Kacheln, Portraits ab drei gleichzeitig sichtbaren Initialen-Avataren und
  New Releases am Kopf der Artists-Ansicht. Sichtbare Evidenz rastet den
  Hinweis ein; einmal gezeigt oder geschlossen kehrt er dauerhaft nicht
  zurück. Der Hinweis ist kein Badge und kein Toast.
- **DISCOVER-2** [aktiv] [gtk] — Pro Ansicht ist höchstens eine
  Aktivierungszeile sichtbar. Treffen Portrait- und New-Releases-Hinweis in
  der Artists-Ansicht zusammen, werden sie zu einer Zeile „Enable network
  features for artists (images & new releases) →" mit Deep-Link auf die
  Plugins-Seite kombiniert; zwei gestapelte Aktivierungszeilen sind verboten.
## U. UI-Politur, Kontrast & ansichtsübergreifender Kontext

<!-- Entscheidungen und die Abgrenzung zu Batch B stehen in
     docs/superpowers/plans/2026-07-18-ui-polish-beschluesse.md. -->

- **SEARCH-6** [aktiv] [gtk] — Lupe und Ctrl+F toggeln die Suchleiste
  beidseitig (zeigen ↔ verstecken). Das Verstecken löscht die Query nie: bei
  nicht leerer Query bleibt ihr Chip sichtbar und die Lupe im
  `:checked`-Akzentstil (FIL-1, SEARCH-3/5).
- **LYR-4** [aktiv] [gtk] — Die Zentrierung der aktiven Lyrics-Zeile wird
  am Songanfang nach oben geklemmt. Solange nicht genug Kontextzeilen über
  der aktiven Zeile liegen, sitzt der Textblock oben; erst mit genügend
  Vorlauf wandert die aktive Zeile in die Mitte.
- **STYLE-2** [aktiv] [gtk] — Content und Tracktabelle verwenden die
  `.view`-Stufe; linke Sidebar und rechtes Now-Playing-Panel verwenden
  gemeinsam die eine Stufe höhere `sidebar_bg`-Fläche des aktiven Themes.
  Beide Flanken tragen an ihrer Innenkante eine 1-px-Hairline. Es gibt keine
  pane-spezifische Nachtönung und keine hartkodierte Pane-Fläche.
- **STYLE-3** [geplant] [gtk] — Zwei Akzentrollen bleiben getrennt: der feste
  App-Akzent (`@accent_color`) bezeichnet dauerhafte UI-Bedeutung wie
  Selektion, Ratings, aktive Toggles, Links, Chips und Fokus; der dynamische
  Playback-Akzent (`@reprise_player_accent`) bezeichnet ausschließlich den
  laufenden Track wie Play/Pause, Waveform, Playing-Row, EQ, Glow und
  GRID-1-Innenring. Ein Element mischt die Rollen nie.
- **CONTRAST-1** [aktiv] [gtk] — Es gibt drei zentrale Textstufen: Primär
  ungefähr 0,95 für Titel und Werte, Sekundär ungefähr 0,7 für Artist,
  Status, Metadaten und Spaltenköpfe, Hint ungefähr 0,5 für Platzhalter,
  Hinweise und deaktivierte Sekundärtexte. Passende Adwaita-Named-Colors
  haben Vorrang vor eigenen Alphas; pro Element wird nicht nachgetönt.
- **CONTRAST-2** [aktiv] [gtk] — Jede „N tracks · Dauer"-Statuszeile ist
  eine echte untere Leiste mit definierter Fläche und oberer Hairline. Sie
  reserviert eigenen Platz und überdeckt nie eine Trackzeile; erst gegen
  diese feste Fläche wird ihr Sekundärtext-Kontrast bestimmt.
- **CONTRAST-3** [aktiv] [gtk] — Statuszeilen, Spaltenköpfe,
  Sidebar-Sektionslabels und Kartenmetazeilen erreichen gegen ihre jeweilige
  Fläche mindestens 4,5:1. `.caption` plus Sekundärstufe gilt dabei als
  Kleinschrift und benötigt dieselbe Prüfung wie Hint bei Normalgröße.
- **NAV-10** [aktiv] [gtk] — Der laufende Kontext bleibt in allen Ansichten
  mit einer gemeinsamen Playback-Akzent-Markierung sichtbar; beim ersten
  Eintritt einer Ansicht wird er einmalig aufgedeckt, spätere Wechsel stellen
  NAV-5s gemerkten ID-plus-Offset-Anker wieder her. Explizites „Go to
  album/artist" springt immer deterministisch; Selektion folgt der Wiedergabe
  nie.
- **QUE-7** [aktiv] [gtk] — Up Next besteht aus der manuellen Queue plus
  einem virtuellen, benannten Kontext-Tail mit Count. Der Tail wird nicht als
  Einzelzeilen materialisiert, sondern nur im sichtbaren Fenster gerendert;
  die Sidebar-Zeile „Queue" zählt ausschließlich die manuelle Queue und zeigt
  bei null keinen Zähler.
- **QUE-8** [aktiv] [gtk] — Drag-Reorder existiert ausschließlich in „Next
  in Queue". Die manuelle Sektion ist umsortierbar; ein Drag aus „Continuing"
  nach oben materialisiert genau diesen Eintrag in der manuellen Sektion.
  Multi-Select, Clear, Save-as-Playlist und das vollständige Kontextmenü
  bleiben in der Queue-ColumnView.
- **NPP-11** [aktiv] [gtk] — Die Panel-Ansichten verwenden einen
  zentrierten `AdwViewSwitcher` als Title-Widget und degradieren bei schmalem
  Fenster adaptiv zu einer unteren `AdwViewSwitcherBar` oder einem
  icons-only `AdwInlineViewSwitcher` per `AdwBreakpoint`. Umsetzung in Batch
  B; siehe Beschlussdokument.

## V. My Stats

- **STATS-0** [aktiv] [core] — Ein „play" ist überall dieselbe Sache:
  mindestens 50 % des Tracks oder mindestens vier Minuten gehört. Genau diese
  Ereignisse stehen in `listen_events`, und die My-Stats-Ansicht rechnet
  ausschließlich aus ihnen — Hero-Zeit, Plays, Top-Listen, Spotlight, Genres,
  Clock und Highlights sind Projektionen derselben Zeilenmenge. Der laufende
  Zähler `tracks.play_count` speist die Ansicht nie; Zeit und Anzahl können
  daher nicht auseinanderlaufen. Tages- und Stundengrenzen entstehen nicht in
  SQL: die Kernfunktionen nehmen eine Zeitzone als Parameter und bucketen jedes
  Ereignis einzeln durch sie hindurch, damit Sommer-/Winterzeit-Wechsel keine
  Grenze verschieben. Alles ist lokal: kein Netz, keine Cloud, keine
  Fremdquelle wird eingemischt.
- **STATS-1** [aktiv] [core] — Der Kopf zeigt die Gesamt-Hörzeit groß in vollen
  Stunden („68 hours"; unter einer Stunde in Minuten, nie „0 hours"), eine
  Vergleichs-Pill „▲ N % vs <Vorperiode>" im teal App-Akzent (nie im
  Cover-Akzent) und die Subzeile „N plays · Ø X min/day · N artists" auf
  Sekundär-Ton. Rechts steht das Zeitraum-Dropdown („<Jahr> so far / <Vorjahr> /
  All time / Last 30 days"). Darunter läuft ein schlankes Area-Ribbon der
  Hörzeit, dessen Achse **exakt dem gewählten Zeitraum** folgt — „2026 so far"
  zeigt Jan–Jul, nie ein rollendes 12-Monats-Fenster. Der laufende Bucket ist
  offen markiert (gestrichelt, hohler Punkt), der Peak gesetzt; Hover nennt den
  exakten Wert. Fehlt eine Vorperiode mit Hörzeit, entfällt die Pill. Die Pill
  **benennt** die verglichene Spanne, statt „previous period" zu sagen. Die
  Vorperiode ist gleich lang **und** saisonal deckungsgleich: „2026 so far"
  wird gegen Jan–Jul 2025 gerechnet und heißt „vs same period 2025", nie gegen
  die gleich lange Strecke unmittelbar davor (Jun–Dez 2025) — Hörzeit ist
  saisonal, sonst stünde Sommer gegen Winter. Ein volles Kalenderjahr wird
  gegen das ganze Vorjahr gerechnet („vs 2025"), das rollende Fenster gegen die
  30 Tage direkt davor („vs previous 30 days"), denn dafür gibt es keine
  wiedererkennbare Kalenderentsprechung ein Jahr zurück. Der 29. Februar klemmt
  im Vorjahr auf den 28. „All time" hat keine Vorperiode und trägt nie eine
  Pill.
- **STATS-2** [aktiv] [core] — Das Artist-Spotlight ist das Herzstück:
  #1-Artist mit großem Cover und Rang-Badge, Eyebrow „YOUR #1 ARTIST", Name,
  Zeile „N plays · N h · N % of your artist listening" — der Anteil bezieht
  sich auf die Hörzeit mit Artist-Zuordnung, dieselbe Grundgesamtheit, die die
  Rangliste bildet, nicht auf jeden Play —, drei Top-Track-Chips sowie die
  Aktionen Play (Container-Play über die Trackliste des Artists) und
  „Go to artist" (regulärer NAV-Push mit Back-Historie). Hinter dem Cover liegt
  ein dezenter Cover-Akzent-Glow — der Cover-Akzent bleibt Playback-Elementen
  vorbehalten. Darunter nennt eine Ghost-Zeile die Ränge 2–5.
- **STATS-3** [aktiv] [core] — Das Genre-Spektrum ist **eine** horizontale
  Segment-Leiste in Teal-Abstufungen mit Legende (Punkt · Name · %), gespeist
  aus den Genre-Tags der Bibliothek. Die fünf stärksten Genres bilden eigene
  Segmente, der Rest wird zu „Other" gebündelt; Tracks ohne Genre zählen weder
  als Segment noch als „Other". Die Leiste ist reine Anzeige und keine
  Navigation: Segmente und Legende sind nicht klickbar.
- **STATS-4** [aktiv] [core] — Unter dem Spektrum steht eine asymmetrische
  Reihe (1.35fr / 1fr): links die Listening Clock als 24-Stunden-Histogramm aus
  den Timestamps mit teal hervorgehobenen Peak-Stunden und Caption
  („Peak 11 PM–1 AM · night owl"), rechts vier Highlight-Kacheln — Streak
  (längste Folge aufeinanderfolgender lokaler Tage mit ≥ 1 play), Discovered
  (im Zeitraum erstmals gespielte Tracks), Busiest day, On repeat (höchste
  Play-Zahl) — plus der CTA „Mix from <Top-Genre> · Create". Er mischt genau
  die Trackgruppe des angezeigten Genres, nie die Tracks, die zufällig genau
  so geschrieben sind (STATS-9). Ist die Gruppe als Regel ausdrückbar — also
  eine einzige Schreibweise —, entsteht eine echte Smart Playlist; fasst sie
  mehrere Schreibweisen zusammen, entsteht stattdessen eine gewöhnliche
  Playlist mit genau den Tracks der Gruppe, denn die Regel-Engine verknüpft
  ihre Regeln nur per UND und kennt keine Alternative. Gemischt wird immer
  **ein** Genre; ohne Genre im Zeitraum entfällt der CTA. Tages- und
  Stundengrenzen folgen der lokalen
  Zeit des Nutzers, nicht UTC. Im schmalen Fenster klappt die Reihe per
  AdwBreakpoint einspaltig, ohne dass sich die Reihenfolge ändert. Die Reihe
  ist so bemessen, dass ihre beiden Mindestbreiten zusammen unter dem
  Breakpoint bleiben — sonst gäbe es Fensterbreiten, in denen sie noch
  nebeneinander steht, aber schmaler ist als sie braucht.
- **STATS-5** [aktiv] [core] — Top Tracks steht über die volle Breite:
  nummerierte Liste mit Cover, Titel und Artist, relativem Play-Balken und
  Play-Count, mit Sort-Toggle „by plays / by time". Der Balken ist relativ zum
  Spitzenreiter der Liste, nie zu einem absoluten Maximum.
- **STATS-6** [aktiv] [core] — Leere und dünne Datenlagen werden nie als
  leere Diagramme gezeigt. Ohne Hörhistorie im Zeitraum erscheint ein
  freundlicher Leerzustand („Start listening to see your stats") statt Achsen
  mit einem einsamen Balken. Bei dünner Datenlage wird die Granularität feiner
  (Tage bzw. Wochen statt größtenteils leerer Monate).
- **STATS-6a** [aktiv] [gtk] — Ein Fehler ist kein Leerzustand: schlägt die
  Abfrage fehl, erscheint eine eigene Fehlerseite („Your stats could not be
  read"), nie die Einladung „Start listening to see your stats". Sichtbarkeit
  entsteht dabei über die Seitenumschaltung, nicht über zusätzliches
  Ein-/Ausblenden einzelner Sektionen darunter.
- **STATS-6b** [aktiv] [gtk] — Importierte Hörhistorie ist kein normaler
  Leerzustand: enthält der gewählte Zeitraum keine `listen_events`, während
  `SUM(tracks.play_count) > 0` ist, erscheint eine eigene Statusseite („Your
  Rhythmbox history was imported"), nennt die Zahl der importierten Plays und
  erklärt, dass detaillierte Statistiken ab jetzt mit dem Hören in Reprise
  entstehen. Sobald im Zeitraum echte `listen_events` vorliegen, verschwindet
  die Seite selbständig. Eine frische Bibliothek ohne Zähler behält den
  regulären Leerzustand. `tracks.play_count` entscheidet ausschließlich über
  diese Nachricht und speist gemäß STATS-0 weiterhin keine Statistik.
- **STATS-7** [aktiv] [gtk] — My Stats ist kuratiert, nicht frei editierbar:
  kein Drag-and-Drop-Widget-Board. Ein ⋮-Menü „Customize" blendet die Sektionen
  Clock, Genres und Highlights per CheckButton ein und aus; die Auswahl bleibt
  über Sitzungen erhalten. Mehr enthält das Menü nicht — das Spotlight ist
  fest das Artist-Spotlight. Die Reihenfolge der Sektionen ist fix, Größen sind
  nicht manuell veränderbar — Anpassung an die Fensterbreite geschieht
  ausschließlich per AdwBreakpoint.
- **STATS-8** [aktiv] [gtk] — In My Stats gibt es keine Filter-Zeile und
  keine Suche der Trackliste — das ist eine andere Ansicht. Die rechte
  Now-Playing-Spalte verhält sich wie überall. Das Zeitraum-Dropdown ist der
  einzige Ansichts-Regler dieser Ansicht.
- **STATS-9** [aktiv] [core] — **Dedup:** Unsaubere Tags dürfen Zahlen nicht
  zersplittern. Top Artists, Top Genres, Album-Artist-Aggregate, das Spotlight
  und jede Trackauswahl, die von einer dieser Zeilen ausgeht, benutzen **eine
  einzige** Schlüsselauflösung — nie eine zweite Formel pro Aufrufer.
  **Zuerst der Name:** Trim, Unicode-Kleinschreibung (`str::to_lowercase`, also
  über ASCII hinaus, aber kein volles Casefold — „Straße" bleibt von „STRASSE"
  getrennt), Whitespace-Kollaps und Diakritika-Faltung (NFKD ohne Combining
  Marks). „Lorna Shore", „lorna shore" und „Lorna Shore " sind damit ein
  Eintrag mit einer Summe. **Danach erst die MBID, und nur innerhalb der
  Namensgruppe:** sie ist die stabile Identität dieser Gruppe und führt weitere
  Namensgruppen mit derselben Identität zusammen; sie darf eine Namensgruppe
  aber **nie spalten**, denn MBIDs sind dünn besetzt und hängen typischerweise
  an genau einer Schreibweise („Sigur Rós" mit, „Sigur Ros" ohne). Tragen
  mehrere MBIDs eine Namensgruppe, gewinnt die meistgespielte, bei Gleichstand
  die alphabetisch erste — der Schlüssel hängt nie an der Zeilenreihenfolge.
  `tracks.artist_mbid` beschreibt die rohe `artist`-Spalte: nennt der
  Album-Artist der Zeile einen anderen Act, gilt die MBID dort **nicht**, sonst
  zöge ein Gastbeitrag zwei fremde Bands in eine Zeile, einen „Play" und eine
  Tag-Editor-Einladung. Weil Zeitraum und Gesamtkatalog verschiedene
  Grundgesamtheiten sind, dürfen ihre Auflösungen abweichen: findet eine
  Trackauswahl zum Schlüssel nichts, fällt sie auf die Namensgruppe zurück, und
  ein leeres Ergebnis wird protokolliert, nie stillschweigend verschluckt.
  Der Schlüssel existiert nur zur Laufzeit: keine gespeicherte Spalte, und die
  Ansicht schreibt **niemals** Tags zurück — Statistik ist lesend.
  Angezeigt wird stets eine echte Original-Schreibweise
  der Gruppe (die häufigste; bei Gleichstand die zuletzt gespielte, dann
  alphabetisch), nie die normalisierte Form. **Geraten wird nie:**
  zusammengefasst wird ausschließlich, was nach Normalisierung exakt gleich ist
  — kein Fuzzy-Matching, keine Levenshtein-Distanz, kein Präfix-Merge, also
  bleibt „Lorna Shore Band" von „Lorna Shore" getrennt. Fasst eine Gruppe
  mindestens zwei Schreibweisen zusammen, weist ein dezenter Hinweis am
  Listeneintrag darauf hin und führt in den Mehrfach-Tag-Editor der betroffenen
  Tracks; das Vereinheitlichen bleibt eine Einladung, nie ein automatischer
  Schreibvorgang.

## W. Buttons & Interaktionszustände

<!-- Sektionsbuchstabe: V (My Stats) ist die letzte auf main vergebene
     Sektion; W schließt lückenlos an. Die Buchstabenlage wurde beim Einfügen
     gegen den main-Stand verifiziert. Achtung beim Merge: feature/tag-rework
     beansprucht auf einer älteren Basis ebenfalls ein „W" (Library Doctor),
     dort ist aber die ganze Lage ab T verschoben — der Buchstabe ist bei
     dessen Rebase neu zu vergeben, nicht hier. -->

Ein Button, der auf Zeigen und Drücken nicht antwortet, ist für den Nutzer
kaputt, auch wenn er funktioniert. Reprise hatte das Problem nicht, weil
Adwaita zu wenig liefert, sondern weil eigenes App-CSS auf
`STYLE_PROVIDER_PRIORITY_APPLICATION` läuft und die Theme-Regeln unabhängig
von der Spezifität schlägt: ein *zustandsloses* `background-color: transparent`
auf einem Button-Selektor löscht Adwaitas `:hover` und `:active` gleich mit.
Deshalb gilt hier nicht „mehr Effekte", sondern: **ein Zustands-Vokabular,
zentral definiert, überall angewandt** (BTN-4, die Button-Lesart von STYLE-1).

- **BTN-1** [aktiv] [gtk] — Jeder klickbare Button hat vier unterscheidbare
  Zustände, und jeder ist sichtbar. **Rest** ruht. **Hover** hebt die Fläche an:
  Icon-Buttons bekommen einen sichtbaren Background (weiß ~8 %), Cursor
  `pointer`, Übergang im Micro-Token (150 ms) — nicht nur ein Schatten.
  **Active/Pressed** sinkt sofort ein: Fläche weiß ~14 % plus `scale(0.94)`,
  damit der Klick landet. **Focus-visible** ist ein Akzent-Ring für die
  Tastatur und nie der Hover-Zustand allein. Der Cursor ist dabei
  Widget-Sache, nicht CSS: GTK4-CSS kennt keine `cursor`-Property, also setzt
  ihn `style::buttons::arm` — und zwar nur auf app-eigenen Flächen, damit
  Dialoge und Preferences nativ bleiben.
  <!-- Bewusste HIG-Abweichung: Adwaita-Buttons ändern den Cursor nicht. -->
- **BTN-2** [aktiv] [gtk] — Toggle-Buttons zeigen ihren Zustand dauerhaft, nicht
  nur im Moment des Klicks. Shuffle und Repeat sind beide `GtkToggleButton` und
  sprechen dasselbe `:checked`: Akzentfläche im App-Akzent (nie im Cover-Akzent)
  plus ein kleiner Punkt unter dem Icon als zweites, **nicht-farbliches** Signal
  — Farbe allein trägt bei Farbenfehlsichtigkeit nicht. Der Zustand überlebt
  Hover und Unhover; Hover moduliert nur die Helligkeit der Fläche und kippt die
  Zustandsanzeige nie. Repeat-One schaltet zusätzlich auf das Icon mit der „1".
  <!-- Der Punkt ist eine zweite Background-Ebene (radial-gradient), kein
       Extra-Widget: er funktioniert so auch auf runden Buttons. „Gefülltes
       Icon" aus der Vorlage ist nicht umsetzbar — der Adwaita-Symbolic-Satz
       hat für shuffle/repeat keine gefüllte Variante; das Füllsignal liefert
       die Akzentfläche. -->
- **BTN-3** [aktiv] [gtk] — Nicht alle Buttons sind gleich laut, und die
  Lautstärke ist eine Stufe, keine Einzelfallentscheidung. **Primär** (Play,
  Create Mix, Apply): Akzentfläche, stärkster Hover und Press. **Standard**
  (Icon-Transport, Header-Aktionen): flach, Hover-Background, dezenter Press.
  **Tertiär** (Menüeinträge, Listenzeilen): nur Background-Hover, kein Scale —
  eine Zeile in einer Liste darf unter dem Cursor nicht springen. Der große
  Play/Pause-Knopf ist die Hauptaktion und darf beim Press sichtbarer antworten
  als seine Nachbarn: zusätzlich ein Ring im Playback-Akzent.
- **BTN-4** [aktiv] [gtk] — Hover, Active und Focus sind **einmal** definiert
  (`ui/style/buttons.rs`) und werden überall angewandt, per Klasse oder — wo
  Adwaita die Buttons intern baut — per Selektor aus derselben Liste. Kein
  Per-Button-Nachtönen. Eine Fläche darf ihre eigene *Ruheoptik* behalten
  (Füllung, Radius, ein gestalteter Hover wie im Kontextmenü), aber niemals
  `:active` oder `:focus-visible` lokal definieren. Hover und Press sind Alphas
  über `currentColor` — nicht über den Akzent, der im Glas der Player-Bar und
  der Now-Playing-Tableiste versackt, und nicht über ein festes Weiß, das in den
  hellen Paletten unsichtbar wäre. `currentColor` ist die Vordergrundfarbe der
  Fläche selbst, also wird immer auf dem Tint gemessen und nie auf
  Nulluntergrund. Bei `gtk-enable-animations = false` entfallen Scale und
  Übergang, **der Zustandswechsel bleibt** und schaltet hart — Rückmeldung darf
  nie ganz verschwinden.
  <!-- CSS-`transition` und `@keyframes` folgen dem Setting von selbst (MOT-7,
       Probe `mot_7_css_honours_enable_animations_setting`). Ungegated bliebe
       nur `transform` in `:active` — ein statischer Zustandsstil, kein
       Übergang. Den neutralisiert der Provider in `style/reduced_motion.rs`. -->

---

Wenn beim Testen ein Fall auftaucht, den keine Regel deckt: Regel ergänzen
(Prozessregeln oben), nicht lokal entscheiden.
