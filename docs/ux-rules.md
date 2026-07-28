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
- **P-5** [ersetzt durch BROWSE-6] — Die frühere Regel koppelte den lokalen
  Hörverlauf an den Library-Eintrag. BROWSE-6 trennt historische Ereignisse
  vom aktuellen Katalog.
- **P-6** [aktiv] [core] — Evidenz-Regel: Was beweisbar da ist, wird
  angezeigt/geheilt (Mount-Event, Resurrect); was beweisbar weg ist, wird
  sofort ehrlich markiert (Eject). Vermutungen (unmounted) sind nie
  Lösch-Grundlage.

## B. Navigationsmodell

- **NAV-1** [ersetzt durch BROWSE-1] — Sidebar = Orte, Content = Modus. Sidebar wählt
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
- **NAV-5** [ersetzt durch BROWSE-2] — Modus-Gedächtnis (Scroll + Selektion je
  Tracks/Albums/Artists) gilt nur innerhalb der Session; auch Sidebar-/
  Ortswechsel erhalten Scroll + Selektion des verlassenen Modus. Der
  Scroll-Anker besteht aus Track-/Album-ID plus Offset, nie aus einem rohen
  Pixelwert; Re-Sort und Insert halten dadurch den Inhalt an seiner Position.
  START-1 restauriert über Neustarts ausschließlich die zuletzt aktive Ansicht
  samt Scroll-Position; alle anderen Modi starten oben, unselektiert.
- **NAV-6** [aktiv] [e2e] — Suche (Ctrl+F) filtert die aktuelle Ansicht
  live; Esc leert und schließt. Suche navigiert nie selbst.
- **NAV-7** [aktiv] [e2e] — Hamburger-Menü: „Scan Library" → startet Scan,
  bleibt in der Ansicht (Karte erscheint). „Preferences" →
  Preferences-Fenster. „Keyboard Shortcuts" → Shortcuts-Overlay. „About
  Reprise" → About-Dialog. Kein Menüpunkt wechselt kommentarlos die
  Content-Ansicht.
- **NAV-8** [geplant] [gtk] — My Stats ist ein Sidebar-Ort wie jeder andere:
  volle Content-Fläche, Headerbar mit Suche bleibt stehen (Suche dort
  disabled/ausgeblendet ist erlaubt, aber die Leiste bleibt).
- **NAV-9** [ersetzt durch NAV-9b/GRID-5] — Ursprünglich teilten Cover/Titel
  der Player-Leiste und Ctrl+L denselben Sprung zur Heimat des spielenden
  Tracks. Aufgeteilt in Track-Ursprung per Ctrl+L (NAV-9a) und Album-Grid-
  Reveal per Player-Oberflächen (GRID-5).
- **NAV-9a** [ersetzt durch NAV-9b] — Ctrl+L navigiert zur Herkunftsansicht des
  geladenen Tracks, selektiert dessen Zeile und zentriert sie ohne
  scrollIntoView-Kantenkleben. Der Sprung pusht auf den globalen
  History-Stack; Back kehrt zum vorherigen Ort zurück.
- **NAV-9b** [ersetzt durch BROWSE-4] — Ctrl+L und Player-Metadaten wurden
  früher als gemeinsamer Track-Sprung behandelt. BROWSE-4 trennt Track-,
  Album- und Interpret-Intents appweit und behält den expliziten Track-Sprung
  für Ctrl+L und den Player-Titel bei.
- **NAV-11** [aktiv] [gtk] — Jeder bedienbare Sidebar-Eintrag exponiert
  gegenüber Assistenztechnik eine eigene Bezeichnung, eine interaktive Rolle
  und eine auslösbare Aktion. Sektionsüberschriften bleiben nicht bedienbar,
  werden aber semantisch als Überschriften exponiert.
- **NAV-12** [ersetzt durch NAV-2] — Die globale Zurück-Historie als benannter
  ‹-Button in der Headerbar (deaktiviert ohne vorherigen Ort, aktiv nach einer
  Navigation, stellt beim Auslösen den vorherigen Ort samt Fokus wieder her)
  gehört im Single-Track-Browser zum NAV-2-History-Komplex; Album und Interpret
  sind keine eigenen Ansichten mehr, sondern Scopes der Musikliste.
- **NAV-13** [ersetzt durch NAV-10a] — Wiedergabestart ist keine Navigation:
  Enter oder Doppelklick auf eine Track-Row lässt Selektion, Tastaturfokus und
  Viewport unverändert; nur die Now-Playing-Markierung wechselt. Die Trennung
  von Markieren und Scrollen im einen Track-Browser regelt jetzt NAV-10a.

## C. Abspielen, Queue, Shuffle, Filter

- **PLAY-1** [aktiv] [gtk] — Queue-Quelle = sichtbare Trackliste. „Was du
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
- **PLAY-3b** [aktiv] [gtk] — Filter nachträglich ändern fasst eine bereits
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
- **PLAY-7** [ersetzt durch PLAY-7a] — Die Player-Leiste ist eine strukturelle
  Abgrenzung, kein Overlay: Sie beansprucht ihre eigene Höhe im Layout, und
  kein Content-Element (Trackliste, Sidebar, rechte Info-Spalte) läuft je
  unter oder hinter ihr durch. Ihr Hintergrund ist opak.
  <!-- REVIEW: Regelvorschlag -->
- **PLAY-7a** [ersetzt durch PLAY-7b] — Header, geöffnete Suche und Player-Leiste
  liegen als globale Glaszonen über allen Bibliotheksansichten. Der Content
  läuft sichtbar darunter; sein Scroll-Anfang und -Ende erhalten exakt die
  tatsächlich allokierte Höhe der überlagernden Top-/Bottom-Zone als
  Scroll-Inset, sodass keine letzte Zeile verdeckt oder unbedienbar bleibt.
  Die Player-Leiste funktioniert spiegelbildlich oben und unten.
- **PLAY-7b** [aktiv] [gtk] — Die Player-Leiste ist wieder eine strukturelle
  Abgrenzung statt eines Overlays: Sie beansprucht oben oder unten ihre eigene
  Höhe im Layout, und kein Content-Element läuft unter oder hinter ihr durch.
  Ihr Hintergrund ist opak.

- **PLAY-8** [aktiv] [core] — **Wiedergabe ist ein unveränderlicher Snapshot.**
  Beim Start werden geordnete Track-IDs, Cursor, vollständiger Browser-Ursprung
  und dessen Anzeigename eingefroren. Spätere Navigation, Suche, Facetten oder
  selbst ein Verfeinern auf null Treffer verändern weder Snapshot noch laufenden
  Track. Nach dem letzten Track endet die Wiedergabe bei Repeat Off, sofern
  nicht ein expliziter Up-Next-Eintrag folgt; Löschhygiene regeln PLAY-5a/5b.
- **PLAY-9** [aktiv] [gtk] — Play/Pause startet bei gestoppter Wiedergabe ohne
  geladenen Titel, Queue-Snapshot oder „Play Next" sofort einen zufällig
  gewählten vorhandenen Bibliothekstitel. Dafür entsteht ein unveränderlicher
  Snapshot aus allen vorhandenen Bibliothekstiteln in zufälliger Reihenfolge;
  Missing- und gelöschte Titel sind ausgeschlossen. Bei leerer Bibliothek
  bleibt Play/Pause deaktiviert und die Wiedergabe gestoppt.

## D. Albums- & Artists-Ansicht

- **ALB-1** [ersetzt durch GRID-2/GRID-4] — Ursprüngliche gemeinsame
  Album-Grid-Regel für Hover-Overlay, Aktivierung, Container-Play und
  Kontextmenü; in Bedienung/Aktionen (GRID-2) und Overlay-Optik (GRID-4)
  aufgeteilt.
- **ALB-2** [geplant] [gtk] — Album-Detail: Hero mit Cover + dominanter
  Farbfläche (Akzent-Pipeline), Play all/Shuffle-Pills (PLAY-1a), Trackliste
  nach Disc/Tracknummer. Spielender Track: Akzent-Row + EQ-Icon + bold —
  identisch in jeder Liste der App (eine Markierungssprache).
- **GRID-1** [ersetzt durch BROWSE-1] — Persistenter Playing-Zustand: Das geladene
  Album zeigt unabhängig von Hover und Fokus oben links auf dem Cover das
  gemeinsame EQ-Badge und einen 1.5-px-Innenring um das Cover. Beides nutzt
  `@reprise_player_accent`. Bei Pause bleibt der Ring und die EQ-Bewegung
  friert ein; bei `gtk-enable-animations=false` ist die Glyphe statisch.
- **GRID-2** [ersetzt durch BROWSE-1] — Bedienung und Aktionen: Das native
  GtkGridView bewegt den Fokus mit Pfeiltasten zweidimensional. Enter öffnet
  die Album-Detailquelle als History-Push, Ctrl+Enter ersetzt die Queue durch
  das Album in kanonischer Disc-/Track-Reihenfolge und startet bei Track 1.
  Space bleibt global Play/Pause. Menütaste und Shift+F10 öffnen an der
  fokussierten Kachel dasselbe Menü wie Rechtsklick, exakt mit Play, Play
  next, Add to queue, Go to artist und Edit tags….
- **GRID-3** [ersetzt durch BROWSE-1] — Sichtbarer Fokus und Zustandskomposition:
  Tastaturfokus zeichnet einen 2-px-Außenring in `@accent_color` nur um das
  Cover und zeigt dieselbe Play-Affordance wie Hover. Playing, Fokus und
  Hover bleiben getrennte Zustandslayer: Playing innen, Fokus außen,
  Interaktions-Overlay darüber; kombinierte Zustände verdecken einander
  nicht.
- **GRID-4** [ersetzt durch BROWSE-1] — Bottom-Gradient-Overlay: Hover oder Fokus
  blendet statt einer schwebenden Tooltip-Box einen unten verankerten
  Abdunkel-Gradienten ein. Darin stehen eine dünne Metazeile („13 tracks ·
  47 min") und unten rechts ein Play/Pause-Button in
  `@reprise_player_accent`; Album und Artist bleiben unter dem Cover. Die
  Covermitte bleibt frei. Der Kartencontainer hat keinen Metadaten-Tooltip;
  nur tatsächlich ellipsierte Titel-/Artist-Labels zeigen ihren Volltext.
- **GRID-5** [ersetzt durch BROWSE-4] — Spielendes Album aufdecken: Aktivierung von
  Cover oder Titel in Playerleiste oder Now-Playing-Panel wechselt bei Bedarf
  in die Album-Ansicht, leert ein sichtbares Suchfeld samt Albumfilter,
  scrollt per GtkGridView/Adjustment zur geladenen Albumkachel, fokussiert sie
  und hebt sie rund 1 s hervor. Der Ortswechsel ist ein History-Push; bereits
  im Album-Grid entsteht kein Duplikat. Fehlt die Albumkachel, greift NAV-9b
  ohne Fehlerdialog. `gtk-enable-animations=false` zeigt für dieselbe Dauer
  ein statisches Highlight.
- **GRID-6** [ersetzt durch BROWSE-2/BROWSE-4] — Rückkehrfokus: Back aus einem Album-Detail in
  die Album-Übersicht stellt den Tastaturfokus auf genau der zuvor
  aktivierten Albumkachel wieder her und scrollt sie bei Bedarf sichtbar.
- **GRID-7** [ersetzt durch BROWSE-1] — Die Album-Übersicht trägt hinter ihren Karten
  eine dezente Textur des aktuell spielenden Covers. Das Cover wird pro
  Trackwechsel genau einmal auf 32 px verkleinert und vorgerendert
  weichgezeichnet; beim Zeichnen wird nur diese gecachte Textur skaliert,
  niemals ein Live-Blur über der Liste ausgeführt. Ohne Cover, nach Stop und
  in High Contrast bleibt die Textur unsichtbar. Sie ist nicht interaktiv und
  verwendet den Coverinhalt, färbt aber keine Chrome-Fläche ein.
- **GRID-8** [ersetzt durch BROWSE-1] — Die Album-Übersicht füllt unabhängig von der
  Anzahl sichtbarer Karten die gesamte verfügbare Höhe des Library-Bereichs.
  Ambient-Layer, Inhalt, Grid-Seite und Scroller bleiben nach dem Wechsel aus
  Tracks oder Artists vertikal expandiert; Kartenzeilen werden weder
  abgeschnitten noch auf ihre natürliche Gesamthöhe begrenzt.
- **ART-1** [ersetzt durch BROWSE-1/BROWSE-4] — Artist-Liste: Klick selektiert und zeigt Detail
  rechts; Selection folgt NIE der Wiedergabe, spielender Artist zeigt nur
  Mini-EQ.
- **ART-2** [geplant] [gtk] — Artist-Detail: Hero-Glow (vorberechnete
  Textur, 250 ms Crossfade beim Wechsel), Alben-Reihe (Hover wie ALB-1), Top
  Tracks (Doppelklick spielt gemäß PLAY-2 im Kontext „Top Tracks"). „Show all
  N tracks ›" → Tracks-Modus im Artist-Scope; dessen sichtbarer und per ×
  entfernbarer Scope-Chip ist bereits durch FIL-1c aktiv.
- **FX-1** [geplant] [manuell] — Alle Effekte respektieren
  `gtk-enable-animations=false` (harte Schaltung) und laufen nur GPU-billig
  (Opacity/Transform, vorgerenderte Glows). Keine Live-Blurs in Listen.

## E. MTP / Sync

- **MTP-1** [aktiv] [gtk] — Ein neues verbundenes Android-MTP-Gerät erzeugt
  einen gerätenamenspezifischen Connected-Toast und eine Gerätekarte in der
  Sidebar. Es navigiert nie automatisch aus der aktuellen Ansicht.
- **MTP-2** [ersetzt durch MTP-13]
- **MTP-3** [aktiv] [gtk] — Gerätekarte und offene Geräte-Seite projizieren
  denselben gerätebezogenen Runtime-State. Syncs verschiedener Geräte dürfen
  parallel laufen; Start und Cancel wirken ausschließlich auf das benannte
  Gerät, und ein spätes Progress-Event eines abgebrochenen Laufs wird durch
  dessen Generation verworfen.
- **MTP-4** [aktiv] [gtk] — Eject lebt ausschließlich auf der
  Geräte-Seite. Es ist nur bei einem verbundenen, nicht synchronisierenden
  Gerät aktiv; während Sync und Finishing ist es deaktiviert und erklärt den
  Grund im Tooltip.
- **MTP-5** [aktiv] [gtk] — Beim Abziehen verschwindet die Gerätekarte, der
  offene Geräte-Seite bleibt als „Device disconnected"-Status lesbar, und ein
  laufender gerätebezogener Sync wird abgebrochen. Ein reconnect-fähiges Gerät
  setzt beim Wiederverbinden den verbleibenden sicheren Mirror-Plan fort;
  unvollständige `.part`-Dateien werden vor der nächsten Veröffentlichung
  bereinigt.
- **MTP-6** [aktiv] [gtk] — Finishing wird als vollständiger Fortschritt
  projiziert. Danach zeigt der Lifecycle-Toast den Abschluss oder Fehlerstatus,
  und die Gerätekarte wechselt ohne separaten 100-%-Haltezustand zurück in den
  aktuellen Idle-/Synced-Zustand.
- **MTP-7** [aktiv] [gtk] — Die Geräte-Seite stellt vollständig
  bekannten Speicher als themenfarbene Segmentleiste aus Music, geplantem
  After-sync-Zuwachs, Other und Free dar; dieselben Werte bleiben textuell
  verfügbar. Bei unvollständiger oder inkonsistenter Kapazität verschwindet
  die Leiste, und der Text benennt „unknown“, statt Anteile zu erfinden.
- **MTP-8** [aktiv] [gtk] — Die Geräte-Seite bietet genau drei
  Transferprofile: Opus mit 160 kbit/s als Empfehlung und Standard, MP3 mit
  256 kbit/s als Kompatibilitäts-Fallback sowie unveränderte Originaldateien.
  Ein verlustbehaftetes oder nicht eindeutig als verlustfrei erkanntes
  Quellformat wird unter jedem Profil unverändert kopiert und nie in ein
  anderes verlustbehaftetes Format transkodiert.
- **MTP-9** [aktiv] [gtk] — Die Geräte-Seite benennt den von GIO
  gemeldeten Schreibzugriff des gewählten Zielspeichers als „Writable“,
  „Read-only“ oder „Write access unknown“. Ein sicher schreibgeschütztes Ziel
  sperrt den Sync-Start und erklärt den Grund; unbekannte Angaben werden nicht
  als Schreibfähigkeit ausgegeben und blockieren nicht vorsorglich.
- **MTP-10** [aktiv] [gtk] — Ein fehlerfreier Transfer bleibt „Finishing“,
  bis Reprise den verwalteten Geräteordner erneut gelesen hat. Erst dieses
  erfolgreiche Rücklesen erzeugt den Abschluss-Toast und eine als „Verified“
  bezeichnete Seiten-Zusammenfassung mit der tatsächlich gefundenen Anzahl
  verwalteter Tracks; ein fehlgeschlagenes Rücklesen behauptet keinen Erfolg.
- **MTP-11** [aktiv] [gtk] — Eine untätige Gerätekarte ohne gültige
  Playlist-Auswahl zeigt keine Handlungsaufforderung. Ihre Detailzeile beginnt
  mit dem bekannten Schreibstatus („Writable“, „Read-only“ oder „Write access
  unknown“) und nennt den freien Speicher; echte Scan-, Sync-, Warn- oder
  Auswahlfehler behalten stattdessen „Needs attention“.
- **MTP-12** [aktiv] [gtk] — Jede verfügbare Playlist-Zeile auf der
  Geräte-Seite nennt ihren letzten auf diesem Gerät verifizierten
  Sync-Zeitpunkt in lokaler Zeit. Ohne belastbaren Zeitpunkt steht dort
  ausdrücklich „No verified sync time“. Ein Zeitpunkt wird erst nach
  erfolgreichem Geräte-Readback gespeichert; fehlgeschlagene oder nur
  teilweise veröffentlichte Läufe überschreiben ihn nicht.
- **MTP-13** [aktiv] [gtk] — Die gesamte Gerätekarte ist genau ein nativer
  Tastatur- und Pointer-Einstieg in eine nicht-modale Geräte-Vollseite im
  Hauptfenster und startet keinen Sync direkt. Der primäre Menüeintrag öffnet
  bei einem Gerät dieselbe Seite und bei mehreren Geräten zuerst eine kompakte
  Auswahl. Die Seite enthält keine Song- oder Geräte-Dateiliste und als einzige
  Einstellung das Transferprofil; sie zeigt jede Playlist mit sichtbarem,
  markup-sicherem Namen, Auswahl, letztem verifiziertem Sync und der für das
  aktive Profil projizierten Zielgröße sowie bei einem laufenden Sync
  Fortschrittsbalken und aktuelle geglättete MTP-Transferrate.
- **MTP-14** [aktiv] [gtk] — Die Geräte-Vollseite besitzt die
  Informationshierarchie eines Geräte-Dashboards und nicht die einer
  Preferences-Seite: Geräteidentität, MTP-Status, letzter Geräte-Sync,
  Gerätespeicher und Aktionen bilden einen gemeinsamen, einfachen Hero-Kopf.
  Playlists mit profilabhängiger Zielgröße und letztem Playlist-Sync bilden
  den Hauptinhalt; Transferprofil, Delta und laufender Fortschritt bleiben
  eine kompakte Nebenübersicht. Lokal bekannte Playlists erscheinen und bleiben
  auswählbar, während Reprise den MTP-Speicher noch prüft; nur der
  Sync-Start wartet auf diese Prüfung.
- **MTP-15** [aktiv] [gtk] — Playlist-Arbeitsbereich und Sync-Übersicht
  besitzen unabhängig von Delta-, Track- und Geschwindigkeitstext dieselben
  stabilen oberen und unteren Kartenkanten; wechselnder Statustext wird
  innerhalb einer begrenzten Overview-Breite umgebrochen und verschiebt keine
  Spalte. Die aktuelle MTP-Transfergeschwindigkeit steht während Copy als
  eigene beschriftete Zeile neben dem Tracktext. Die Sidebar-Gerätekarte nennt
  den freien Gerätespeicher auch während Checking, Sync und Finishing so früh,
  dass Ellipsize ihn nicht verdeckt.
- **MTP-16** [aktiv] [gtk] — Eine Änderung des Transferprofils wird sofort
  gerätebezogen gespeichert und für dasselbe Gerät sowohl nach einem Reconnect
  als auch nach einem App-Neustart wiederhergestellt. Ein neues Gerät beginnt
  weiterhin mit Opus 160 kbit/s.
- **MTP-17** [aktiv] [core] — `Music/Reprise` ist der einzige und vollständig
  autoritative Gerätebereich von Reprise. Nach erfolgreicher Veröffentlichung
  aller gewünschten Tracks und Playlists werden dort sämtliche übrigen
  sicheren Dateien entfernt, auch wenn sie nicht im Reprise-Inventar stehen;
  gewünschte Track- und Playlist-Pfade bleiben erhalten. Außerhalb dieses
  Unterordners wird nichts geschrieben, verschoben oder gelöscht, und ein
  fehlender oder ungültiger Playlist-Sollzustand plant keine destruktive
  Arbeit.

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
  more than 30 days ago), including their ratings, playlist entries, and
  device sync state. Listening history stays in My Stats. Remove now / Start
  counting from today." Letzteres speichert das
  Aktivierungsdatum als Stichtag (`auto_clean_armed_at`); gelöscht wird nur,
  was Frist UND Stichtag reißt. Beide Lösch-Dialoge der App (dieser und
  „Remove all N") benennen die Kaskade explizit: Ratings, Playlist-Einträge
  und Geräte-Sync-Zustand gehen; Hörereignisse bleiben (BROWSE-6).
- **SET-5** [aktiv] [gtk] — Der Inhalt jeder Preferences-Hauptseite beginnt
  mit dem kompakten Standardabstand direkt unter dem Inhalts-Header. Kurze
  Seiten werden nicht vertikal zentriert; ungenutzter Raum bleibt unter der
  letzten Gruppe.
- **SET-6a** [aktiv] [gtk] — Die Plugins-Seite gruppiert nach Nutzerabsicht:
  „Local Features", „Online Content" und „Connected Services". Scrobbling
  erscheint dort genau einmal als Navigationseintrag und öffnet eine
  Navigation-Page im selben Preferences-Fenster mit ‹-Back. Es gibt keinen
  globalen Scrobbling-Schalter.
- **SET-6b** [aktiv] [gtk] — Die Scrobbling-Unterseite führt ListenBrainz und
  Last.fm als unabhängige Anbieter; beide dürfen gleichzeitig aktiv sein.
  Aktivierung, Konto, Status, Fehler und Warteschlange bleiben
  anbieterspezifisch. Mit gebündelten App-Zugangsdaten bietet Last.fm den
  normalen Browser-Login direkt an; eigene API-Zugangsdaten liegen
  eingeklappt unter „Advanced setup".
- **SET-7** [aktiv] [gtk] — „New Releases" und „Concerts" sind gleichrangige
  Preferences-Hauptseiten in der vertikalen Navigation. Für diese beiden
  Features behält die Plugins-Seite nur die Aktivierungsschalter; Scope-,
  Provider-, Location- und Similar-Optionen stehen ausschließlich auf den
  jeweiligen Hauptseiten und sind bei deaktiviertem Modul nicht bedienbar.

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
- **FB-2a** [ersetzt durch FB-8] — Der Relink-Suchlauf läuft off-thread in der
  bestehenden, mit Scan/Sync stapelbaren Fortschrittskarte **innerhalb** des
  unten fixierten Issues-Bereichs. Seine Reihenfolge ist: Überschrift
  „ISSUES“ → laufende Karten → Import errors / Missing files; laufende Karten
  und Issue-Zeilen bilden dabei ohne flexiblen Zwischenraum einen gemeinsamen
  Block am unteren Rand. Karte: Spinner + Titel + % rechts (tabular) +
  3-px-Balken + ellipsierte Detailzeile. Klick auf die Karte → Missing files;
  der sichtbare Cancel-Button prüft den Abbruch vor jeder Audiodatei.
- **FB-2b** [geplant] [gtk] — Scan, Sync und Playlist-Import verwenden für
  jeden Lauf > ~1 s denselben vollständigen Kartenvertrag aus FB-8,
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
  (Kaskade: Playlist-Einträge, Ratings, Sync-Zustand); der eigenständige
  Hörverlauf bleibt gemäß BROWSE-6 erhalten. App-Ende im
  Fenster → Löschung wird beim nächsten Start committed, nie zurückgerollt
  („7 removed" muss wahr bleiben). Auto-clean (opt-in, default off, nur
  deleted-Tracks) löscht hart ohne Toast und ohne Undo — es feuert
  frühestens 30/90 Tage nach dem Verschwinden (SET-4).
- **FB-8** [aktiv] [gtk] — Scanner- und Relink-Suchläufe laufen off-thread in
  den bestehenden, mit Sync/Doctor stapelbaren Fortschrittskarten des unten
  fixierten Bereichs. Solange mindestens eine Fortschrittskarte sichtbar ist,
  **ersetzt** der Kartenblock den vollständigen Issues-Block; Überschrift
  „ISSUES“ und Import errors / Missing files sind weder sichtbar noch belegen
  sie zusätzlichen Platz. Vollständig inaktive Fortschrittskarten belegen
  ebenfalls keinen Platz; nur aktive oder noch ausblendende Karten nehmen am
  Layout teil. Die Unterkante des sichtbaren Kartenblocks liegt direkt über
  der Playerleiste, während sämtliche freie Sidebar-Höhe oberhalb des Blocks
  bleibt. Nach dem vollständigen Ausblenden der letzten Karte kehrt der
  Issues-Block zurück. Persistenter Device-Status bleibt davon unabhängig
  sichtbar.
  Karte: Spinner + Titel + % rechts (tabular) + 3-px-Balken + ellipsierte
  Detailzeile. Klick auf die Karte → Missing files; der sichtbare
  Cancel-Button prüft den Abbruch vor jeder Audiodatei.

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
- **FIL-1c** [aktiv] [gtk] — Artist-, Album- und Genre-Scopes der Track-Liste tragen
  in der Filter-Zeile eine eigene Scope-Chip-Klasse neben Such- und
  Facetten-Chips: „<Interpret>", „<Album> — <Interpret>" beziehungsweise
  „<Genre>" mit eigenem ×-Klickziel von mindestens 20 px. Das × verlässt den Scope per
  regulärem NAV-2-History-Push zur Library; dort werden deren gemerkte Suche
  und Facetten wiederhergestellt. Die Zählung folgt FIL-2 und setzt die
  Scope-Treffer ins Verhältnis zur ganzen Library. Playlist, Smart, Queue,
  Missing und eigenständige Panels tragen keinen Scope-Chip. „Clear all"
  räumt weiterhin nur Suche und Filter und wechselt nie den Ort.
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
- **FIL-7** [aktiv] [gtk] — „Hide AI music" ist ein **opt-in**-Filter:
  Default sichtbar (KI-Fassungen sind gewollte Bibliotheksbürger, INST-Sektion),
  auf Wunsch blendet er KI-manipulierte (und künftig -generierte) Titel aus. Er
  schlüsselt auf das **Provenance-Flag in der DB** (`track_provenance.ai`), nie
  auf Ordnerpfade — der Ordner ist Ablage-Layout, das Flag die Wahrheit. Aktiv
  fügt er sich als sichtbare Einschränkung in die Filter-Zeile nach **FIL-1a**
  (eigener Chip mit ×-Klickziel) und in die Zählung nach **FIL-2** ein
  („15 of 1,664 tracks", Force-show); er ist wie die Facetten-Chips
  **Library-only** und wird als Query-Klausel im Core umgesetzt
  (`queries::query_track_window_browsed_ai`). Der Filterzustand ist **sticky
  über Sessions** wie andere View-Zustände. **Keine Shuffle-/Auto-Queue-
  Sonderregel** in v1: das Queue-Nachfüllen folgt der sichtbaren Ansicht — bei
  aktivem Filter sind KI-Titel nicht sichtbar und werden nicht nachgefüllt. Nur
  verfügbar, solange der Experimental-Schalter an ist (INST-11). (Beschluss 17)
- **FIL-8** [aktiv] [core] [gtk] — „Recently added" ist ein eigener
  Library-Scope über alle gegenwärtig vorhandenen Tracks, deren `added_at`
  höchstens sieben Tage zurückliegt; es gibt kein 50-Track-Limit. Die Quelle
  sortiert initial nach `added_at` absteigend und trägt in der Filter-Zeile
  eine löschbare Scope-Pille nach FIL-1c. Deren × verlässt den Scope über den
  normalen History-Push und stellt die gemerkte, uneingeschränkte Library
  wieder her.
- **FIL-9** [aktiv] [gtk] — Wird eine Suche oder ein Facettenfilter gesetzt,
  geändert oder entfernt und der geladene Track gehört zur neuen
  Ergebnismenge, wird seine markierte Zeile vertikal zentriert statt an der
  oberen Tabellenkante verankert. Selektion und Tastaturfokus bleiben
  unverändert. Ohne geladenen oder im Ziel sichtbaren Track bleibt der
  bisherige ID-plus-Offset-Anker erhalten.

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
  Sortierung, Rating). Beim asynchronen Tag-Editor-Save wird der Scrollanker
  vor dem Öffnen des Dialogs erfasst und nach dessen Worker-Abschluss
  wiederverwendet. Ein reiner Rating-Save, der weder Sortierung noch Filter
  oder Quellenmitgliedschaft beeinflusst, aktualisiert Cache und realisierte
  Sternzellen ohne Model-Signal und damit ohne Scrollbewegung. Gelöschte IDs
  fallen still heraus; ein gewollter Reset ist explizit, nie Nebeneffekt.
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

  <!-- REVIEW: Regelvorschlag -->
- **TAG-9** [geplant] [manuell] — Das Autocomplete-Popover verwendet
  durchgehend die erhöhte, vom Theme gelieferte Popover-Fläche. Innere
  Listen malen keine eigene dunkle View-Fläche darüber; Auswahl und
  Akzent-Hervorhebung bleiben auf hellen und dunklen Themes lesbar.

## M. Tooltips

<!-- Die Sektionsbuchstaben K (Filter- & Such-Sichtbarkeit) und L (Tag-Editor)
     sind bereits vergeben; Tooltips sind daher Sektion M. -->

Tooltips sind Beschriftung, kein Feedback-Mechanismus — sie tragen nie die
einzige Aussage (TIP-3) und fallen daher nicht unter P-1s Rollenmodell.
Wird ein ganzer Container deaktiviert, gilt TIP-2a/b für die
Container-Aussage, nicht für jedes Kind einzeln (die leere Player-Leiste
ist ihre eigene Aussage).

- **TIP-1a** [ersetzt durch TIP-1c] — Existenz folgt der Beschriftung:
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
- **TIP-1c** [ersetzt durch TIP-1d] — Existenz folgt Beschriftung und Aktion:
  Icon-only-Buttons haben immer einen Tooltip; Buttons mit sichtbarem
  Aktionslabel bekommen keinen. Ein kompaktes Metadaten-Label darf die
  verborgene Aktion benennen (Player-Bar-Interpret: „Go to artist").
  Ellipsierte Labels zeigen weiterhin nur bei tatsächlicher Kürzung den
  vollen Text.
- **TIP-1d** [aktiv] [gtk] — Existenz folgt Beschriftung und Aktion:
  Icon-only-Buttons haben immer einen Tooltip; Buttons mit sichtbarem
  Aktionslabel bekommen keinen. Ein kompaktes Metadaten-Label darf die
  verborgene Aktion samt passendem Shortcut benennen (Player-Bar-Interpret:
  „Jump to now playing (Ctrl+L)"). Ellipsierte Labels zeigen weiterhin nur
  bei tatsächlicher Kürzung den vollen Text.
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
- **TIP-6** [aktiv] [gtk] — Shortcut-Hinweise bleiben aktionsgleich:
  besitzt die im Tooltip benannte Control-Aktion bereits einen dokumentierten
  Tastatur-Shortcut, steht er in Klammern hinter der Beschriftung. Shortcuts
  anderer Aktionen werden nicht an benachbarte Controls angehängt; Controls
  ohne passenden Shortcut bleiben unverändert.

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
  StatusPage⇄Liste-Stacks crossfaden mit dem Standard-Token wie der äußere
  Library/Stats/Device-Stack.
- **MOT-4** [ersetzt durch MOT-8] — Listen bewegen sich nicht: kein
  Stagger/Fade-in pro Row (windowed Model, 200er-Fenster, Bibliotheken
  jenseits 1 600 Rows). Erlaubt: ein Crossfade der gesamten Fläche beim
  View-Wechsel, solange nicht zwei dichte Quellen gleichzeitig lesbar
  werden; Podcasts⇄Music schaltet deshalb hart. Benannte Ausnahme: die Queue
  darf DnD-Drop und Einzel-Remove animieren.
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
- **MOT-8** [aktiv] [gtk] — Listen bewegen sich nicht: kein Stagger/Fade-in
  pro Row (windowed Model, 200er-Fenster, Bibliotheken jenseits 1 600 Rows).
  View-Wechsel behalten das Standard-Token. Zwischen zwei dichten Quellen
  (Podcasts⇄Music) wird die ausgehende Fläche vor dem Stack-Wechsel
  vollständig ausgeblendet und nur die eingehende Fläche über die
  Standarddauer eingeblendet: sichtbare Bewegung ohne harten Schnitt und
  ohne zwei gleichzeitig lesbare Tabellen. Die Queue-Ausnahme aus MOT-4
  bleibt erlaubend; `gtk-enable-animations=false` schaltet nach MOT-7 hart.

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
- **NPP-10** [ersetzt durch NPP-13] — Trackwechsel ist kein Ortswechsel: Cover,
  Titelblock, Glow und Tab-Inhalt crossfaden **gemeinsam** in einem
  Übergang (Standard-Token, MOT-5), niemals als Slide; die Lyrics starten
  danach bei Zeile 0 und positionieren sie gemäß LYR-4.
  `gtk-enable-animations=false` schaltet auch hier hart (MOT-7).

## Q. Suche

- **SEARCH-1** [aktiv] [gtk] — Im Ruhezustand belegt die Suche in der
  Headerbar nur eine Lupe. Das Suchfeld lebt in einer zweiten, standardmäßig
  eingeklappten Top-Bar und wird nie als permanentes breites Feld dargestellt.
- **SEARCH-2** [ersetzt durch SEARCH-2a] — Ein Klick auf die Lupe, Ctrl+F oder direktes
  Tippen öffnet die Suchleiste und fokussiert das Feld. Sie ist ein
  vollbreiter Streifen bündig unter der Headerbar, hat eine eigene Fläche mit
  unterer Trennlinie und schiebt beim Reveal den Inhalt nach unten; das
  Suchfeld ist darin per Clamp auf ungefähr 450 px zentriert. Die Leiste
  slidet mit der zentralen Standarddauer (MOT-1/3); bei GTK-eigenen Revealern
  gilt deren Default, sofern er dem Standard-Token entspricht.
- **SEARCH-2a** [ersetzt durch SEARCH-2b] — Ein Klick auf die Lupe, Ctrl+F oder direktes
  Tippen öffnet die Suchleiste und fokussiert das Feld. Header und Suche sind
  eine zusammenhängende obere Glaszone mit gemeinsamem neutralem Blur, Tint
  und genau einer unteren Hairline; Content läuft unter beiden weiter. Der
  Reveal vergrößert das obere Scroll-Inset um die tatsächlich allokierte
  Suchleistenhöhe, bei einer oberen Player-Leiste zusätzlich um deren Höhe.
  Das Suchfeld ist per Clamp auf ungefähr 450 px zentriert. Die Leiste slidet
  mit der zentralen Standarddauer (MOT-1/3); bei GTK-eigenen Revealern gilt
  deren Default, sofern er dem Standard-Token entspricht.
- **SEARCH-2b** [aktiv] [gtk] — Ein Klick auf die Lupe, Ctrl+F oder direktes
  Tippen öffnet die Suchleiste und fokussiert das Feld. Sie ist ein
  vollbreiter, opaker Streifen bündig unter der Headerbar mit eigener Fläche
  und unterer Trennlinie; beim Reveal reserviert sie strukturell ihre eigene
  Höhe. Das Suchfeld ist per Clamp auf ungefähr 450 px zentriert. Die Leiste
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

- **NR-1** [ersetzt durch NR-1a] [core] — Eine bibliotheksweite
  MusicBrainz-Pipeline ist die einzige Wahrheit für neue Releases und
  spätere Artist-News-Ansichten. Artist-MBIDs kommen zuerst aus Tags, sonst
  aus einer persistierten Namensauflösung inklusive negativer Ergebnisse;
  Artists werden nach Play-Count priorisiert. Pro Artist bleiben höchstens
  fünf reguläre Alben oder EPs der letzten 90 Tage sowie ausschließlich
  zukünftige Singles; unvollständige Daten gelten nie als zukünftig,
  Sekundärtypen bleiben draußen.
- **NR-1a** [aktiv] [core] — Eine bibliotheksweite MusicBrainz-Pipeline ist
  die einzige Wahrheit für neue Releases und spätere Artist-News-Ansichten.
  Artist-MBIDs kommen zuerst aus Tags, sonst aus einer persistierten
  Namensauflösung inklusive negativer Ergebnisse; Artists werden nach
  Play-Count priorisiert. Pro Artist bleiben höchstens zwanzig reguläre
  Alben oder EPs der letzten 90 Tage sowie ausschließlich zukünftige
  Singles; unvollständige Daten gelten nie als zukünftig, Sekundärtypen
  bleiben draußen.
- **NR-2** [aktiv] [gtk] — Release-Cover laden lazy über Cover Art Archive
  (`/release-group/{mbid}/front-250`). Ein fehlendes Cover ist Normalzustand
  und zeigt sofort eine gleich große Kachel aus gespeicherter Artist-
  Akzentfarbe plus Initialen — niemals ein Loch oder einen Dauer-Spinner.
- **NR-3** [ersetzt durch NR-3a] [gtk] — Die Header-Lupe ✦ erscheint nur bei vorhandenen
  Einträgen und trägt einen Badge ausschließlich für `seen_at IS NULL`.
  Öffnen stempelt die gelistete Episode als gesehen; sie badgt nie erneut,
  erst ein später neu gefundener Eintrag erzeugt wieder einen Badge (FB-4).
- **NR-4** [ersetzt durch NR-12] [gtk] — „See all" öffnet einen echten
  Digest-Ort mit Back/Forward-Historie, aber ohne Sidebar-Eintrag. Releases
  lassen sich dort verbergen; vorhandene Hidden-Einträge halten „See all"
  erreichbar und die Fußzeile „N hidden · Show" macht sie rückholbar. Ein
  künftiges „Remind me" bleibt bis zu einem eigenen Scheduler ausdrücklich
  außerhalb dieser Regel.
- **NR-5** [ersetzt durch NR-5a] [gtk] — Das Popover ist transient und
  verändert den Navigations-Stack nie. Erst „See all" navigiert regulär in
  den Digest-Ort; Schließen kehrt ohne Zustandsverlust zur aktuellen Ansicht
  zurück.
- **NR-5a** [ersetzt durch NR-5b] [gtk] — Das Popover ist transient; Öffnen/Schließen
  verändert den Navigations-Stack nie. Nur explizite Zeilen-Aktionen (Show in
  library) navigieren regulär und schließen das Popover; der Verlauf ist eine
  Popover-interne Unterseite ohne Navigation.
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
- **NR-9** [ersetzt durch NR-9a] [gtk] — setzt auf NR-3 auf: Der Badge
  aus NR-3 zeigt die **Anzahl** der Einträge mit `seen_at IS NULL`, ab 10 als
  „9+", verschwindet mit dem Öffnen (alle gelisteten Einträge werden
  gestempelt) und rendert bei 0 kein leeres Element.
- **NR-10** [aktiv] [gtk] — Zeilen-Hover bzw. -Fokus blendet den
  Status-Chip aus und die Zeilen-Aktionen ein; beim Verlassen kehrt der Chip
  zurück. Tastaturparität: die Zeile ist fokussierbar, Fokus zeigt die
  Aktionen, die Buttons sind per Tab/Enter erreichbar.
- **NR-11** [aktiv] [gtk] — „Open announcement" öffnet eine URL nach
  Priorität: MusicBrainz-URL-Relations der Release-Group (Bandcamp/Kauf/
  Streaming vor offizieller Homepage/Discography) → Fallback
  MusicBrainz-Release-Group-Seite. Geöffnet wird extern (Standardbrowser).
- **NR-12** [ersetzt durch NR-12a] [gtk] — Der Verlauf ist eine persistente Historie
  aller je gezeigten Meldungen als **Popover-Unterseite** (kein eigener
  Navigations-Ort), gruppiert nach Zeitraum, ausgeblendete Einträge einzeln
  rückholbar. Retention: 6 Monate **und** höchstens 200 Einträge (strengere
  Grenze gewinnt), hartes Löschen, aber nie innerhalb des
  90-Tage-Fetch-Fensters. Ersetzt NR-4.
- **NR-13** [aktiv] [gtk] — Bereits in der Bibliothek vorhandene,
  erschienene Releases werden markiert (nicht herausgefiltert) und bieten
  die Aktion „Show in library" (Navigieren + Fokussieren, **kein** direkter
  Play-Pfad).
- **NR-3a** [aktiv] [gtk] — Der Header-Auslöser öffnet „Updates" und ist
  sichtbar, sobald mindestens ein aktiver Feed Einträge oder einen
  Erstlauf-Zustand nach NR-8 hat. Sein Badge zählt ausschließlich ungesehene
  Einträge aller aktiven, fetch-bereiten Feeds.
- **NR-5b** [aktiv] [gtk] — Das Popover ist transient; Öffnen/Schließen
  verändert den Navigations-Stack nie. Explizite Zeilen-Aktionen und die
  Sprungzeilen „Show all releases/concerts →" navigieren regulär und
  schließen das Popover. Das Popover hat keine internen Unterseiten; der
  Verlauf lebt in der Releases-Vollansicht (NR-12a).
- **NR-9a** [aktiv] [gtk] — Das Badge zeigt die Summe ungesehener Releases
  und Konzerte, ab 10 als „9+", und rendert bei 0 nichts. Öffnen stempelt die
  gesamte Delta-Menge beider Sektionen im aktuellen Scope. Vollständig in der
  Bibliothek vorhandene Releases werden gelistet und gestempelt, zählen aber
  nie in den Unseen-Badge.
- **NR-12a** [ersetzt durch NR-16] [gtk] — Die persistente Historie aller je gezeigten
  Meldungen lebt in der Releases-Vollansicht als eigenem Sidebar-Ort.
  Ausgeblendete Einträge sind dort über den Hidden-Filter einzeln mit „Show
  again" rückholbar. Retention bleibt: sechs Monate UND höchstens 200
  Einträge, hartes Löschen, nie innerhalb des 90-Tage-Fetch-Fensters.
- **NR-14** [ersetzt durch NR-17] [gtk] — Die Releases-Vollansicht ist eine Tabelle
  `Date · Title · Artist · Type · Status`, standardmäßig nach Datum
  absteigend. Status ist `In library`, sonst `upcoming` oder `released`.
  Aktivierung führt immer die Dreiweg-Primäraktion aus: Hidden → Show again;
  vollständig vorhanden und erschienen → Show in library; sonst Open
  announcement. Die permanente Filterzeile bietet sticky Chips für Not in
  library, Type und Hidden samt „X of Y releases", „Clear all" und genau
  einem „Show all"-Schritt bei null Treffern.
- **NR-15** [ersetzt durch NR-18] [gtk] — „Releases" ist ein Sidebar-Ort in SMART, vor
  Concerts und nur bei aktivem `new_releases`-Modul. Sein Badge entspricht
  exakt der Anzahl der nach persistenten Filtern beim Öffnen sichtbaren
  Zeilen; 0 rendert keinen Badge.
- **NR-16** [aktiv] [core] [gtk] — Die Releases-Vollansicht ist ein
  Discography-Gap-Katalog für aktuell in der Bibliothek vertretene Artists.
  Sie enthält reguläre Alben und EPs unabhängig vom Alter, aber niemals
  Singles oder vollständig vorhandene Releases. Einzelne Vorab-Singles oder
  unvollständige Albumtitel zählen nicht als Besitz; vollständig ist ein
  erschienenes Release erst, wenn seine distinct lokalen Track-Identitäten
  mindestens die kleinste offizielle MusicBrainz-Edition abdecken.
  Ausgeblendete Lücken bleiben über den Hidden-Filter rückholbar; Album- und
  EP-Katalogzeilen unterliegen keiner zeitlichen Retention.
- **NR-17** [aktiv] [gtk] — Die Gap-Ansicht bleibt die Tabelle
  `Date · Title · Artist · Type · Status`, standardmäßig nach Datum
  absteigend. Status ist `upcoming`, `Missing`, `Incomplete` oder — bei
  bekannter Länge — `X of Y tracks`. Die permanente Filterzeile bietet nur
  noch sticky Type- und Hidden-Chips; Aktivierung öffnet die externe
  Release-URL, Hidden aktiviert `Show again`. Ein leerer Standardfilter
  bestätigt „No missing albums or EPs"; die Fußzeile enthält keine
  Sechs-Monats-Retention.
- **NR-18** [aktiv] [core] [gtk] — „Releases" bleibt ein nur bei aktivem
  `new_releases`-Modul sichtbarer Sidebar-Ort in SMART vor Concerts. Sein
  Badge entspricht exakt der Anzahl der mit den persistenten Type-/Hidden-
  Filtern sichtbaren Discography-Lücken; 0 rendert keinen Badge.
- **NR-19** [geplant] [gtk] — Eine Releases-Lücke darf zusätzlich einen
  klar als Affiliate-Link gekennzeichneten Kaufpfad anbieten, aber nur für
  einen vertraglich für installierbare Linux-Desktop-Apps freigegebenen
  Partner. Die Offenlegung steht direkt am Kauflink; ohne Freigabe oder
  echte Kaufrelation bleibt die unveränderte externe MusicBrainz-Relation
  provisionsfrei. Bibliotheksdaten und geheime Schlüssel gelangen nie in
  die URL. <!-- REVIEW: Regelvorschlag -->
- **NR-20** [aktiv] [core] [gtk] — Die Releases-Tabelle erweitert NR-17 um
  die Spalte `Buy`. Nur wenn MusicBrainz für die Release-Group eine echte
  HTTP(S)-Relation auf eine `/album/…`-Seite bei `bandcamp.com` oder einer
  Subdomain liefert, zeigt die Zeile dort `Bandcamp` und öffnet exakt diese
  URL im Standardbrowser. Lookalike-Domains, Artist-Homepages, geratene
  Such-URLs und alle anderen Ziele erzeugen keinen Kaufknopf. Der Direktlink
  ist provisionsfrei, enthält keine
  Trackingparameter und wird nicht als Affiliate-Link bezeichnet; NR-19
  bleibt einer späteren vertraglich freigegebenen Monetarisierung
  vorbehalten.

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
## T. Accessibility & Tastatur

<!-- Sektionsbuchstabe: S ist die letzte auf main vergebene Sektion;
     T schließt lückenlos an. Die automatisierbaren Regeln
     sind durch isolierte GTK-/CUA-Läufe aktiviert; ACC-7 benötigt zusätzlich
     die reale Sichtprüfung. -->

- **ACC-1** [aktiv] [e2e] — Vollständige Eingabeparität: Jede Aktion, die
  per Maus oder Touch erreichbar ist, ist im selben Kontext auch allein mit
  der Tastatur ausführbar und endet im selben Action-/Callback-Pfad. Eine
  Geste auf `Label`, `Image`, `Box`, `DrawingArea` oder einer Drag-Fläche ohne
  gleichwertigen Tastaturweg ist ein Bug. Ein Kontextmenü oder globaler
  Shortcut zählt nur, wenn er am fokussierten Ziel verfügbar und über Help,
  Beschriftung oder zugänglichen Hilfetext auffindbar ist.
- **ACC-2** [aktiv] [gtk] — Semantik ist Teil der Bedienung: Jedes
  interaktive Element exponiert einen kurzen übersetzten Namen, die passende
  Rolle, seinen aktuellen Zustand (`selected`, `checked`, `expanded`,
  `disabled`, `busy`) und — wo nötig — Beziehungen, Shortcut und Hilfetext.
  Dekoration trägt `Presentation`. Native GTK/libadwaita-Controls sind der
  Standard; eine eigene Rolle ist ein Versprechen, die zugehörige native
  Tastatursemantik vollständig zu liefern.
- **ACC-3** [aktiv] [e2e] — Fokusordnung folgt der sichtbaren Bedeutung:
  Tab vorwärts und Shift+Tab rückwärts durchlaufen die Oberfläche logisch,
  ohne Sprünge in versteckte/inaktive Controls und ohne doppelte Stops für
  denselben Befehl. Sidebar, Liste und Grid sind je **ein** Tab-Stop; Pfeile
  bewegen darin den aktiven Eintrag. Reines Fokussieren/Selektieren löst
  keine Navigation, Wiedergabe oder andere Aktion aus — erst Aktivierung.
- **ACC-4** [ersetzt durch ACC-4a] — Standardtasten gelten überall konsistent;
  die globale Space-Ausnahme des linken Sidebar-Toggles ist jetzt in ACC-4a
  explizit.
- **ACC-4a** [aktiv] [e2e] — Standardtasten gelten überall konsistent:
  Pfeile navigieren räumlich bzw. zeilenweise, Home/End springen in langen
  Collections an Anfang/Ende, Page Up/Down bewegen seitenweise, Enter
  aktiviert den fokussierten Eintrag. Space bleibt in passiven Collections
  sowie auf einem bereits ausgewählten, passiven View-Tab global Play/Pause.
  Dasselbe gilt für den fokussierten linken Sidebar-Toggle; er klappt die
  Sidebar nur per Pointer oder Enter ein und aus. Andere fokussierte
  Buttons/Toggles mit echter lokaler Aktion behalten Space, Textfelder tippen
  ein Leerzeichen. Menü-Taste/Shift+F10 öffnet das Kontextmenü, F10 das
  Primärmenü und Esc schließt den obersten transienten Container. Ein globaler
  Shortcut darf nie Texteingabe oder die lokale Semantik eines fokussierten
  Controls stehlen.
- **ACC-5** [aktiv] [e2e] — Fokus hat einen nachvollziehbaren Lebenszyklus:
  Start und Navigation setzen ihn in die aktive Zielansicht; Ctrl+F setzt ihn
  ins Suchfeld, dessen Esc-Kaskade gibt ihn an die **aktuelle** Content-View
  zurück. Dialoge/Popover starten auf ihrem ersten sinnvollen Control, halten
  den Fokus innerhalb der obersten Ebene, Esc schließt genau diese Ebene und
  gibt den Fokus an den Auslöser zurück. Back/Forward restauriert den letzten
  sinnvollen Fokus der Zielansicht statt Header oder unsichtbare Kinder zu
  fokussieren.
- **ACC-6** [aktiv] [gtk] — Dynamische Updates stehlen oder verlieren den
  Fokus nie: Bleibt das logische Element erhalten, bleibt auch sein Fokus;
  wird es entfernt, fällt der Fokus auf den nächsten, sonst vorherigen
  bedienbaren Eintrag und zuletzt auf den stabilen Container. Filter,
  Re-Sortierung, View-Rebuild, Trackwechsel, Scan/Sync/Mount und asynchrone
  Karten-Updates setzen den Fokus niemals ungefragt auf ein anderes Ziel.
- **ACC-7** [geplant] [manuell] — Fokus ist immer sichtbar und eindeutig:
  jedes per Tastatur erreichbare Element zeigt im Normal- und
  High-Contrast-Theme einen dauerhaften Fokusindikator, der nicht mit Hover,
  Selektion oder „spielt gerade" verwechselt werden kann. `outline: none` ist
  nur mit einem mindestens gleich deutlichen `:focus-visible`-Ersatz erlaubt.
  Hover-eingeblendete Aktionen erscheinen ebenso bei Tastaturfokus oder sind
  über das Kontextmenü des fokussierten Containers erreichbar.
  <!-- REVIEW: Regelvorschlag -->
- **ACC-8** [aktiv] [e2e] — Direkte Manipulation hat eine Alternative:
  jedes Drag-and-drop/Reorder-Ziel bietet denselben zulässigen Move auch per
  Button, Menü oder dokumentierter Tastaturaktion; dieselben Guards und
  Persistenzpfade gelten. Eigene Werte-Controls (z. B. Waveform-Seek) sind
  fokussierbare Ranges: Pfeile ändern fein, Page Up/Down grob, Home/End setzen
  Minimum/Maximum; Name, aktueller Wert und Grenzen sind zugänglich.
- **ACC-9** [aktiv] [gtk] — Shortcuts und Zugriffstasten folgen GNOME:
  vorhandene Standardaktionen verwenden die Standardbelegung (u. a. Ctrl+F,
  Ctrl+W, Ctrl+Q, Ctrl+,, Ctrl+?, F1, F10, Alt+←/→); häufige beschriftete
  Aktionen und primäre Dialogaktionen erhalten kollisionsfreie Mnemonics,
  soweit Übersetzungen dies zulassen. Die Shortcuts-Ansicht listet nur
  tatsächlich verdrahtete Aktionen und bleibt mit ihnen im selben Commit
  synchron.

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
  Einträge. Maßgeblich ist der geladene Track, nicht der Abspielzustand: ein
  aus der Sitzung wiederhergestellter Track, der in der Playerleiste steht,
  zeigt seine Lyrics ohne vorherigen Start. Der Leerzustand „Play a track to
  see its lyrics" gilt nur, solange gar kein Track geladen ist.
- **LYR-3** [aktiv] [gtk] — Bei offenem Lyrics-Tab, fehlendem Text und
  ausgeschaltetem Modul zeigt eine zentrierte StatusPage Icon, Titel
  „Online lyrics are disabled", Untertitel „Enable them to load missing
  lyrics automatically" und „Enable in Settings" als Deep-Link zur kurz
  hervorgehobenen Plugins-Zeile. Solange LYR-1 geplant ist, verspricht dieser
  Zustand keine lokalen eingebetteten Songtexte. Ein eingeschaltetes Modul
  ohne Treffer zeigt stattdessen „No lyrics found".
- **DISCOVER-1** [ersetzt durch BROWSE-1] — Netz-Features ohne dauerhaft sichtbare
  eigene Fläche erhalten genau einen dezenten, schließbaren Inline-Hinweis am
  Ort der sichtbaren Lücke: Cover ab drei gleichzeitig sichtbaren Fallback-
  Kacheln, Portraits ab drei gleichzeitig sichtbaren Initialen-Avataren und
  New Releases am Kopf der Artists-Ansicht. Sichtbare Evidenz rastet den
  Hinweis ein; einmal gezeigt oder geschlossen kehrt er dauerhaft nicht
  zurück. Der Hinweis ist kein Badge und kein Toast.
- **DISCOVER-2** [ersetzt durch BROWSE-1] — Pro Ansicht ist höchstens eine
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
- **SEARCH-7** [aktiv] [gtk] — Verliert das Suchfeld samt seiner internen
  Controls den Tastaturfokus, klappt die offene Suchleiste nach Abschluss der
  laufenden Pointer-Aktivierung ein. Eine nicht leere Query bleibt gemäß
  SEARCH-3/5 als aktiver Filter samt Chip und Akzent-Lupe erhalten; ein Klick
  auf die Lupe darf die durch denselben Fokuswechsel geschlossene Leiste nicht
  versehentlich erneut öffnen.
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
- **STYLE-4** [ersetzt durch STYLE-1] — Chrome-Glas ist neutral und theme-abhängig,
  niemals vom Cover-Akzent eingefärbt. GL/NGL/Vulkan verwenden 24 px
  Backdrop-Blur über einem neutralen Tint-Floor von mindestens 80 %;
  Cairo, unbekannte Renderer, High Contrast und deaktivierte Animationen
  degradieren fail-closed zu einem neutralen, mindestens 94 % opaken Tint.
- **STYLE-5** [aktiv] [gtk] — **Verkleinern schneidet keine wesentliche
  Bedienung ab.** Bei horizontaler, vertikaler oder kombinierter
  Fensterverkleinerung bleiben die primären Controls und Statusinformationen
  innerhalb der Fensterfläche erreichbar. Insbesondere behält die strukturelle
  Playerleiste (PLAY-7b) ihre vollständige Höhe; Cover, Play/Pause,
  Positionszeit, Waveform, Dauer und Lautstärke liegen vollständig innerhalb
  ihrer Allokation. Lange Titel und Interpreten ellipsieren innerhalb der
  linken Metadatenzone und verschieben weder Transport noch Waveform aus der
  Fenstermitte. Scrollbarer Content gibt den Platz ab, nicht die Playerleiste.
- **STYLE-6** [aktiv] [gtk] — Bei starker horizontaler Verkleinerung klappt
  die Tracktabelle sekundäre sichtbare Spalten vorübergehend ein; Cover,
  Titel und Interpret bleiben sichtbar. Dieses Einklappen verändert weder
  gespeicherte Sichtbarkeit, Reihenfolge oder Breiten noch die Sortierung.
  „Show columns" stellt die Nutzerkonfiguration im schmalen Fenster wieder
  her; zusätzliche Breite wird dann ausschließlich innerhalb der Tabelle
  horizontal gescrollt.
- **STYLE-7** [aktiv] [gtk] — Wird das Library-Fenster auf eine Breite
  verkleinert oder gesnappt, bei der beide Flanken den Hauptinhalt sichtbar
  verdrängen, schließen linke Library-Sidebar und rechtes Now-Playing-Panel
  gemeinsam in derselben responsiven Transition. Ein 10-s-Undo-Toast stellt
  exakt den Zustand beider Flanken vor dem Verkleinern wieder her; auch das
  spätere Verbreitern restauriert diesen Zustand, sofern der User die Flanken
  im schmalen Fenster nicht selbst geändert hat. Responsive Änderungen
  überschreiben keine gespeicherte Sidebar- oder Panel-Präferenz, und beide
  Header-Toggles bleiben zum manuellen Wiederöffnen erreichbar.
- **CONTRAST-1** [aktiv] [gtk] — Es gibt drei zentrale Textstufen: Primär
  ungefähr 0,95 für Titel und Werte, Sekundär ungefähr 0,7 für Artist,
  Status, Metadaten und Spaltenköpfe, Hint ungefähr 0,5 für Platzhalter,
  Hinweise und deaktivierte Sekundärtexte. Passende Adwaita-Named-Colors
  haben Vorrang vor eigenen Alphas; pro Element wird nicht nachgetönt.
- **CONTRAST-2** [ersetzt durch CONTRAST-2a] — Jede „N tracks ·
  Dauer"-Statuszeile ist eine echte untere Leiste mit definierter Fläche und
  oberer Hairline. Sie reserviert eigenen Platz und überdeckt nie eine
  Trackzeile; erst gegen diese feste Fläche wird ihr Sekundärtext-Kontrast
  bestimmt.
- **CONTRAST-2a** [aktiv] [gtk] — Die „N tracks · Dauer"-Statuszeile ist ein
  kompakter Pill-Overlay mit definierter Fläche, Rundung und Hairline unten
  rechts in der Tracktabelle. Sie reserviert keine Vollbreiten-Zeile. Öffnet
  sich die rechte Info-Spalte, bleibt der Pill mit gleichem Abstand an deren
  linker Kante; erst gegen seine feste Fläche wird der Sekundärtext-Kontrast
  bestimmt.
- **CONTRAST-3** [aktiv] [gtk] — Statuszeilen, Spaltenköpfe,
  Sidebar-Sektionslabels und Kartenmetazeilen erreichen gegen ihre jeweilige
  Fläche mindestens 4,5:1. `.caption` plus Sekundärstufe gilt dabei als
  Kleinschrift und benötigt dieselbe Prüfung wie Hint bei Normalgröße.
- **CONTRAST-4** [ersetzt durch CONTRAST-1] — Jeder aktive Text und jedes aktive Icon im
  Glas erreicht mindestens 4,5:1 gegen den Worst Case seiner Zone: den
  Tint-Floor komponiert über dem hellsten beziehungsweise dunkelsten
  durchscheinenden Content. Artist, Zeit, Suchfeld und Header-Aktionen sind
  aktive Inhalte; nur deaktivierte oder rein dekorative Elemente dürfen
  darunter liegen.
- **NAV-10** [ersetzt durch NAV-10a] — Der laufende Kontext bleibt in allen
  Ansichten mit einer gemeinsamen Playback-Akzent-Markierung sichtbar; beim
  ersten Eintritt einer Ansicht wird er einmalig aufgedeckt, spätere Wechsel
  stellen NAV-5s gemerkten ID-plus-Offset-Anker wieder her. Explizites „Go to
  album/artist" springt immer deterministisch; Selektion folgt der Wiedergabe
  nie.
- **NAV-10a** [aktiv] [gtk] — **Markieren und Scrollen sind getrennt.** Jede
  sichtbare Instanz des geladenen Tracks trägt dieselbe Playback-Markierung.
  Doppelklick/Enter auf eine bereits sichtbare Row verändert den Viewport
  nicht. Play aus Stopped sowie explizites Previous/Next zentrieren den neuen
  Track ohne Fokus- oder Selektionsdiebstahl. Auto-Advance zentriert nur,
  wenn seit 1,5 Sekunden keine Scrollbewegung stattfand; explizite
  Metadaten-/Reveal-Navigation selektiert, fokussiert und zentriert immer.
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
- **NPP-12** [aktiv] [gtk] — Ohne gespeicherte Präferenz startet das rechte
  Now-Playing-Panel geschlossen. Sobald der Nutzer es über den Header-Toggle
  öffnet oder schließt, gewinnt dieser persistierte Zustand bei allen
  folgenden Starts (NPP-4); der neue Default überschreibt keine bestehende
  Präferenz.
- **NPP-13** [aktiv] [gtk] — Ein Trackwechsel baut die rechte Spalte nicht
  sichtbar neu auf: Tabs, Queue bzw. aktiver Tab, Footer und Panel-Fläche
  bleiben durchgehend stehen. Nur das Album-Cover wechselt mit dem
  Standard-Token; das alte Cover liegt dafür über dem vollständig
  aufgelösten neuen Cover oder Platzhalter und blendet erst dann aus. Die
  Queue aktualisiert ihre Zeilen unabhängig davon, sodass der abgespielte
  Titel nach oben aus der Liste rückt. Der vom Cover abgeleitete
  Playback-Akzent folgt weiterhin separat der Ambient-Transition aus MOT-1;
  Unterbrechungen folgen MOT-6. Neu geladene synchronisierte Lyrics starten
  bei Zeile 0 und positionieren sie gemäß LYR-4. Ohne Animationen wechseln
  Cover und Inhalt hart (MOT-7).

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
- **STATS-1** [ersetzt durch STATS-11/STATS-12] [core] — Der Kopf zeigt die Gesamt-Hörzeit groß in vollen
  Stunden („68 hours"; unter einer Stunde in Minuten, nie „0 hours"), eine
  Vergleichs-Pill „▲ N % vs <Vorperiode>" im teal App-Akzent (nie im
  Cover-Akzent) und die Subzeile „N plays · Ø X min/day · N artists" auf
  Sekundär-Ton. Bei ausreichender Breite steht rechts das Zeitraum-Dropdown
  („<Jahr> so far / <Vorjahr> / All time / Last 30 days"). Bevor Gesamtzeit
  oder Pill ellipsieren, brechen Dropdown und Customize-Menü unter den Hero
  um; bei noch engerer Breite steht die Pill unter dem Stundenanker. Darunter
  läuft ein schlankes Area-Ribbon der
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
- **STATS-1a** [ersetzt durch STATS-11a] [core] — Die Vergleichs-Pill bleibt bei jedem Verhältnis
  lesbar: Anstiege unter +1000 % erscheinen weiter als ganze Prozentzahl, ab
  +1000 % als gerundeter Faktor („▲ ×11 vs 2025"). Eine sinnvolle
  Nachkommastelle bleibt erhalten („×11,5"), eine bedeutungslose Null entfällt
  („×11", nie „×11,0"). Starke Rückgänge ab 50 % verwenden dieselbe Form mit
  Abwärtsmarker („▼ ×0,3"); ein nichtnulliger Faktor unter 0,1 bleibt als
  „▼ ×<0,1" ehrlich und rundet nie auf „×0". Lag die Vergleichszeit unter einer
  Minute, ist sie für die sichtbare Minutengranularität effektiv null; statt
  Prozent oder Faktor steht eine zeitraumgerechte qualitative Aussage wie
  „New this year".
  Die Pill nennt nur die kurze Referenz („vs 2025") und ellipsiert nie; der
  Tooltip trägt die vollständige Semantik („vs same period 2025"). `×` und
  Dezimaltrenner bleiben übersetzbar. Saisonale Spanne und Vergleichsrechnung
  aus STATS-1 ändern sich dadurch nicht.
- **STATS-2** [ersetzt durch STATS-13] [core] — Das Artist-Spotlight ist das Herzstück:
  #1-Artist mit großem Cover und Rang-Badge, Eyebrow „YOUR #1 ARTIST", Name,
  Zeile „N plays · N h · N % of your artist listening" — der Anteil bezieht
  sich auf die Hörzeit mit Artist-Zuordnung, dieselbe Grundgesamtheit, die die
  Rangliste bildet, nicht auf jeden Play —, drei Top-Track-Chips sowie die
  Aktionen Play (Container-Play über die Trackliste des Artists) und
  „Go to artist" (regulärer NAV-Push mit Back-Historie). Hinter dem Cover liegt
  ein dezenter Cover-Akzent-Glow — der Cover-Akzent bleibt Playback-Elementen
  vorbehalten. Darunter nennt eine Ghost-Zeile die Ränge 2–5.
- **STATS-3** [ersetzt durch STATS-15] [core] — Das Genre-Spektrum ist **eine** horizontale
  Segment-Leiste in Teal-Abstufungen mit Legende (Punkt · Name · %), gespeist
  aus den Genre-Tags der Bibliothek. Die fünf stärksten Genres bilden eigene
  Segmente, der Rest wird zu „Other" gebündelt; Tracks ohne Genre zählen weder
  als Segment noch als „Other". Die Leiste ist reine Anzeige und keine
  Navigation: Segmente und Legende sind nicht klickbar.
- **STATS-4** [ersetzt durch STATS-10] [core] — Unter dem Spektrum steht eine asymmetrische
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
- **STATS-5** [ersetzt durch STATS-14] [core] — Top Tracks steht über die volle Breite:
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
- **STATS-6b** [ersetzt durch STATS-6c] [gtk] — Importierte Hörhistorie erzeugte
  früher eine eigene Statusseite, obwohl ihre Zähler keinem Statistikzeitraum
  zugeordnet werden können.
- **STATS-6c** [aktiv] [gtk] — Die Zeitraumliste folgt ausschließlich der
  detaillierten Hörhistorie: Das laufende Jahr bleibt immer verfügbar, ältere
  Kalenderjahre erscheinen nur, wenn sie mindestens ein `listen_event`
  enthalten. Importierte `tracks.play_count`-Zähler erzeugen weder ein Jahr
  noch eine Sondermeldung. Ist ein verfügbarer Zeitraum leer, bleibt der
  reguläre Leerzustand sichtbar; Hero und Zeitraum-Dropdown bleiben darüber
  bedienbar, damit die Auswahl nie zur Sackgasse wird.
- **STATS-7** [ersetzt durch STATS-10] [gtk] — My Stats ist kuratiert, nicht frei editierbar:
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
- **STATS-10** [aktiv] [gtk] — My Stats erzählt in fester Reihenfolge von
  oben nach unten: Kopfzeile (Titel, optionales „New this year"-Badge,
  Zeitraumwahl) · Hero (Gesamtzahl, Subline, KPI-Reihe) · Wochen-Chart ·
  zweispaltige Reihe aus Band-Karte und Songs-Karte · optional die
  aufgeklappte Top-Track-Liste als eigene Sektion in voller Breite ·
  Genre-Karte. Mehr Sektionen gibt es nicht: keine Listening Clock, keine
  Highlight-Kacheln, kein Customize-Menü — die Seite ist kuratiert und nicht
  konfigurierbar.
  Im schmalen Fenster stapelt die zweispaltige Reihe, ohne die Reihenfolge zu
  ändern. Die Zeitraumwahl bleibt gemäß STATS-8 der einzige Ansichts-Regler.
- **STATS-11** [aktiv] [core] — Der Hero zeigt die Gesamt-Hörzeit riesig im
  seitenweit einheitlichen Kompaktformat: ab einer Stunde „N h M", bei vollen
  Stunden „N h", unter einer Stunde „N min". Darunter steht die
  Subline „N plays · N artists", rechts an der Grundlinie vier KPI-Paare:
  „Per day" (Ø Hörzeit/Tag) · Trend (absolutes Hörzeit-Delta zur
  Vergleichsspanne mit Richtungs-Icon in Akzentfarbe) · „Pace for <Jahr>" (lineare
  Jahres-Hochrechnung, nur im laufenden Jahr) · „Best week" (Startdatum und
  Hörzeit der stärksten lokalen Kalenderwoche). Alle KPI-Dauern nutzen
  dasselbe Kompaktformat. Die Vergleichsspanne ist
  unverändert die saisonal deckungsgleiche Vorperiode: „<Jahr> so far" gegen
  dieselbe Spanne des Vorjahrs, ein volles Jahr gegen das Vorjahr, das
  30-Tage-Fenster gegen die 30 Tage davor; „All time" hat keinen Trend-KPI.
  KPIs ohne Wert entfallen ersatzlos statt Platzhalter zu zeigen.
- **STATS-11a** [aktiv] [core] — Der Trend bleibt bei jedem Verhältnis
  ehrlich lesbar: Der KPI nennt das absolute Delta und die kurze Referenz
  („vs 2025"); der Tooltip trägt die vollständige Semantik samt Prozentwert,
  ab ×11-Verhältnissen als gerundeter Faktor nach den bisherigen Formregeln.
  War die Vergleichszeit effektiv null (unter einer Minute), erscheint statt
  des KPI das Badge „New this year" in der Kopfzeile — nie „∞ %" und nie
  „×0". Der KPI ellipsiert nicht.
- **STATS-12** [aktiv] [core] — Das Chart zeigt die Hörzeit je lokaler
  Kalenderwoche. Ab acht Wochen mit Plays gilt der Flächenverlauf über den
  exakt gewählten Zeitraum. Bei weniger Wochen beginnt die Achse mit der
  ersten Play-Woche und jede Woche erhält über die volle Kartenbreite einen
  gleich breiten Slot; Nullwochen bleiben als 2-Pixel-Strich auf einer
  durchgehenden 1-Pixel-Basislinie sichtbar. Unter zehn Wochen trägt jeder
  Slot ein Wochenlabel, längere Achsen tragen Monatslabels. Die kompakte
  Variante ist ungefähr 160 Pixel hoch. Beide Varianten lassen 10–15 % Luft
  über dem Maximum. Die beste Woche erhält statt einer Markerlinie eine
  hellere Akzentstufe; ihr gemessenes Label steht mit Randabstand darüber
  („best week · 4 h 12"). Die laufende Woche endet in einem offenen Punkt.
  Hover nennt Woche und exakten Wert. Markierungen und Punkte sind reine
  Anzeige. Nur wenn der Zeitraum zu kurz für Wochen ist, fällt die Achse auf
  Tage zurück (STATS-6); sehr lange „All time"-Spannen dürfen Monate zeigen
  und lassen dann die Wochenmarkierung weg — der Best-week-KPI bleibt.
- **STATS-13** [aktiv] [gtk] — Die Band-Karte zeigt den meistgehörten
  Interpreten als Bild-Hero: das Album-Cover seines meistgespielten Tracks
  füllt die Karte und blendet nach unten in den Kartengrund aus; fehlt ein
  Cover, steht eine Initialen-Kachel an seiner Stelle — nie eine leere
  Fläche. Darüber Kicker „MOST PLAYED BAND", Name und die Zeile „N plays ·
  <Dauer> · N % of your artist listening"; die Dauer folgt dem Kompaktformat
  aus STATS-11. Darunter die Ränge 2–5 mit dünnem Balken relativ zu Platz 1.
  Klick auf Karte oder Rangzeile öffnet die
  Library gefiltert auf den Interpreten (regulärer History-Push). Fasst eine
  Gruppe mehrere Schreibweisen zusammen, bleibt der Vereinheitlichungs-Hinweis
  aus STATS-9 erhalten.
- **STATS-14** [aktiv] [gtk] — Die Songs-Karte zeigt die sechs führenden
  Tracks: Cover, Titel und Interpret zweizeilig, horizontaler Balken relativ
  zu Platz 1 in einem Akzent-Verlauf, rechts die Play-Zahl. Neben dem Kicker
  sortiert der Toggle „by plays / by time" sowohl diese sechs Zeilen als auch
  die vollständige Rangliste. Klick auf die Zeile öffnet die Library gefiltert
  auf den Interpreten mit fokussiertem Track; Hover oder Fokus zeigt am Cover
  einen Play-Button, der genau diesen Track sofort abspielt; das Kontextmenü
  bietet „Play next", „Add to queue" und „Go to album". Der Ghost-Button
  „Show all top tracks" klappt unter der zweispaltigen Reihe die nummerierte
  Top-10-Liste als eigene Sektion in voller Breite auf; die Genre-Karte folgt
  darunter und der Balken bleibt relativ zum Spitzenreiter der jeweiligen
  Sortierung. Die Liste zeigt Dauern im Kompaktformat aus STATS-11; ihre Titel
  und Interpreten erhalten Linkfarbe und Unterstreichung erst bei Hover, der
  Fokus-Ring bleibt sichtbar.
- **STATS-15** [aktiv] [core] — Die Genre-Karte besteht aus einem
  gestapelten Balken (Segmentbreite = Anteil, Akzent-Abstufungen nach Rang,
  letztes Segment neutral, Tooltip „<Genre> · N % · <Dauer>") und bis zu vier
  Kacheln der stärksten Genres: Cover des meistgespielten Tracks im Genre,
  „<Genre> · N %", darunter „<Dauer> · top: <Interpret>". Beide Dauern folgen
  dem Kompaktformat aus STATS-11. Top-Interpret und Cover je Genre entstehen
  über dieselbe Schlüsselauflösung wie alle Gruppierungen (STATS-9). Klick auf
  das Kachel-Cover öffnet die Library
  gefiltert auf das Album; Klick auf ein Segment oder die übrige Kachelfläche
  öffnet die Library im Scope des jeweiligen Genres.
  Tracks ohne Genre zählen weiterhin weder als Segment noch als „Other".
- **STATS-16** [aktiv] [gtk] — Unter zehn Plays im gewählten Zeitraum ist
  die Datenlage zu dünn für einen Trend: Statt des Charts erscheint der
  Hinweis „Keep listening — stats grow with you"; Hero-Zahlen bleiben echt,
  und nur Karten mit Daten werden gerendert — nie Platzhalterkarten. Ohne
  jeden Play gilt unverändert der Leerzustand aus STATS-6/STATS-6c samt
  bedienbarer Zeitraumwahl.
- **STATS-17** [aktiv] [gtk] — My Stats steht ab dem ersten Frame vollständig
  da: Karten, Hero-Zahl, KPIs, Texte, Cover und Bilder faden nicht, gleiten
  nicht und zählen nicht hoch. Nur Balken bewegen sich, gemeinsam nach einem
  ruhigen Startframe von ungefähr 100 ms und mit Ease-out
  `cubic-bezier(0.16, 1, 0.3, 1)`: Im Sparse-Week-Modus wachsen die
  Chart-Balken in 500 ms von der Grundlinie mit 80 ms Versatz; das
  Best-Week-Label fadet erst nach dem Ende seines eigenen Balkens über 150 ms
  ein. Der alternative Flächen-/Linienmodus ist bereits im ersten Frame
  vollständig gezeichnet und besitzt keine Eingangsanimation. Horizontale
  Balken — Band-Ränge 2–5, Song-Balken und Genre-Segmente — wachsen innerhalb
  ihrer jeweiligen Karte in 450 ms von links mit 40 ms Versatz; Genre-Segmente
  laufen in Leserichtung. Auch Balken unterhalb des sichtbaren Ausschnitts
  folgen demselben Start, es gibt keine Fold-Sonderbehandlung. Ein
  Zeitraumwechsel startet keine Eingangschoreografie neu und interpoliert
  ausschließlich Balkenwerte über 250 ms; alle übrigen Inhalte wechseln
  sofort in ihren neuen Endzustand. Bei `gtk-enable-animations=false` stehen
  ausnahmslos alle Balken und das Best-Week-Label sofort im Endzustand.

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

## X. Song Visuals

<!-- Historie: Diese Sektion hieß „Lokales Klangprofil" und trug die Regeln der
     Song-Analyse (Audio Character), Create Similar Mix und Related Artist
     Discovery. Diese Features wurden entfernt (chore eda0edaebb); ihre Regeln
     AC-1..AC-6, AC-9 und AC-12..AC-18 sind hier gelöscht (git bewahrt die
     Historie). Es bleiben die noch aktiven Song-Visuals-Regeln. Das AC-Präfix
     bleibt als stabile Regel-ID der Visuals-Regeln erhalten. -->

- **AC-7** [ersetzt durch AC-10]
- **AC-8** [ersetzt durch AC-11]
- **AC-10** [ersetzt durch AC-19]
- **AC-11** [aktiv] [gtk] — Dauerbewegung existiert nur bei sichtbarem
  Visual-Tab und nur, solange der Player überhaupt einen Track hält. Laufende
  Wiedergabe zeigt die audioreaktiven Säulen. Pause und Stop lassen die
  Live-Balken ausklingen und übergeben an eine ruhende Atembewegung: eine
  flache, zu beiden Rändern auslaufende Welle von höchstens 10 % Balkenhöhe,
  die in sechs Sekunden einmal über die Breite wandert und im Ruhezustand mit
  rund 30 Hz statt der vollen Renderrate neu gezeichnet wird. Ohne geladenen
  Track bleibt die Fläche leer und ohne Tick-Callback;
  `gtk-enable-animations=false` zeigt die Ruhewelle als stehendes Bild. Das ist
  die in MOT-2 erlaubte, audiofunktionale Ausnahme für Dauerbewegung.
- **AC-19** [ersetzt durch AC-20]
- **AC-20** [ersetzt durch AC-21]
- **AC-21** [ersetzt durch AC-22]
- **AC-22** [aktiv] [core] [gtk] — „Song Visuals" ist ein standardmäßig
  ausgeschaltetes, live anwendbares Plugin. Eingeschaltet zweigt die
  Linux-Pipeline vor ReplayGain lokal normalisiertes Mono-PCM ab; CAVA-Mathematik
  erzeugt daraus 64 logarithmische, auf 0–1 begrenzte Anzeigebänder. Der
  portable Core verwendet CAVAs doppelte FFT-Auflösung unter 100 Hz,
  quantisierte Cutoff-Frequenzen und festen Frequenz-EQ sowie Noise-Floor-Gate,
  Auto-Sensitivität, Integral und Gravity. Digitale Stille erhöht die
  Sensitivität nicht; nicht-endliche Eingaben und Ausgaben werden neutralisiert,
  und alle internen Rückkopplungen bleiben begrenzt.
  Die Scene-Engine übernimmt jedes CAVA-Band im selben Frame ohne zweite
  Lautstärkeabbildung, Normalisierung oder Live-Hüllkurve. Sie zeichnet
  64 frequenzabhängige, fein segmentierte Neon-Säulen eins zu eins, mit dem
  bestehenden Cyan-zu-Magenta-Verlauf, Reflexionen, Glühen und langsam
  sinkenden Peak-Kappen. Unter Renderlast gilt strikt „latest wins"; alte
  Impulse werden nicht in neuere CAVA-Frames übertragen. Pause und Stop dürfen
  ausschließlich für das nach AC-11 geforderte Ausklingen eine visuelle
  Absenkung anwenden; die dort definierte Ruhewelle hebt danach nur an, was die
  Live-Balken frei lassen, und verändert weder CAVA-Werte noch Peak-Kappen.
  Unabhängig von den Balkenhöhen steuert die quadratische Energie der zwölf
  tiefsten CAVA-Bänder eine reine Darstellungsschicht: kräftiger Bass zündet
  sofort zwei breite Neon-Glows hinter den Säulen, die nach dem Impuls weich
  ausklingen. Ein extremer, breit anliegender Breakdown-Bass ergänzt
  nichtlinear zwei hellere innere Auren; ausschließlich hohe Frequenzenergie
  löst den Effekt nicht aus. Diese Bass-Aura verändert weder CAVA-Werte noch
  Peak-Kappen oder Balkenhöhen. Bei ausgeschalteten Animationen springt sie
  ohne Nachlauf auf den statischen Wert des aktuellen Frames.
  Track- und Album-ReplayGain normalisieren erst hinter dem PCM-Abzweig die
  hörbare Ausgabe; dieselbe Eingangswellenform erzeugt deshalb unabhängig vom
  gespeicherten Gain-Wert denselben visuellen Ausschlag. Eine Modusauswahl und
  „Grid" existieren nicht. Fullscreen begrenzt nur die interne
  Szenen-Rasterfläche und skaliert sie auf die unveränderte Canvas-Größe. Bei
  knapper Panelhöhe bleibt der Visual-Inhalt unter dem Tab-Switcher und scrollt
  innerhalb seines Tabs, statt den Switcher zu überlagern. Der beschriftete
  Canvas übernimmt den aktuellen Cover-Akzent über denselben globalen
  Ambient-Crossfade wie die Playerleiste; nur ohne brauchbare Coverfarbe gilt
  der Theme-Akzent.

## Y. Library Doctor / Tag Cleanup

Library Doctor trennt Erkennen, Entscheiden und Schreiben strikt: Ein Scan
liest und sammelt Vorschläge, die Review-Tabelle entscheidet feldgenau, und
nur ihr Apply startet einen journalisierten Schreibjob. „Safe" bedeutet
deterministisch und hoch-konfident, nie „ohne Review".

- **DOC-1a** [aktiv] [core] — **Local ist lesend und erfindet nichts.**
  Der Scan liest die tatsächlichen Tags der eingefrorenen Dateien; die DB
  liefert dafür nur Scope, Track-ID, Pfad und Dateiidentität. Er schreibt
  weder Tags noch bestehende Track-Metadaten und startet weder Scanner noch
  Reconcile. Als lokale Vorschläge sind ausschließlich erlaubt: mechanisches
  Unicode-Trim an Feldrändern; fehlender Album Artist aus dem nichtleeren
  Artist derselben Datei; sowie das Vereinheitlichen von Artist, Album, Album
  Artist und Genre über exakt `normalize_group_key` aus STATS-DEDUP. Ohne
  Remote-Evidenz gewinnt innerhalb des eingefrorenen Scopes die häufigste
  tatsächlich vorhandene exakte Schreibweise nach Edge-Trim. Gleichstand
  erzeugt nur eine manuelle Kandidatengruppe aus real vorhandenen Werten.
  Title erhält ausschließlich Edge-Trim. Interner Whitespace dient nur der
  Gruppierung; geschrieben wird immer ein realer Gewinner. Title Case,
  Genre-Aliaslisten, Fuzzy-Matching und erfundene Ersatzwerte sind verboten.

- **DOC-1b** [aktiv] [core] — **Remote bleibt eine getrennte Quelle.**
  Bei eingeschalteten MusicBrainz/AcoustID-Vorschlägen gilt die sparsame
  Kaskade: gültige eingebettete MBIDs zuerst, MusicBrainz nur für danach
  ungelöste Metadaten, AcoustID-Fingerprint nur für weiterhin ungelöste
  Tracks. Ein per MBID eindeutig kanonischer Name sticht die lokale
  Häufigkeit, bleibt aber ein Remote-Vorschlag mit Quelle und Konfidenz und
  wird nie als Local umetikettiert. Pro Track und Feld existiert höchstens ein
  konkurrierender Vorschlag. Remote darf ausschließlich Title, Artist, Album,
  Album Artist, Year und MusicBrainz Recording ID vorschlagen; in Version 1
  wird kein anderer MBID-Typ geschrieben. Year kommt bei eindeutiger Release
  aus deren Jahr, sonst bei eindeutiger Release Group ausdrücklich aus deren
  „original release"; mehrdeutige Editionen erzeugen keinen Jahresvorschlag.
  Remote-Genre, Track-/Discnummer, Rating, Pfad und Cover bleiben verboten.

- **DOC-1c** [aktiv] [core] — **Netzabrufe sind minimal, begrenzt und
  abbrechbar.** MusicBrainz erhält nur die zur Auflösung nötigen vorhandenen
  Title-/Artist-/Album-/Album-Artist-Werte, MBIDs und gegebenenfalls Dauer;
  AcoustID ausschließlich Fingerprint und Dauer. Pfad, Dateiname,
  Library-Root, interne Track-ID, Rating, Hörhistorie, Playlist- und
  Gerätezustand verlassen die App nie; dateinamenbasierte Platzhalter werden
  nie gesendet. AcoustID nutzt HTTPS und POST. Positive vollständige
  Cache-Einträge gelten 30 Tage, negative oder mehrdeutige sieben Tage und
  sind über MBID bzw. Chromaprint-Version + Fingerprint + Dauer stark
  identifiziert; ein Cache-Treffer behält seine Remote-Provenienz.
  Unvollständige Antworten werden nicht gecacht. MusicBrainz läuft gemeinsam
  begrenzt auf höchstens eine Anfrage pro Sekunde, AcoustID unter seinem
  öffentlichen Limit. `429` respektiert `Retry-After`, Timeout/5xx erhalten
  höchstens zwei Backoff-Wiederholungen, Auth-/Key-Fehler öffnen den Circuit
  für den Rest des Jobs. Cancel verhindert die nächste Anfrage und wirkt
  auch während Backoff; lokaler Scan und vollständige Einzelergebnisse bleiben
  dabei gültig.

- **DOC-1d** [ersetzt durch DOC-7a] [gtk] — **Lokale Aktivierung ist keine
  Netzfreigabe.** Library Doctor ist standardmäßig aus. Sein Hauptschalter
  aktiviert ausschließlich lokale Checks und zeigt keine Netzfrage. Der
  getrennte, standardmäßig ausgeschaltete Schalter „MusicBrainz/AcoustID
  suggestions" zeigt beim ersten Einschalten eine kurze, versionierte
  Bestätigung mit der Daten-Allowlist aus DOC-1c; Abbrechen lässt ihn aus.
  Plugin-Zeile und Ergebnisansicht binden denselben persistenten Schalter.
  Ausschalten stoppt künftige Remote-Anfragen, versteckt Remote-Zeilen und
  entfernt deren Auswahl; erneutes Einschalten zeigt vorhandene oder neu
  geladene Remote-Vorschläge ungeprüft. Fehlende Fingerprint-Capability wird
  sichtbar als „AcoustID unavailable" erklärt, während Local und reine
  MusicBrainz-Auflösung weiter funktionieren.

- **DOC-2a** [aktiv] [core] — **Scope und Scan-Ergebnis sind Snapshots.**
  Whole Library enthält ausschließlich aktuell `PRESENT` vorhandene lokale
  Tracks; Current View enthält alle Treffer der aktuellen Quelle, Suche,
  Filterung und Sortierung, nicht nur geladene Rows; Selection enthält exakt
  die ausgewählten vorhandenen Track-IDs und ist leer nicht startbar. Erst
  „Run scan now" friert die IDs ein. Spätere View- oder Auswahländerungen
  verändern den Lauf nicht. Ein ungültiger oder leer gewordener
  Aufrufkontext fällt sichtbar auf Whole Library zurück. „Tracks checked"
  zählt nur erfolgreich gelesene Dateien, übersprungene Dateien separat. Das
  letzte vollständig abgeschlossene Ergebnis überlebt Navigation und
  Neustart mit Scope, Zeitpunkt, Optionen und Provenienz; ein neuer oder
  abgebrochener Lauf ersetzt es erst nach vollständigem Abschluss. Günstige
  Stale-Prüfung markiert veränderte Zeilen beim Wiederöffnen, exakte
  Revalidierung folgt vor dem Schreiben. Neu hinzugekommene Tracks werden
  nicht nachträglich in den Snapshot aufgenommen.

- **DOC-2b** [aktiv] [gtk] — **26a ist Zusammenfassung, nie Schreibfläche.**
  Nach dem Scan zeigt Library Doctor getrennt „N safe · local, preselected"
  und „N suggestions · review" sowie Problemklassen für Casing/Whitespace,
  fehlenden Album Artist, Genre-Varianten, fehlendes/falsches Year und
  fehlende Recording MBID; jede Klasse zählt konkrete Track-Feld-Änderungen
  getrennt nach safe/review. Gleichstände erscheinen zusätzlich als
  „N unresolved groups", nicht als safe. „Review N changes" öffnet die
  vollständige Review-Tabelle; „Review N safe fixes" öffnet dieselbe Tabelle
  lokal gefiltert. Kein Control dieser Seite schreibt Tags. Bei
  ausgeschaltetem Remote-Schalter verschwinden Remote-Klassen, -Zeilen und
  -Counts vollständig, während das lokale Ergebnis bestehen bleibt.

- **DOC-2c** [aktiv] [gtk] — **Ein laufender Scan zeigt ehrliche
  Zwischenergebnisse.** 26a ersetzt den leeren Startzustand während des Jobs
  durch „Results found so far" und aktualisiert geprüfte/übersprungene Tracks,
  Safe-, Review-, Problem- und Unresolved-Zähler nach jedem abgeschlossenen
  Track. Der Zwischenstand ist ausschließlich lesbar: Review-Aktionen bleiben
  bis zum vollständigen Abschluss verborgen, er wird weder persistiert noch
  anwendbar. Cancel oder Fehler verwerfen ihn und zeigen wieder das letzte
  vollständig abgeschlossene Ergebnis aus DOC-2a.

- **DOC-3a** [aktiv] [core] — **Review entscheidet pro Feld.** Jede konkrete
  Track-Feld-Änderung besitzt eine eigene Auswahl. „All safe" ist ein
  Reset-Preset auf exakt alle aktuell zulässigen eindeutigen Local-Fixes und
  entfernt Remote-, manuelle, stale und unresolved Auswahl; „None" entfernt
  alles. Ein Gleichstand zeigt „N spellings, no clear winner — pick one" mit
  ausschließlich realen Kandidaten und ihren Häufigkeiten, ohne Default.
  Eine Kandidatenwahl materialisiert die betroffenen Track-Feld-Diffs;
  einzelne Rows bleiben abwählbar. Kandidatenwechsel berechnet sie neu und
  erhält manuelle Abwahlen, soweit dieselbe Row weiter betroffen ist. Die
  Review-Reihenfolge bleibt während der Sitzung stabil: ausgewählte Local
  safe, Tie-Gruppen, Remote ab 85 %, Remote 50–84 %, Remote unter 50 %,
  stale/conflict; darin Scope-Reihenfolge und die feste Feldfolge Title,
  Artist, Album, Album Artist, Year, Genre, Recording MBID. Apply erhält
  einen unveränderlichen Plan aus exakt der aktuellen Auswahl.

- **DOC-3b** [aktiv] [gtk] — **26b zeigt denselben Diff breit und schmal.**
  Breit stehen Checkbox · Track + Field · Current · Proposed · Source in
  einer virtualisierten Tabelle; leer erscheint als „— empty —", ein
  ersetzter Current-Wert durchgestrichen. Im schmalen Breakpoint stapelt
  dieselbe Row Current → Proposed ohne horizontalen Seiten-Scroll. Beide
  Darstellungen binden dieselbe Auswahl und erhalten Row-Fokus und stabile
  Reihenfolge beim Umschalten. Ellipsierte Werte besitzen Volltext-Tooltip
  und zugängliche Beschreibung. „Edit track tags…" öffnet den vorhandenen
  Tag Editor; dessen Save markiert betroffene Doctor-Zeilen stale und
  deselectiert sie. Der Footer führt Tracks als Handlungswährung:
  „Apply N tracks"; daneben „X tag changes · M files · undo available
  after".

- **DOC-4a** [aktiv] [core] — **Konfidenz wählt nie für den Nutzer.**
  Eindeutige Local-Fixes sind vorausgewählt; Remote-Vorschläge,
  Gleichstände, stale Zeilen und Konflikte nie. Eine gültige direkt
  aufgelöste MBID trägt 100 %, sonst bleibt der native MusicBrainz- bzw.
  AcoustID-Score erhalten. Stimmen mehrere Quellen überein, werden beide
  gezeigt und der niedrigere Score gilt; Scores werden nie gemittelt.
  Widersprechende Quellen erzeugen eine manuelle Kandidatengruppe. Bei
  mehreren Remote-Treffern darf nur dann ein einzelner Vorschlag entstehen,
  wenn der Spitzenwert mindestens zehn Prozentpunkte vor dem zweiten liegt
  und weder Dauer noch Entität widersprechen; sonst muss der Nutzer wählen.
  Unter 50 % bleibt ein Vorschlag ausdrücklich low-confidence und ungeprüft.
  Es gibt keinen Fuzzy-Auto-Merge.

- **DOC-4b** [aktiv] [gtk] — **Konfidenz ist redundant sichtbar.** Local
  erscheint mit Quelle „Local" im App-Akzent. Remote ab 85 % erscheint
  normal mit Quelle und Prozentwert, 50–84 % gelb, unter 50 % rot mit
  Warnsymbol und ungeprüfter Checkbox. Farbe ist nie die einzige Information:
  Quelle, Prozentwert, Warnsymbol bzw. zugänglicher Hilfetext tragen denselben
  Zustand. Kandidatendetails nennen Quelle, Score, Artist, Title, Album, Year
  und Dauerabweichung, soweit vorhanden.

- **DOC-5a** [aktiv] [core] — **Jeder Library-Doctor-Write geht durch
  Review.** Weder Scan noch 26a noch Plugin-Zeile besitzen einen
  Direkt-Schreibpfad. Ausschließlich Apply in 26b darf den unveränderlichen
  Review-Plan starten, und geschrieben werden nur dessen geprüfte Felder.
  Unmittelbar vor jeder Datei werden Track-/Pfadidentität und jeder erwartete
  Current-Wert aus der Datei erneut gelesen. Ein inzwischen verändertes Feld
  wird als Konflikt übersprungen, ohne andere weiterhin gültige ausgewählte
  Felder derselben Datei zu blockieren. Verschwundene oder verschobene Tracks
  fallen als unavailable/skipped aus dem Lauf, nicht als Write-Fehler.
  Library Doctor, Tag Editor und Revert benutzen dieselbe Lofty-
  Schreibprimitive und dieselbe Fehlerklassifikation.

- **DOC-5b** [aktiv] [core] — **Apply und Revert sind journalisierte
  Datei-Jobs.** Vor jedem Write speichert ein persistentes Journal pro Feld
  den unmittelbaren Before- und geplanten After-Wert; nur erfolgreich
  geschriebene Felder werden applied. Ein Crash rekonstruiert vorbereitete
  Einträge durch Dateiread: Current = After bedeutet applied, Current =
  Before nicht angewendet, jeder andere Wert Konflikt. Bereits erfolgreiche
  Writes bleiben bei Cancel bestehen; Cancel greift kooperativ zwischen
  Dateien, lässt den laufenden Container-Write sauber enden und startet keine
  weitere Datei. Es gibt weder Auto-Rollback noch Auto-Retry. Der letzte
  vollständig abgeschlossene Doctor-Cleanup bleibt über Neustarts revertierbar
  und wird erst nach sicherem Abschluss eines neueren ersetzt. Revert schreibt
  ein Feld nur, wenn Current weiterhin dem journalisierten After entspricht,
  läuft selbst abbrechbar zwischen Dateien und meldet partielle Erfolge,
  Fehler und Konflikte. Ein vollständiger Revert konsumiert den Cleanup;
  Tag-Editor-Jobs ersetzen dessen sichtbaren Pointer nie.

- **DOC-5c** [aktiv] [gtk] — **Schreibjobs frieren die UI nicht ein.**
  Apply und Revert laufen in der gemeinsamen Fortschrittskarte mit sichtbarem
  Cancel und derselben Geometrie wie Scan/Sync. Button, Fortschritt,
  Abschluss und Fehler zählen primär Tracks: „Apply 128 tracks",
  „Updating tags… 42/128 tracks", „Tags updated · 128 tracks" bzw.
  „42 tracks updated · 86 cancelled". Tag-Änderungen und Dateien stehen nur
  ergänzend. Ein erfolgreicher oder partieller Doctor-Write zeigt genau einen
  unverdrängbaren Undo-Klassen-Toast mit „Revert"; gesammelte Fehler erscheinen
  einmal als „N updated, M failed · Details", nie als Datei-Toast. Remote-
  Toggle und Apply-Auswahl sind während des Schreibjobs gesperrt.

- **DOC-5d** [aktiv] [gtk] — **Ergebnis und App bleiben nach Writes
  ehrlich aktuell.** Nach Apply oder Revert werden Trackliste, Browse Bar,
  Cover/Player-Metadaten, Sidebar, Albums, Artists und Stats über eine
  gemeinsame Tag-Mutation-Invalidierung erneuert; ein Neustart ist nie nötig.
  Ergebniszeilen bleiben sichtbar als Applied, Remaining, Failed, Stale,
  Conflict oder Reverted. Cancelled/unstarted und Failed bleiben für einen
  neuen Review-Lauf rekonstruierbar ausgewählt, stale/conflict ungeprüft.
  26a fasst partiell als „N applied · M remaining" zusammen. Reverted-Zeilen
  können erneut reviewed werden; ein neuer vollständiger Scan ersetzt das
  Scan-Ergebnis unabhängig vom weiterhin gültigen Undo-Journal.

- **DOC-6a** [ersetzt durch DOC-7b] [gtk] — **Library Doctor ist eine
  Hauptfenster-Navigation.** 26a lebt als Root-Page im bestehenden
  `content_nav`, 26b wird darauf gepusht; Back kehrt mit unveränderter
  In-Session-Auswahl zu 26a zurück. Es gibt keinen Doctor-Dialog und keinen
  zusätzlichen Apply-Bestätigungsdialog. Einstiege sind die Plugins-Seite
  mit Privacy-Untertitel „contacts MusicBrainz / AcoustID", das ⋮-Menü der
  Library und der STATS-DEDUP-Hinweis. Bei deaktiviertem Modul führt ein
  Einstieg zur hervorgehobenen Plugin-Zeile, bei aktivem zur Doctor-Seite;
  Preferences schließt vor der Hauptfenster-Navigation. Der Scope ist kein
  persistentes Plugin-Setting: Default Whole Library, aus gefilterter View
  Current View vorgeschlagen, aus Auswahlkontext Selection. Die aufgeklappte
  Plugin-Zeile zeigt Scope, Remote-Schalter, den Hinweis „local fixes always
  included · no network", „Run scan now" und „Revert last cleanup", aber
  keinen „Local fixes only"-Schalter. Revert bleibt auch bei deaktiviertem
  Plugin über eine minimale Doctor-Jobseite verfügbar und aktiviert weder
  Plugin noch Netzwerk.

- **DOC-6b** [aktiv] [gtk] — **Ein laufender Job hat genau einen Ort.**
  Scan, Apply und Revert überleben das Wegnavigieren; die eine
  Sidebar-Fortschrittskarte führt zur passenden Doctor-Seite zurück.
  Gleichzeitige Doctor-Jobs sind verboten. Library-Scan und Doctor-
  Scan/Apply/Revert laufen nicht parallel, und alle Tag-Writes sind global
  serialisiert; Playback, Navigation und lesender Device-Sync bleiben
  benutzbar. Ein erneuter Doctor-Einstieg während eines laufenden Jobs
  navigiert zu diesem Job statt einen zweiten zu starten. Scope,
  Remote-Toggle und Scan-Aktion sind während des Jobs gesperrt und erklären
  den laufenden Job; Cancel lebt ausschließlich an dessen
  Fortschrittsoberfläche.

- **DOC-7a** [aktiv] [gtk] — **Lokale Checks sind ein verfügbares
  Werkzeug; Netzwerk bleibt Opt-in.** Library Doctor hat keinen
  Hauptschalter und seine lokalen, rein lesenden Checks sind jederzeit
  manuell startbar. Das ist keine Netzfreigabe. Der getrennte, standardmäßig
  ausgeschaltete Schalter „MusicBrainz/AcoustID suggestions" zeigt beim
  ersten Einschalten eine kurze, versionierte Bestätigung mit der
  Daten-Allowlist aus DOC-1c; Abbrechen lässt ihn aus. Plugin-Zeile und
  Ergebnisansicht binden denselben persistenten Schalter. Ausschalten stoppt
  künftige Remote-Anfragen, versteckt Remote-Zeilen und entfernt deren
  Auswahl; erneutes Einschalten zeigt vorhandene oder neu geladene
  Remote-Vorschläge ungeprüft. Fehlende Fingerprint-Capability wird sichtbar
  als „AcoustID unavailable" erklärt, während Local und reine
  MusicBrainz-Auflösung weiter funktionieren.

- **DOC-7b** [aktiv] [gtk] — **Library Doctor ist eine direkt verfügbare
  Hauptfenster-Navigation.** 26a lebt als Root-Page im bestehenden
  `content_nav`, 26b wird darauf gepusht; Back kehrt mit unveränderter
  In-Session-Auswahl zu 26a zurück. Es gibt keinen Doctor-Dialog und keinen
  zusätzlichen Apply-Bestätigungsdialog. Einstiege sind die Plugins-Seite
  mit Privacy-Untertitel „contacts MusicBrainz / AcoustID", das ⋮-Menü der
  Library und der STATS-DEDUP-Hinweis; jeder Einstieg führt direkt zur
  Doctor-Seite. Der Scope ist kein persistentes Plugin-Setting: Default Whole
  Library, aus gefilterter View Current View vorgeschlagen, aus
  Auswahlkontext Selection. Die aufgeklappte Plugin-Zeile zeigt ohne
  Hauptschalter Scope, Remote-Schalter, den Hinweis „local fixes always
  included · no network", „Run scan now" und „Revert last cleanup". Revert
  bleibt über eine minimale Doctor-Jobseite verfügbar und aktiviert kein
  Netzwerk.

- **DOC-6c** [geplant] [manuell] — **Die sichtbare Abnahme entspricht den
  Frames 26a, 26b und 27.** Auf einem echten GNOME-Display werden breite und
  schmale Review-Geometrie, Zeilenvirtualisierung beim Scrollen,
  Durchstreichung und Empty-Darstellung, teal/gelb/rote Quellenzustände,
  41-%-Warnung, Fokusindikatoren im normalen und High-Contrast-Theme,
  Plugin-Aufklappen samt einmaliger Netzbestätigung sowie die gemeinsame
  Scan-/Apply-/Revert-Fortschrittskarte geprüft. Kein Text wird abgeschnitten,
  keine Spalte erzwingt horizontales Seiten-Scrolling, und die Oberfläche
  bleibt während echter Datei-Jobs bedienbar.

## Z. Einteiliger Track-Browser

- **BROWSE-1** [aktiv] [e2e] — **Music besitzt genau eine Trackliste.**
  Album und Interpret sind navigierbare, aus Track-Metadaten abgeleitete
  Library-Scopes derselben virtualisierten Trackliste, keine Tabs, Modi oder
  dauerhaften Datenbankentitäten. My Stats bleibt ein eigener Dashboard-Ort.

- **BROWSE-2** [aktiv] [core] — **Jeder Browser-Ort besitzt seinen Zustand.**
  Quelle, Scope, Textsuche, Facetten, Sortierung, ID-plus-Offset-Anker,
  Auswahl und stabiler Inhaltsfokus werden gemeinsam im History-Eintrag
  gehalten. Eine frische Album-/Interpret-Navigation startet unverfeinert;
  Back/Forward restauriert exakt. Ein leer gewordener Scope bleibt in der
  Sitzung als ehrlicher Leerzustand navigierbar.

- **BROWSE-3** [aktiv] [gtk] — **Sidebar-Einträge sind absolute Ziele.**
  Jede Aktivierung verlässt auch Utility-Seiten und routet in die aktive
  Zielansicht; Music führt aus einem Unter-Scope zur gemerkten Library-Wurzel.
  Ein bereits aktives Root-Ziel ist ein No-op. Laufende Jobs bleiben global
  sichtbar und blockieren Navigation nie.

- **BROWSE-4** [aktiv] [gtk] — **Metadaten navigieren appweit identisch.**
  Track, Album und Interpret lösen unabhängig von Playerleiste,
  Now-Playing-Panel, Trackliste, Queue, Cover oder My Stats genau die zentralen
  Intents RevealTrack, OpenAlbum und OpenArtist aus. Das Ziel selektiert,
  fokussiert und zentriert den Ankertrack; Back restauriert den Ausgangsort.

- **BROWSE-5** [aktiv] [core] — **Session-Restore ist begrenzt.** Der
  aktuelle Browser-Ort, die gemerkte Library-Wurzel und der strukturierte
  Wiedergabe-Ursprung werden restauriert. History, offene Suchoberflaechen,
  Utilities und rohe Widget-Fokusse überleben den Neustart nicht. Nicht mehr
  auflösbare Ziele fallen auf die Library-Wurzel zurück.

- **BROWSE-6** [aktiv] [core] — **Hörereignisse sind historische Fakten.**
  Jeder qualifizierte Play speichert den beim Wiedergabestart eingefrorenen
  Titel-, Album-, Interpret-, Genre-, Dauer-, Pfad- und MBID-Snapshot.
  Entfernen, Auto-clean oder Trash eines aktuellen Library-Eintrags löscht
  diese Ereignisse nicht; My Stats bleibt dadurch zeitlich stabil. Ein
  späterer Tag-Edit ändert alte Ereignisse nicht, während Track-Ranglisten
  bei mehreren Snapshots desselben Track-IDs die jüngsten Metadaten zeigen.
  Dialoge unterscheiden explizit Katalogfolgen von erhaltenem Hörverlauf.

- **BROWSE-7** [aktiv] [core] — **Entfernen, Trash und Listenaktionen sind
  verschiedene Befehle.** „Remove from library" lässt Dateien unberührt,
  entfernt aktuelle Katalog-, Rating-, Playlist- und Geräte-Sync-Daten und
  legt atomar eine persistente Scan-Ausnahme für die Dateidentität an; ein
  Rename derselben Datei hebt sie nicht auf. Preferences > Library zeigt die
  Anzahl und „Restore All" löscht die Ausnahmen und startet einen Rescan.
  „Move to Trash" verschiebt ausschließlich erfolgreich bestätigte Dateien,
  entfernt nur deren aktuelle Katalogdaten und erzeugt keine Ausnahme — eine
  später restaurierte Datei darf wiederkehren. „Remove from playlist/queue"
  ändert ausschließlich diese Liste. Der langlebige Hörverlauf folgt immer
  BROWSE-6.

- **BROWSE-8** [aktiv] [gtk] — **Katalog-Löschung unterbricht den geladenen
  Track nicht.** Wird der aktuell geladene Track entfernt, getrasht oder durch
  Wartung hart gelöscht, laufen sein Player-eigener Metadaten-Snapshot und die
  bereits geöffnete Audiodatei bis zum natürlichen oder expliziten
  Transportwechsel weiter. Alle zukünftigen Vorkommen gelöschter IDs
  verschwinden sofort aus Queue und Up Next; Repeat One kann einen gelöschten
  Track nicht erneut starten. Nach dem Wechsel wird auch der geladene
  Queue-Tombstone entfernt. Ein Track-Link auf eine nicht mehr vorhandene ID
  bleibt am Ausgangsort und erklärt dies per Toast; Album- und Interpret-Links
  öffnen weiterhin den Snapshot-Scope, jedoch ohne Phantom-Anker. Nach einer
  Löschserie bleiben überlebende ausgewählte Zeilen fokussiert; andernfalls
  fällt Auswahl und Fokus auf die nächste, am Listenende auf die vorherige
  Zeile und bei leerer Liste auf den stabilen Content-Container.

- **BROWSE-9** [aktiv] [gtk] — **Das Aufnahmedatum ist eine normale
  Library-Spalte.** „Added" ist im Spalteneditor wählbar, verschiebbar,
  breitenpersistierbar und nach `added_at` sortierbar. Die ISO-formatierte
  Zeit ist standardmäßig ausgeblendet; bestehende Layouts erhalten die neue
  Spalte beim Normalisieren ebenfalls ausgeblendet, ohne ihre gespeicherte
  Reihenfolge oder Sichtbarkeit zu verlieren.

- **BROWSE-10** [aktiv] [core] — **Widersprüchliche eingebettete Album-Cover
  werden kanonisiert.** Ist der Cover-Download aktiviert, erkennt der
  Bibliothekslauf verschiedene eingebettete Bilder für denselben
  normalisierten Album-Interpreten und Albumnamen und beschafft genau ein
  gemeinsames Cache-Cover. Dieses gewinnt danach für alle Tracks der
  Album-Identität; die Musikdateien bleiben unverändert. Bei deaktiviertem
  Modul oder nicht verfügbarem Netz bleibt die rein lokale Auflösung erhalten.

## AA. Externe Änderungen (Live-Refresh von CLI/MCP)

<!-- Sektionsbuchstabe: A–Z sind auf main bereits vergeben (T doppelt); die
     nächste freie Marke jenseits von Z ist AA. Die Buchstabenlage wurde beim
     Einfügen gegen den main-Stand verifiziert. Diese Sektion verankert
     Beschluss 6 des multi-frontend-core-Plans (Live-Sichtbarkeit fremd
     erzeugter Änderungen) und serialisiert vor Paket F, das sie später um die
     Instrumental-/Filter-Regeln ergänzt (Track 2). -->

Ein zweiter Prozess (CLI, MCP; künftig weitere Oberflächen) schreibt über
denselben Core-Pfad in dieselbe Datenbank. Die laufende App macht solche
fremden Änderungen sichtbar — ohne Neustart, als **Hintergrundereignis** und
damit nach P-1/P-4/MOT-2: leise, ohne Layout-Diebstahl, ohne eigene
Ankündigung. Die App refresht ihre *eigenen* Schreibaktionen weiterhin selbst
(Writer-Token-Filter); diese Sektion regelt ausschließlich den Fremd-Write.

- **EXT-1a** [aktiv] [gtk] — Fremd erzeugte Inhalte erscheinen ohne
  Neustart: eine von einem anderen Prozess über dieselbe Datenbank angelegte
  Playlist — allgemein jede fremde Änderung an Playlists, Smart-Playlists oder
  Katalog — wird in der laufenden App sichtbar; die betroffenen Ansichten
  (Sidebar, aktuelle Track-Liste) aktualisieren sich von selbst. Das
  Sichtbarkeitsbudget ist großzügig und degradiert bewusst (Notifier-Weckruf,
  bei nicht armierbarem Datei-Watch Polling); geprüft wird das *Was* (die
  Playlist erscheint), nicht das *Wie-schnell*.
- **EXT-1b** [geplant] [manuell] — Der Fremd-Refresh ist still: kein Toast,
  kein Badge, kein Indikator, keine Fokus-Wanderung als Ankündigung. Ein
  Hintergrundereignis bedient nie die Ankündigungs-Rolle (P-1); die
  Aktualisierung geschieht geräuschlos an Ort und Stelle.
- **EXT-2** [geplant] [gtk] — Selektion und Scrollposition überstehen den
  Fremd-Refresh: ein extern ausgelöster Reload setzt weder Auswahl noch
  Scrollposition zurück (navigations-neutraler Reload nach TAG-1). Eine
  unberührte Liste zahlt nichts — kein Anker, kein Sprung.
- **EXT-3** [geplant] [gtk] — Kein Fokus-Diebstahl: ein Hintergrund-Refresh
  entzieht der aktuellen Eingabe nichts, grabt keinen Fokus und zieht keine
  View in den Vordergrund. Der Nutzer bemerkt die Aktualisierung nur an neuen
  Inhalten, nie an springendem Fokus (P-3/P-4 in der Live-Refresh-Lesart).
- **EXT-4** [geplant] [core] — Laufende Wiedergabe und Queue bleiben
  unberührt: fremde Änderungen aktualisieren ausschließlich Ansichten. Die
  Wiedergabe-Queue ist ein Snapshot (`queue::snapshot`); ein Fremd-Write an
  der Bibliothek ändert weder die laufende Wiedergabe noch die Reihenfolge der
  bereits eingereihten Titel.
- **EXT-5** [geplant] [gtk] — Autorisierte externe Live-Queue-Befehle
  aktualisieren eine sichtbare Queue geräuschlos an Ort und Stelle: kein
  Toast, kein Fokus-, Selektions- oder Scrollpositionsverlust. Fehlende oder
  unbekannte Tracks werden nicht eingereiht.
  <!-- REVIEW: Regelvorschlag -->

## AB. Instrumental-Fassungen (experimentell)

<!-- Sektionsbuchstabe: A–Z sind vergeben (T doppelt), AA ist Externe
     Änderungen; die nächste freie Marke ist AB. Die Buchstabenlage wurde beim
     Einfügen gegen den main-Stand verifiziert (AA kündigt diese Sektion in
     ihrem Kopfkommentar an). Diese Sektion verankert die GTK-UX der
     Instrumental-Fassungen des multi-frontend-core-Plans (Abschnitt 2.4/3.2,
     Beschlüsse 11/13–19). Alle Fortschrittszahlen stammen ausschließlich aus
     den `ai_jobs`-Zeilen/Events — dieselben Zahlen wie CLI/MCP (Plan 2.2). -->

Eine Instrumental-Fassung ist ein **explizit beauftragter, dauerhafter,
klar als KI-manipuliert gekennzeichneter** Titel (CONTEXT.md), kein flüchtiger
Abspiel-Effekt. Das Feature ist **experimentell** (Beschluss 11): seine
gesamte UI erscheint nur hinter dem „Experimental features"-Schalter; raue
Kanten sind bewusst akzeptiert. Der Player spielt ausschließlich fertige
Dateien.

- **INST-1** [aktiv] [gtk] — Auslösung per Track-Kontextmenü: Bei
  aktivem Experimental-Schalter trägt das Track-Kontextmenü den Eintrag
  „Create instrumental"; er wirkt auf die **gesamte Auswahl** (Mehrfachauswahl
  → ein Batch mit gemeinsamer `batch_id` für Aggregat-Fortschritt) und ist bei
  reiner Missing-Auswahl inaktiv (eine fehlende Datei ist nicht separierbar).
  Ohne den Schalter erscheint der Eintrag nicht (INST-11). (Plan 2.4/1)
- **INST-2** [aktiv] [gtk] — Konvertierungs-Playlist = Staging-Bereich mit
  **genau einem Aggregat-Fortschrittsbalken** (fertig/gesamt + Prozent,
  gespeist aus den Job-Zeilen/Events, nicht aus Backend-internen Zahlen).
  **Weitere Fortschritts-UI gibt es nicht**: kein Sidebar-/Statusleisten-Slot
  (der android-sync-V2-Bottom-Slot wird nicht angefasst), **kein Toast**.
  (Beschluss 18)
- **INST-3** [aktiv] [gtk] — Je Zeile ein sichtbarer Zustand:
  queued / processing (mit Zeilen-Fortschritt) / done — ungespeichert /
  saved / failed. Die Ansicht ist technisch eine Spezial-View über `ai_jobs` +
  Staging-Store (Wiedergabe per Dateipfad), kein Playlist-Row-Source — auch
  wenn sie sich als Playlist anfühlt. (Plan 2.4/7)
- **INST-4** [ersetzt durch INST-4a und INST-4b] — Die ursprüngliche Regel
  bündelte die Sicht-Markierung und die tatsächliche Wiedergabe; sie wird in
  die view-seitige Markierung (INST-4a) und die reale Staging-Wiedergabe
  (INST-4b, P3b) geteilt.
- **INST-4a** [aktiv] [gtk] — In der Konvertierungs-Ansicht ist ein fertiger,
  im Staging vorhandener Render als **spielbar markiert** (Play aktiv), während
  ein noch verarbeitender Eintrag es nicht ist (er zeigt Fortschritt). Der
  Staging-Render ist eine echte Datei vor jeder Speicher-Entscheidung.
  (Beschluss 15, Plan 2.4/7)
- **INST-4b** [aktiv] [gtk] — Das Aktivieren eines spielbaren Eintrags
  spielt den Staging-Render (bzw. den promoteten Titel) **tatsächlich ab** —
  Wiedergabe per Dateipfad. Bis der Player das kann, ist die Aktion ein
  markierter Platzhalter (P3b).
- **INST-5** [ersetzt durch INST-5a und INST-5b] — Die ursprüngliche Regel
  bündelte die Klick-Entscheidung und die laufende Warte-Interaktion; sie wird
  in die View-Model-Entscheidung (INST-5a) und die App-Interaktion (INST-5b,
  P3b) geteilt.
- **INST-5a** [aktiv] [gtk] — Warte-Regel (Entscheidung): Ein Klick auf einen
  **noch verarbeitenden** Eintrag löst „Warten mit Fortschritt" aus — **nie
  Play** (kein Original-Fallback), **nie Auto-Skip**. Die reine View-Model-
  Entscheidung ist damit einklagbar, unabhängig von der Wiedergabe.
- **INST-5b** [aktiv] [gtk] — In der laufenden App blockiert der Klick auf
  einen verarbeitenden Eintrag den Start mit sichtbarem Render-Fortschritt und
  beginnt nach Abschluss (kein Fallback/Skip). Progressiver Frühstart ist eine
  spätere Optimierung, nicht v1 (P3b).
- **INST-6** [aktiv] [gtk] — Speicher-Entscheidung pro Zeile
  (Speichern / Verwerfen) plus „Alle speichern" in der Kopfzeile. Speichern
  **promotet** über die Core-Fassade (Move in den dedizierten Ordner, finale
  Tags inkl. KI-Provenienz, Registrierung — atomar, kein Re-Render); danach
  **wechselt die Zeile auf den promoteten Bibliothekstitel und bleibt**, bis
  der User aufräumt. Verwerfen löscht den Staging-Render; Unentschiedenes
  erscheint nie in der Library. (Beschluss 15/16)
- **INST-7** [aktiv] [gtk] — „Playlist leeren" **warnt**, wenn
  unentschiedene (done-ungespeicherte) Einträge existieren — Stunden
  Rechenzeit verdampfen nicht unbestätigt. (Beschluss 15)
- **INST-8** [aktiv] [gtk] — Unentschiedene Renders **bleiben über
  Neustarts erhalten**; ihre **Plattenkosten sind in der Ansicht sichtbar**
  (Größe je Zeile / Summe). Es gibt **keinen stillen Reaper** — nur die
  explizite Verwerfen-Aktion (oder Speichern) entfernt einen Render.
  (Beschluss 15)
- **INST-9** [aktiv] [gtk] — Drag eines **bereits konvertierten** Tracks in
  die Konvertierungs-Playlist erzeugt einen **Hinweis mit Verweis auf das
  Bestehende**, keinen Doppel-Job (Dedup-Skip der Core-Fassade). (Beschluss 16)
- **INST-10** [aktiv] [gtk] — Promotete Fassungen tragen ein sichtbares
  **KI-Badge** („Instrumental · KI-manipuliert") mit **Quellverweis**, sofern
  verknüpft. Die Provenienz ist DB-primär (`track_provenance`), die Tag-
  Referenz sekundär; das Badge schlüsselt auf das DB-Flag, nie auf den
  Ablageordner. (Beschluss 13/14)
- **INST-11** [aktiv] [gtk] — **Master-Gate:** Die gesamte Instrumental-UI
  — Kontextmenü-Eintrag, Konvertierungs-Ansicht, KI-Badges, „Hide AI music"-
  Filter (FIL-7) — ist **verborgen, solange der „Experimental features"-
  Schalter aus ist**. Der Schalter ist eine persistierte Einstellung; sein
  Zustand entscheidet allein über die Sichtbarkeit. (Beschluss 11)
- **INST-12** [aktiv] [gtk] — Modell-Bereitstellung: Hinter dem Schalter
  liegt der First-Use-Download-Flow der ML-Runtime-Gewichte über die
  Core-Fassade `ensure_weights` (Hintergrund-Thread mit Fortschritt,
  SHA-256-Checksum, Lizenznotiz neben der Datei, klare Fehlerpfade inkl.
  offline — Muster Cover-Download-Modul). Gewichte werden **nicht** ins
  Default-Build/Flatpak gebündelt. In einem Build **ohne** das
  `stem-backend`-Feature zeigt die Ansicht einen ehrlichen, deaktivierten
  Platzhalter mit Hinweis statt eines funktionslosen Buttons. (Beschluss 11)
- **INST-13** [aktiv] [gtk] — Erreichbarkeit: Die Konvertierungs-/Staging-Ansicht
  ist über einen eigenen **Sidebar-Eintrag** (`ViewSource::Conversions`, Titel
  „Instrumental conversions") erreichbar. Der Eintrag erscheint **nur, solange
  der „Experimental features"-Schalter an ist** (INST-11) — dieselbe Gatung, die
  auch die Inhaltsseite anlegt, sodass der Eintrag nie eine fehlende Seite
  auswählt. (Plan 2.4/7, Paket F)
- **INST-14** [geplant] [gtk] — Der Sidebar-Eintrag „Instrumental conversions"
  ist ein Drop-Ziel für Tracks aus der Library. Eine Mehrfachauswahl wird als
  ein Batch eingereiht; fehlende oder entfernte Tracks werden übersprungen,
  bestehende Arbeit wird nach INST-9 referenziert statt dupliziert. Fehlen die
  verifizierten Modell-/Runtime-Assets, öffnet die Aktion die Experimental-
  Einstellungen und legt keinen sicher scheiternden Job an.
  <!-- REVIEW: Regelvorschlag -->
## AD. Kompaktmodus / Mini-Player

<!-- Sektionsbuchstabe: Z (Einteiliger Track-Browser) ist die letzte
     einbuchstabige Sektion; A–Z sind vergeben (T doppelt belegt — Altlast),
     AA (Externe Änderungen) und AB (Instrumental) sind vergeben; AC ist das
     Regelpräfix des „Lokalen Klangprofils" (Sektion X), daher setzt der
     Compact-Mode mit AD fort. Die Regeln
     beschreiben ein bereits implementiertes und getestetes Feature: sie
     starten direkt [aktiv] mit vorhandenen mini_*-Tests als Nachweis.
     Referenz-Frames aus dem Redesign-Mockup: 1e (Ruhe), 9b (Hover),
     9c (Kontextmenü). -->

- **MINI-1** [aktiv] [gtk] — **Der Mini-Player ist das Fenster.** Ctrl+M
  togglet zwischen Voll- und Compact-Ansicht (auch über den ⋮-Eintrag
  „Compact Mode") — beide Richtungen, dieselbe Wiedergabe-Session, nichts wird
  neu aufgebaut; der Voll-Zustand bleibt unangetastet (BROWSE-2) und Ctrl+M
  zurück landet exakt dort. Es ist dasselbe, undekorierte Fenster; die Karte
  IST die Fläche: 430×76, Radius 16, Tint rgba(34,34,34,0.92), 1 px Hairline,
  opak — kein Live-Blur (STYLE-1); das Fenster selbst ist transparent (CSS),
  sodass nur die Karte schwebt — die Fenstergröße IST die Kartengröße (430×76),
  die Karte füllt das transparente Fenster randlos (jede positive Marge ließe die
  opake Adwaita-Fläche als „Rückenplatte" durchscheinen). Layout nach Frame 1e: Cover 52/Radius 10
  mit Inset-Hairline; Titel 13 px bold und Artist 11,5 px auf einer
  ellipsierenden Baseline-Zeile (Titel priorisiert, Artist-Kontrast ≥ 4,5:1
  auf dem Tint); darunter die Mini-Waveform (46 gleichbreite Bars, gespielter
  Teil im Playback-Akzent, Rest weiß ~18 %, Klick = Seek, Drag = Scrub);
  Play/Pause 38 px im Akzent. Kein Volume-, Prev- oder Next-Button sichtbar —
  bewusste Reduktion. Die Compact-Geometrie ist von der Vollfenster-Größe
  isoliert.

- **MINI-2** [aktiv] [gtk] — **Chromelose Karte — keine Hover-Buttons.** Die
  Karte trägt sichtbar nur den Play/Pause-Button; es gibt keine eingeblendete
  ⤢/✕-Chrome. Restore und Quit sind bewusst nur über das Rechtsklick-Menü
  (MINI-3), die Tastatur (Ctrl+M zurück ins Vollfenster, Ctrl+Q beendet) und
  einen Doppelklick auf Cover/Titel (= Restore) erreichbar. So schwebt die Karte
  ungestört und ein Play-Klick kann nie versehentlich ein ✕ (Quit) treffen. Die
  ganze Karte ist Drag-Fläche (GtkWindowHandle), außer Play-Button und Waveform.

- **MINI-3** [aktiv] [gtk] — **Rechtsklick-Menü mit fester Reihenfolge.**
  Rechtsklick, Menütaste oder Shift+F10 öffnet: Restore Full Window (Ctrl+M) ·
  Trenner · Pause/Play (Space; Label folgt dem Zustand) · Next (Ctrl+→) ·
  Previous (Ctrl+←) · Trenner · Always on Top (Toggle) · Trenner · Preferences
  (Ctrl+,) · Quit (Ctrl+Q). „Always on Top" ist X11-only (GTK4 kennt kein
  keep-above); wo es nicht unterstützt wird — Wayland — verschwindet der
  Eintrag ganz, statt tot als deaktivierte Zeile dazustehen.

- **MINI-4** [aktiv] [gtk] — **Tastatur identisch zum Vollfenster.** Space =
  Play/Pause, Ctrl+←/→ = Previous/Next, Ctrl+M = Restore, Ctrl+Q = Quit — keine
  Mini-Sonderbelegung. Ctrl+←/→ wirken als echte Tasten auf der Karte
  (Capture-Phase, damit die Pfeil-Seek der Waveform die modifizierten Pfeile
  nicht schluckt) und decken sich mit den im Kontextmenü gezeigten
  Acceleratoren.
- **MINI-5** [aktiv] [gtk] — Wird das Bibliotheksfenster so klein, dass die
  Vollansicht unbequem wird, bietet Reprise höchstens einmal pro Sitzung
  nichtblockierend „Use Compact Mode" an. Nur die ausdrückliche Aktivierung
  dieses Angebots wechselt über denselben Pfad wie Ctrl+M in die
  Kompaktansicht; Reprise schaltet nie allein um. Ist kein Player verfügbar
  oder die Kompaktansicht bereits aktiv, erscheint das Angebot nicht.

## AE. Concerts

<!-- Sektionsbuchstabe: AD (Kompaktmodus) ist die letzte vergebene Sektion;
     Concerts setzt mit AE fort. Die Regeln starten als Entwürfe und werden
     jeweils zusammen mit Verhalten und regelbenanntem Test aktiviert. -->

- **CONC-1** [aktiv] [gtk] — Concerts ist ein Sidebar-Ort in SMART und nur
  bei aktivem Modul sichtbar. Sein Badge entspricht exakt den kommenden,
  nach persistenten Filtern beim Öffnen sichtbaren Konzerten; 0 rendert
  keinen Badge.
- **CONC-2** [aktiv] [gtk] — Die Filterzeile ist ein permanenter Header.
  Idle zeigt sie leise Gesamtzahl und „+ Add filter"; jede aktive
  Einschränkung ist ein Chip mit eigenem ×-Ziel von mindestens 20 px.
  Aktiv zeigt sie „X of Y concerts" und „Clear all". Ohne Location ist
  Radius deaktiviert und trägt den Tooltip „Set a location in Preferences".
- **CONC-3** [aktiv] [gtk] — Doppelklick/Enter auf eine Zeile und die
  Ticket-Zelle öffnen dasselbe externe Ziel: Offer-URL, sonst Event-Seite.
  Ohne beides ist die Zelle leer und Aktivierung ein No-op mit Tooltip. Es
  gibt keinen Play-Pfad.
- **CONC-4** [ersetzt durch CONC-4a] — Ursprünglicher Zustandsvertrag ohne
  explizite Live-Neubewertung nach Änderungen der Concerts-Einstellungen.
- **CONC-4a** [ersetzt durch CONC-4b] — Ursprünglicher Zustandsvertrag mit
  Credential-Eingabehinweis und Preferences-Deep-Link.
- **CONC-4b** [aktiv] [gtk] — Ohne Credential zeigt Concerts neutral „No
  concert data yet" ohne Aktion; die Concerts-Sektion im Updates-Popover ist
  nicht sichtbar. Es gibt keinen Credential-Eingabehinweis und keinen
  Preferences-Deep-Link. Änderungen an Credentials, Location, Default-Radius,
  Zeitraum und Similar-Einstellungen bewerten die bereits offene View, ihre
  Sidebar-Zahl und das Updates-Popover sofort neu. Nie gefetcht bietet genau
  „Fetch now"; null Treffer mit Filtern genau „Show all". Offline oder Fehler
  lassen Cache und „Updated X ago" sichtbar und melden den Fehler
  ausschließlich inline im Footer.
- **CONC-5** [ersetzt durch CONC-5a] — Ursprünglicher Worker-Vertrag mit
  View-Open-Staleness, Due-Check und „Fetch now" als einzigen Netz-Triggern.
- **CONC-5a** [aktiv] [core] — Netz läuft ausschließlich im Worker oder
  `one_shot_task`. Trigger sind View-Open-Staleness (24 h plus Jitter), der
  stündliche Due-Check, „Fetch now" und eine explizit bestätigte
  Credential-Prüfung. Alle Concerts-Anfragen teilen den 1-req/s-Limiter.
  Track-Wechsel, Navigation und einzelne Credential-Tastendrücke lesen oder
  schreiben nur lokal; Fetch-Ergebnisse werden nach MOT-2 ohne
  Einblendanimation eingespielt.
- **CONC-6** [aktiv] [gtk] — Similar-Zeilen tragen dimm „similar to
  {seed}" und verschwinden mit „Library artists only". Die Source-Pill ist
  sichtbar, sobald Similar aktiviert ist oder Similar-Zeilen existieren.
- **CONC-7** [aktiv] [gtk] — Das Updates-Popover zeigt die Concerts-Sektion
  nur bei aktivem Modul, höchstens drei ungesehene Einträge des persistenten
  Filter-Scopes und „Show all concerts (N) →". Öffnen stempelt die gesamte
  Delta-Menge beider Sektionen. Das Header-Badge summiert ungesehene Einträge
  aller aktiven, fetch-bereiten Feeds nach dem `badge_presentation`-Idiom.
- **CONC-8** [aktiv] [core] [gtk] — Apply
  oder Enter an einer Credential-Zeile prüft den gespeicherten Wert genau
  einmal off-thread über den gemeinsamen Concerts-Limiter. Gültig, abgelehnt
  und nicht verifizierbar erscheinen inline; leer setzt den Zustand ohne
  Anfrage zurück. Die Prüfung schreibt Credential-Werte nie in Logs oder
  Fehlermeldungen.
- **CONC-9** [aktiv] [core] [gtk] — Ticketmaster-Credentials sind in der UI
  weder sichtbar noch editierbar. Der Core bevorzugt einen gespeicherten
  Altwert vor der Laufzeitumgebung und dem eingebetteten Build-Wert; leere
  Werte zählen nicht. Bandsintown bleibt als optionale Credential-Zeile
  unabhängig davon verfügbar.
- **CONC-10** [aktiv] [gtk] — Jede Concerts-Zeile besitzt eine gemeinsame
  vertikale Mitte. Der Interpret steht als einzeilige Gruppe auf derselben
  Grundachse wie Datum, Ort, Venue, Distanz und Ticket; eine optionale
  „similar to …"-Caption erweitert und zentriert die Interpretengruppe als
  Einheit, statt den Interpreten am oberen Zeilenrand festzuhalten.

## AF. Podcasts & Radio

<!-- Sektionsbuchstabe: AE ist nach der Landung von Concerts die letzte
     vergebene Sektion; dieser Branch belegt deshalb AF. Die Regeln starten
     geplant und werden jeweils im Implementierungs-Commit mit ihrem
     regelbenannten Test aktiviert. REVIEW: Regelvorschlag -->

Podcasts und Radio sind eigenständige Bibliotheksquellen, teilen aber eine
UX-Grammatik für Ort, Filter, Hinzufügen und reversibles Entfernen. Externe
Medien bleiben strukturell außerhalb der Track-Queue und der
Hörstatistik.

- **SRC-1** [aktiv] [gtk] — Podcasts und Radio stehen in der
  LIBRARY-Sektion zwischen Music und Queue und erscheinen nur bei aktivem
  Modul. Der Podcast-Zähler zeigt ungespielte Episoden, der Radio-Zähler
  Favoriten; null bleibt unsichtbar. Radio ist standardmäßig aktiv, weil es
  nur auf Nutzeraktion funkt; verbindliche Bedingung ist ein Radio-Leerzustand
  mit genau einer direkt erreichbaren „Add station"-Aktion.
- **SRC-2** [aktiv] [gtk] — Hinzufügen verwendet in beiden Quellen einen
  getönten rechteckigen Button mit Plus, Beschriftung und Radius 8, niemals
  die Chip-Form. Die gemeinsame Toolbar-Grammatik lautet Add-Button ·
  „Add filter" · aktive löschbare Filter-Pills · Zählung rechts; Filterzeilen
  behalten bei Zustandswechseln ihre Höhe.
- **SRC-3** [aktiv] [gtk] — Jede Quelle besitzt genau einen Add-Dialog mit
  genau einem Eingabefeld für Suchbegriffe oder URL. Suche liefert gruppierte
  Ergebnisse mit Zeilenaktionen; eine erkannte URL führt über Preview und
  Optionen zu einer Bestätigung. Netz- und Subprozessarbeit startet nur auf
  Submit und läuft nie auf dem GTK-Main-Loop.
- **SRC-4** [aktiv] [gtk] — Entfernen wirkt sofort, bleibt zehn Sekunden
  tombstoned und ist über einen hoch priorisierten Undo-Toast reversibel.
  Kontextmenü und Hover-Star bieten dieselbe destruktive Aktion; „Play Next"
  und „Add to Queue" fehlen vollständig. Podcast-Downloads werden beim
  Unsubscribe nie still gelöscht: der Commit-Toast meldet behaltene Dateien
  und bietet ausschließlich Verschieben in den Papierkorb an; mehrere
  Unsubscribes werden aggregiert.
- **POD-1** [aktiv] [core] — Episodenstatus ist pure Ableitung: Played
  genau bei gesetztem `played_at`, sonst Resume bei `position_ms > 0`, sonst
  New. Ein Episodenende setzt Played und löscht die Position. Die Tabelle
  lautet Date · Episode · Show · Length · Source · Status und sortiert
  standardmäßig nach Datum absteigend.
- **POD-2** [aktiv] [core] — RSS ist die Daten-API:
  enclosure/guid/pubDate/itunes:duration; GUID, ersatzweise Enclosure-URL und
  bei YouTube die Video-ID, ist die einzige Episodenidentität für Dedupe,
  Resume, Played und Download. Conditional Refresh läuft mit Intervall und
  deterministischem Jitter auf einem Worker; Upserts erhalten Seen- und
  Positionszustand. Automatischer Refresh verlangt aktives Modul, mindestens
  ein Abo, fällige TTL und eine nicht getaktete Verbindung.
- **POD-3** [aktiv] [core] — YouTube liegt ausschließlich hinter der
  yt-dlp-Providergrenze: Flat-Playlist zum Auflisten, Audioauflösung erst beim
  Abspielen und nie persistiert. Fehler werden lesbar klassifiziert und
  crashen nie. Fehlt das Binary, bleibt das Setting unverändert und die
  Degradierung wird am standardmäßig aktiven YouTube-Schalter sichtbar.
- **POD-4** [aktiv] [gtk] — Episoden starten an der gespeicherten Position;
  diese wird gedrosselt sowie bei Pause, Stop, Wechsel und Beenden
  persistiert. Nach dem Ende bietet die App die nächste ungespielte Episode
  derselben Show nach Datum per Toast und persistentem Player-Bar-Button an,
  spielt sie aber nie automatisch. Podcast-Sessions erzeugen weder Scrobbles
  noch `listen_events` oder Play-Counts.
- **POD-5** [aktiv] [gtk] — Downloads sind pro Abo opt-in, liegen im
  XDG-Datenpfad der App unter einem GUID-stabilen Pfad, folgen der gewählten
  Cleanup-Policy und werden offline bevorzugt lokal abgespielt.
- **POD-6** [aktiv] [core] [gtk] — Einzelne RSS- und YouTube-Episoden lassen
  sich im Kontextmenü entfernen, verschwinden sofort und bleiben zehn Sekunden
  per Undo reversibel. Der Commit löscht nur den Datenbankeintrag und sperrt
  seine quellstabile GUID dauerhaft gegen erneuten Feed-Import; eine
  heruntergeladene Datei bleibt erhalten und kann ausschließlich über die
  angebotene Papierkorb-Aktion entfernt werden.
- **RAD-1** [aktiv] [gtk] — Nur die aktuell verbundene Station ist in der
  Tabelle akzentuiert; ihr Zustandsicon, Name, Now-playing und Zeilentint
  wechseln gemeinsam. Alle anderen sowie eine präsentierte, aber getrennte
  pausierte Station zeigen „—". Nur die Player-Bar darf den letzten ICY-Titel
  gedimmt als Session-Gedächtnis behalten.
- **RAD-2** [aktiv] [gtk] — Live-Wiedergabe besitzt weder Seek noch Dauer:
  Player-Bar und Mini-Player zeigen Elapsed und einen geometriegleichen
  Waveform-Platzhalter, MPRIS meldet `CanSeek=false` und keine Länge. Pause
  trennt den Stream, bleibt aber als Paused/CanPause mit Station und
  gedimmtem letztem Titel präsentiert; Play verbindet live neu. Ein
  Reconnect-Fehler lässt den pausierten Zustand mit Inline-Fehler und Retry
  stehen. Radio erzeugt keine Hörstatistik; erneute Aktivierung der laufenden
  Zeile stoppt.
- **RAD-3** [aktiv] [core] — Radio-browser-Server werden über den
  Discovery-Endpunkt gewählt und bei Fehler rotiert. Jeder Start einer
  UUID-Station meldet den Etikette-Klick; ein toter Stream wird vor der
  Fehleranzeige genau einmal über seine UUID neu aufgelöst.
- **RAD-4** [aktiv] [core] — Eingefügte Radio-URLs werden höchstens eine
  Ebene durch PLS oder M3U bis zur Stream-URL aufgelöst; HLS-Manifeste bleiben
  selbst die Stream-URL. Die Preview liest Name, Bitrate, Genre und
  Content-Type ausschließlich aus ICY-/HTTP-Headern und streamt keinen Body.

---

Wenn beim Testen ein Fall auftaucht, den keine Regel deckt: Regel ergänzen
(Prozessregeln oben), nicht lokal entscheiden.
