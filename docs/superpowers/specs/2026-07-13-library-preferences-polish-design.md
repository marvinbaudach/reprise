# Bibliotheksansicht und Einstellungen — visuelle Konsolidierung

## Ziel

Die Bibliotheksansicht und die Einstellungen werden auf den bereits festgelegten
Reprise-Look aus dem Master-Design und dem GTK4-Mockup 7a–7c zurückgeführt. Die
Etappe ergänzt keine neuen Musikfunktionen. Sie ordnet vorhandene Bedienwege,
nutzt den verfügbaren Platz besser und macht die bereits implementierten
Einstellungen als zusammengehörige native Adwaita-Oberfläche verständlich.

Die Bibliothek bleibt das dichte Arbeitsfenster für große Sammlungen. Die
Einstellungen bleiben ein fokussiertes Fenster, in dem jede sichtbare Option
sofort wirkt. Beide Oberflächen teilen eine ruhige Hierarchie: eine klare
Primäraktion, zurückhaltende Sekundäraktionen, native Abstände und keine
dekorative Sonderoptik.

## Ausgangslage

Der aktuelle Funktionsstand ist vollständig, weicht visuell aber an mehreren
Stellen von der Referenz ab:

- Feste Spaltenbreiten lassen rechts neben kleinen oder mittleren Layouts viel
  ungenutzte Fläche, obwohl die Trackliste den Hauptbereich bilden soll.
- Suche, Quelltitel, Import, Scan, Kompaktansicht und Hauptmenü konkurrieren in
  der Headerbar. Textknöpfe dominieren stärker als die eigentliche Quelle.
- Die drei Browse-Felder wachsen auf sehr breiten Fenstern unnötig weit und
  wirken dadurch wie ein zweiter Header statt wie ein optionaler Filter.
- Die im Referenz-Mockup 7e vorgesehene rechte Informationsspalte fehlt. Damit
  besitzen lokale Kontextinformationen und aktivierte Plugin-Inhalte keinen
  ruhigen, dauerhaft erreichbaren Ort neben der Trackliste.
- Zähler in der Sidebar sind typografisch schwach vom Eintragsnamen getrennt.
- Der Preferences-Dialog zeigt fünf gleichrangige Bereiche in einer unteren
  Leiste. Auf langen Seiten verdeckt diese Leiste Inhalt; auf kurzen Seiten
  bleibt sehr viel unstrukturierte Leerfläche.
- Die Layoutauswahl ist funktional, zeigt die wichtige Position der
  Playerleiste aber nur als Textauswahl statt wie im Mockup als Vorschau.
- Zehn Equalizer-Zeilen erzeugen eine lange technische Form statt eines
  erkennbaren Equalizers.

## Gestaltungsgrundsätze

1. **Native Adwaita-Hierarchie.** Struktur entsteht durch `AdwHeaderBar`,
   `AdwPreferencesGroup`, Boxed Lists, `AdwClamp`, native Auswahlzustände und
   Standardabstände. Eigenes CSS beschränkt sich auf Dichte, kleine
   Vorschauflächen, Akzentzustände und bereits vorhandene Coverradien.
2. **Inhalt vor Chrom.** Trackliste und Einstellungsinhalt erhalten die Fläche;
   Header, Browse und Status bleiben flach und kompakt.
3. **Eine sichtbare Primäraktion pro Bereich.** Häufige Handlungen bleiben
   direkt erreichbar, seltene Handlungen wandern in ein benanntes Menü.
4. **Keine reine Farbbedeutung.** Auswahl, aktive Wiedergabe, Filter und
   Pluginzustand besitzen zusätzlich Text, Icon, Häkchen oder Schalter.
5. **Adaptiv, nicht abgeschnitten.** Breite Fenster nutzen horizontale
   Anordnung. Bei schmaler Geometrie umbrechen oder kollabieren Bedienelemente,
   ohne Trackdaten, Dialognavigation oder Aktionsknöpfe zu überdecken.
6. **Kontext statt Dashboard.** Die rechte Spalte erklärt die aktuelle Auswahl
   und zeigt dazu passende Beiträge aktivierter Module. Sie wird keine zweite
   Navigation, kein Webfeed und keine Ansammlung globaler Einstellungswidgets.

## Bibliotheksansicht

### Fensteraufbau

Die bestehende `AdwNavigationSplitView`-, `AdwToolbarView`- und
`GtkColumnView`-Architektur bleibt erhalten. Die Etappe ändert weder Quellen,
Queries, Queue noch Playbackzustand.

```text
┌ Sidebar ────┬ Header: Suche          Musik              [Info] [Menü] ┐
│ BIBLIOTHEK  ├ Filter: Genre       Interpret       Album          ┬ Info  × │
│ Musik   504 │ Titel    Interpret    Album    Jahr  Länge  ★      │ Cover   │
│ Queue    12 │ …                                                  │ Titel   │
│ PLAYLISTEN  │ …                                                  │ Artist  │
│ Training 48 │ …                                                  │─────────│
│ INTELLIGENT ├──────────────── 504 Titel · 1 Tag, 8 Std. ─────────┤ Plugin  │
└─────────────┴ Playerleiste: Cover · Metadaten · Transport ───────┴─────────┘
```

### Headerbar

- Links steht die bestehende Suche. Sie erhält eine begrenzte natürliche
  Breite, expandiert nicht über den Quelltitel und bleibt über `Ctrl+F`
  fokussierbar.
- Der aktuelle Quelltitel bleibt zentriert und ist der einzige hervorgehobene
  Text in der Headerbar.
- Rechts folgen der Informationsspalten-Schalter und das Hauptmenü. Seltene
  Bibliothekspflege und Darstellungswechsel werden nicht als zusätzliche
  dauerhafte Header-Aktionen dupliziert.
- „Playlist importieren…“ liegt im Hauptmenü. Es bleibt eine globale Aktion,
  wird aber nicht dauerhaft als großer Textknopf gezeigt.
- Ordnerwahl und „Bibliothek neu scannen“ bleiben unter Bibliothek in den
  Einstellungen sowie in der Ersteinrichtung erreichbar. Sie verwenden
  weiterhin den einen vorhandenen `ScanControls`-Zustand; im Library-Header
  erscheint kein zusätzlicher Scan-Knopf.
- Der Sidebar-Knopf bleibt ganz links erreichbar, solange die Sidebar in den
  Layouteinstellungen aktiviert ist. Bei breitem Fenster klappt er die
  vollständige linke Spalte ein und gibt den Platz der Tabelle zurück; im
  schmalen Split-View wechselt er zwischen Navigation und Content. Alle
  icon-only Knöpfe besitzen Tooltip, Accessible Name und natives Fokusziel.

### Browse-Leiste

- Die Leiste bleibt ausschließlich in `Library` sichtbar und behält ihre
  kaskadierende Semantik Genre → Interpret → Album.
- Ein `AdwClamp` begrenzt den Filterinhalt auf ungefähr 1.050 logische Pixel.
  Bei großen Fenstern werden Dropdowns daher nicht beliebig breit; die
  Außenfläche bleibt ruhig.
- Die drei Facetten erhalten gleiche flexible Breite. Beschriftung und Feld
  bilden je eine zusammenhängende Einheit. Aktive Werte bleiben einschließlich
  Trefferzahl sichtbar und ellipsieren vor dem Pfeil.
- Unter ungefähr 720 Pixel Contentbreite wechselt die Leiste in drei kompakte
  Zeilen. Kein horizontaler Scrollbereich und kein abgeschnittener Dropdown-
  Pfeil wird eingeführt.
- „Browse-Leiste anzeigen“ wird als persistente Layoutoption ergänzt. Der
  Standard bleibt sichtbar, damit das bestehende Verhalten unverändert
  startet. Ausblenden löscht den aktiven Filter nicht; der Filter bleibt in der
  aktuellen View-Session erhalten und wird beim Wiedereinblenden sichtbar.

### Trackliste

- Gespeicherte Spaltenreihenfolge, Sichtbarkeit und Breiten bleiben die
  Wahrheit. Die primäre Titelspalte nimmt zusätzlich verbleibende Fläche ein,
  sodass die Tabelle stets bis zum rechten Rand reicht; explizite
  Nutzerbreiten bleiben Mindest-/Basisbreiten und werden nicht überschrieben.
- Cover, Titel, Interpret und Album ellipsieren. Jahr, Länge, Bewertung und
  andere numerische Spalten bleiben kompakt und rechts beziehungsweise
  typografisch numerisch ausgerichtet.
- Vertikale Zellgitter werden nicht als dominantes Raster verwendet. Native
  Zeilentrennung, Hover, Mehrfachauswahl und die bestehende Akzentmarkierung des
  laufenden Titels tragen die Orientierung.
- Die drei vorhandenen Dichten bleiben erhalten: komfortabel 48 px, Standard
  36 px, kompakt 28 px. Cover passen sich der Zeilenhöhe an, ohne die Zeile
  nachträglich zu vergrößern.
- Der Spalteneditor bleibt über das Listenmenü und die Layout-Einstellungen
  erreichbar. Diese Etappe ändert keine Sortier-, Drag- oder Bewertungslogik.

### Rechte Kontext- und Pluginspalte

Die rechte Spalte ist ein verschließbares `AdwOverlaySplitView` mit
`sidebar-position=end`, verschachtelt innerhalb des bestehenden Content-Panes.
Damit bleiben die linke Navigation und die rechte Information unabhängig:
links wechselt der Nutzer die Quelle, rechts betrachtet er Kontext derselben
Quelle.

- Ab ungefähr 1.180 Pixel Fensterbreite ist die 320–380 Pixel breite Spalte
  angeheftet und verkleinert die Trackliste. Unterhalb davon öffnet sie als
  native Overlay-Seitenleiste über dem Content. Unter ungefähr 720 Pixel ist
  sie standardmäßig geschlossen, bleibt aber über den Headerknopf erreichbar.
- Der Headerknopf zeigt und verbirgt die Spalte. Die Auswahl wird unter
  `ui.info_panel_visible` sofort persistiert; Standard ist sichtbar. Eine
  schmale Fenstergeometrie darf den gespeicherten Wunsch für den nächsten
  breiten Start nicht überschreiben.
- Die Spalte besitzt eine kleine eigene Headerbar „Information“ mit
  Schließen-Knopf und darunter genau einen vertikalen Scrollbereich. Beim
  Schließen oder Größenwechsel gehen Kontext und Pluginzustand nicht verloren.
- Ein einzelner ausgewählter Track bestimmt den lokalen Kopf: Cover, Titel,
  Interpret und Album. Ohne Auswahl wird der aktuell geladene Track verwendet.
  Bei Mehrfachauswahl zeigt der Kopf nur Anzahl und gemeinsame Aktionsebene;
  uneinheitliche Metadaten werden nicht erfunden. Wenn weder Auswahl noch
  aktueller Track existiert, fordert eine ruhige Statusseite zum Auswählen auf.
- Unter dem lokalen Kopf folgen Karten aktivierter Module, sofern sie für den
  aktuellen Kontext Inhalt liefern. Beispiele sind Coverdownload-Status,
  zukünftige Radar-/Interpreteninformationen, Lyrics oder Scrobblerstatus.
  MPRIS erhält keine Karte, weil seine D-Bus-Bereitschaft keine nützliche
  trackbezogene Information ist.
- Die vorhandene Coverdownload-Karte verwendet den bestehenden
  `CoverDownloadBatch`-Zustand über einen schwachen Subscriber. Sie zeigt
  geprüft/heruntergeladen/nicht verfügbar und den Default-off-Netzwerkhinweis,
  startet aber niemals selbst einen Lauf.
- Modulbeiträge werden in stabiler Prioritätsreihenfolge dargestellt und
  verschwinden sofort, wenn das Modul deaktiviert wird. Ein fehlerhafter oder
  leerer Beitrag entfernt nur seine Karte; lokale Metadaten und andere Karten
  bleiben benutzbar.
- Onlinebeiträge führen keine Netzwerkanfrage allein durch das Öffnen der
  Spalte aus. Sie verwenden ausschließlich Cache und Laufzeit eines bewusst
  aktivierten Moduls. Links öffnen sich über den bestehenden sicheren
  Desktop-/Portalpfad, niemals in einem eingebetteten WebView.
- Das Panel besitzt keine eigene Trackauswahl und verändert weder Queue noch
  Playback. Aktionen innerhalb einer Modulkarte rufen ausschließlich den
  vorhandenen Controller beziehungsweise Modul-Callback auf.

### Sidebar und Status

- Abschnittsüberschriften bleiben klein, gedimmt und in Versalien. Zwischen
  Abschnitten liegt mehr Abstand als zwischen Einträgen desselben Abschnitts.
- Zähler werden als kompakte, rechts ausgerichtete Badge-Fläche dargestellt.
  Die Badge ist auch ohne Akzentfarbe lesbar und bleibt für `Queue`, Playlists,
  Importfehler und fehlende Dateien semantisch identisch.
- Die aktive Quelle verwendet weiterhin die native ausgewählte ListBox-Zeile;
  es gibt keine zweite farbige Seitenmarkierung.
- Die Statuszeile bleibt direkt über der Playerleiste, nutzt die gesamte
  Contentbreite und richtet ihre Zusammenfassung rechts aus. Bei schmaler
  Breite ellipsiert die Dauer zuerst; die Playerleiste wird nie überdeckt.

## Einstellungen

### Fenster und Navigation

Die Einstellungen öffnen als eigenes, zur Library gehörendes, aber
nicht-modales `AdwWindow`. Das Fenster lässt sich über seine native Headerbar
verschieben, bleibt einzeln instanziert und wird beim erneuten Aufruf in den
Vordergrund geholt. Sein Standardinhalt ist ungefähr 760 × 680 logische Pixel,
mit einem sicheren kleineren Minimum und vertikal scrollenden Seiten.

- Reihenfolge: **Wiedergabe · Darstellung · Layout · Bibliothek · Plugins**.
- Ein `AdwViewSwitcher` sitzt unabhängig von der Fensterbreite in der oberen
  Headerbar wie im Mockup 7b/7c. Die aktive Seite besitzt zusätzlich den
  nativen Auswahlhintergrund. Es gibt keine zweite Navigationsleiste am unteren
  Rand; bei kleiner Breite komprimiert die obere native Tabdarstellung.
- Jede Seite besitzt einen eigenen Scrollzustand. Seitenwechsel verändern
  keine Einstellung und schließen keinen offenen Portal-Dialog.
- Die Seitensymbole bleiben: Playback, Appearance, Layout, Library, Plugins.
  Titel und Accessible Names sind vollständig übersetzt.

### Wiedergabe

- Gruppe „Equalizer“: Ein/Aus und Preset bilden zwei normale Boxed-List-Zeilen.
  Darunter zeigt eine zusammenhängende Equalizer-Fläche zehn vertikale native
  Skalen mit Frequenz unterhalb und aktuellem dB-Wert oberhalb. Die Fläche
  scrollt bei schmaler Breite horizontal innerhalb ihrer Gruppe; die gesamte
  Dialogseite erhält keinen horizontalen Scrollbalken.
- Jede Skala besitzt einen eindeutigen Accessible Name, Tastatursteuerung und
  den unveränderten Bereich der vorhandenen Engine. Deaktiviert bleibt die
  Kurve sichtbar, aber nicht bedienbar.
- Gruppe „ReplayGain“ bleibt darunter als kompakte Modusauswahl. Es werden
  keine neuen Playbackoptionen oder DSP-Pfade eingeführt.

### Darstellung

- Eine Gruppe „Farbschema“ zeigt System, Hell und Dunkel als drei gleichwertige
  native Auswahlkarten mit klarer Textbeschriftung. Die Karte verwendet nur
  einfache helle/dunkle Flächen als Vorschau; keine Screenshots, Schatten oder
  Glasoptik.
- Die Auswahl wirkt weiterhin sofort über `AdwStyleManager`. Ein
  Persistenzfehler stellt die vorherige Karte und das vorherige Farbschema
  wieder her und zeigt den bestehenden Toastpfad.

### Layout

- Gruppe „Playerleiste“ zeigt die Positionen Oben und Unten als zwei große
  Auswahlkarten. Jede Vorschau enthält Sidebar, Contentfläche und einen klaren
  Balken an der entsprechenden Kante. „Unten“ bleibt Standard. Die verworfene
  schwebende Variante kehrt nicht zurück.
- Gruppe „Bibliotheksfenster“ enthält Schalter für Sidebar, Browse-Leiste,
  Informationsspalte und Statuszeile sowie die Dichteauswahl. Der
  Sidebar-Schalter entfernt beziehungsweise restauriert den vollständigen
  linken Slot des `AdwNavigationSplitView`; es bleibt keine leere Spalte stehen.
  Die Begriffe beschreiben sichtbar genau die Elemente des Hauptfensters.
- Gruppe „Spalten“ zeigt die aktuell sichtbaren Spalten in der Subtitle-Zeile
  und öffnet denselben vorhandenen Spalteneditor als zweite Ebene der
  bestehenden Preferences-Navigation. Die Detailseite ersetzt den
  Preferences-Inhalt im selben Fenster und kehrt über den nativen
  Zurück-Button zur Layout-Seite zurück; sie erzeugt kein weiteres Fenster.
  Keine zweite Spaltenkonfiguration wird aufgebaut.
- Die Kompaktansicht wird in den Einstellungen nicht erneut angeboten. Sie
  bleibt über das vorhandene Kontext-/Hauptmenü und den Shortcut erreichbar;
  die Einstellungen duplizieren diesen Bedienweg nicht.

### Bibliothek

- Gruppe „Musikordner“ zeigt den aktuellen Pfad als Subtitle. „Ordner wählen…“
  bleibt die eine hervorgehobene Aktion der Zeile.
- „Bibliothek neu scannen“ liegt direkt darunter und verwendet den gemeinsamen
  Scanfortschritt. Eine laufende Operation ist in Einstellungs- und Hauptfenster
  derselbe Zustand, kein zweiter Worker.
- Gruppe „Import“ enthält „Rhythmbox-Spaltenlayout importieren“. Die Zeile
  erklärt ausdrücklich, dass Reprise nur liest und Rhythmbox nicht verändert.
- Coverdownload bleibt unter Plugins, weil er eine optionale
  Netzwerk-Integration ist. Ordnerliste, mehrere Bibliothekswurzeln und
  Dateiverwaltung werden nicht vorgetäuscht.

### Plugins

- Eine einleitende Beschreibung erklärt, dass Plugins optionale Integrationen
  sind, kontextuelle Karten in der rechten Informationsspalte liefern können
  und feste Playbackfunktionen nicht hier erscheinen.
- Jede Integration bleibt eine `AdwSwitchRow` mit Name, kurzer Wirkung und
  gegebenenfalls Neustarthinweis. MPRIS und Coverdownload behalten ihre
  bestehenden Defaultwerte und Lebenszyklen.
- Laufender Coverdownload-Fortschritt steht direkt unter der zugehörigen Zeile
  und bleibt beim Seitenwechsel erhalten. Preferences und Informationsspalte
  abonnieren denselben `CoverDownloadBatch` jeweils schwach; es entsteht weder
  ein zweiter Zustand noch ein zweiter Worker.

## Architektur und Dateigrenzen

- `window.rs` bleibt Composition Root und wird nicht über 800 Zeilen erweitert.
  Headeraufbau und Library-Layoutverkabelung werden in ein fokussiertes
  Geschwistermodul extrahiert.
- `track_list_columns.rs` bleibt Eigentümer der Spaltenwidgets;
  Fill-/Ellipsierungsregeln landen dort, nicht im Composition Root.
- `browse_bar.rs` bleibt Eigentümer der kaskadierenden Filter. Eine kleine pure
  Breakpoint-/Layoutentscheidung wird getrennt testbar gehalten.
- Ein neues `info_panel.rs` besitzt ausschließlich die rechte
  `AdwOverlaySplitView`-Oberfläche und schwache Kontext-/Modulcallbacks;
  `info_panel_state.rs` entscheidet Auswahlfallback, Sichtbarkeit,
  Breakpointmodus und stabile Sektionsreihenfolge ohne GTK.
- Modulbeiträge verwenden eine kleine frontend-interne, statisch registrierte
  `InfoPanelSection`-Schnittstelle. Sie ist keine Fremd-Plugin-ABI und reicht
  keine GTK-Typen in `reprise-core` durch. Modulaktivierung bleibt allein bei
  `reprise_core::modules`.
- `preferences.rs` delegiert die verschiebbare Fenster-Shell und die obere
  Tabnavigation an `preferences_window.rs`. `preference_library.rs` und
  `preference_effects.rs` bleiben fokussierte Seitenhelfer.
- Die neuen Browse- und Informationsspalten-Sichtbarkeitswerte leben typisiert
  in `reprise_core::library::settings`. Core erhält keine GTK-, Adwaita-,
  GStreamer- oder zbus-Abhängigkeit.
- Die parallele Kompaktplayer-Arbeit ist Merge-Grundlage. Vor der
  Implementierung wird dieser Branch auf deren abgeschlossene Etappe rebased;
  `minimal_view.rs`, `compact_player*` und deren Playbackprojektion gehören
  ausdrücklich nicht zu dieser Etappe.

## Persistenz, Fehler und Sicherheit

- Jede Layoutänderung wird zuerst persistiert und erst danach dauerhaft in den
  sichtbaren Zustand übernommen. Bei Fehlern bleiben alter Zustand und
  Auswahlmarkierung aktiv; ein Toast benennt den Fehlschlag.
- DB-`Ref`/`RefMut` endet immer vor GTK-, Action- oder Callback-Aufrufen.
- Portalwahl und Rescan verwenden die bestehenden sicheren Pfade. Kein Test und
  kein Smoke greift auf die reale Datenbank oder Musikbibliothek zu.
- Rhythmbox-GSettings bleiben read-only. Musikdateien werden in dieser Etappe
  weder geschrieben, verschoben noch gelöscht.
- Das Einstellungsfenster hält Callbacks schwach oder klont sie vor dem Aufruf
  aus `RefCell`s. Seitenwechsel und Fensterschließen erzeugen keine
  Referenzzyklen.

## Tests und Verifikation

- Reine Tests prüfen Browse-/Informationsspalten-Fallback und Roundtrip,
  adaptive Browse-/Panel-Anordnung, Kontextfallback Auswahl → aktueller Track →
  leer, stabile Pluginreihenfolge, Headeraktionsreihenfolge, Spalten-Fillregeln,
  Preferences-Seitenreihenfolge und Auswahl-Rollback.
- Displaytests prüfen Library bei 1.440, 900 und 640 Pixel Breite: keine
  ungenutzte rechte Tabellenfläche, kein abgeschnittener Header, korrekter
  Browse-Umbruch sowie erreichbare linke Sidebar und rechte Informationsspalte.
- Ein Displaytest öffnet die Informationsspalte, wechselt Einzel-, Mehrfach-
  und keine Auswahl, deaktiviert einen Modulbeitrag und beweist, dass lokale
  Metadaten, Kartenreihenfolge und Close-/Restore-Zustand korrekt bleiben.
- Je ein Displaytest prüft das verschiebbare Preferences-Fenster, die obere
  Tabnavigation, sichere Content-Unterkante, die beiden
  Playerleisten-Vorschauen, alle zehn Equalizer-Skalen und Accessible Names.
- Der vorhandene PTR-Harness nimmt Library, geöffnete Informationsspalte sowie
  alle fünf Preferences-Seiten auf und bedient Paneltoggle, Scan, Layout,
  Spalteneditor und Plugin-Schalter über echte Pointer-/Tastaturpfade.
- Vollständige Gates, Rustdoc, gettext-Abdeckung, Core-Purity,
  Releasechecker, isolierte App-Smokes und die 800-Zeilen-Regel bleiben
  verpflichtend.

## Manuelle native GNOME-Prüfung

- Proportionen, Ellipsierung und Badge-Wirkung mit einer großen befüllten
  Wegwerf-Bibliothek.
- Helle/dunkle/System-Vorschau und tatsächlicher Themewechsel unter GNOME.
- Touchziele, HiDPI, schmale Split-View-Navigation und horizontale Bedienung
  der Equalizer-Fläche.
- Portal-Ordnerwahl und sichtbarer Scanfortschritt auf einem realen Desktop.

## Explizit nicht Teil

- Keine Grid-/Albumcover-Bibliothek, Künstlerdetailseite oder neue
  Navigationsquelle.
- Kein allgemeines Plugin-Dashboard, kein eingebettetes WebView, keine
  Fremd-Plugin-Widget-ABI und kein Netzwerkladen nur durch Öffnen des Panels.
- Kein frei gestaltbares Theme, keine benutzerdefinierten Farben, keine
  Transparenz, kein Blur und keine schwebende Playerleiste.
- Keine neue Queue-, Sortier-, Browse-, Such-, Tag-, Datei- oder
  Playbacksemantik.
- Keine mehreren Bibliotheksordner, keine Dateiverwaltung und kein Schreiben
  fremder GSettings.
- Keine Fremd-Plugin-Installation und keine Geräte-Synchronisationsseite ohne
  implementiertes Sync-Backend.
- Keine Änderungen an den drei Kompaktplayer-Layouts oder deren gemeinsamem
  Controllerzustand.
