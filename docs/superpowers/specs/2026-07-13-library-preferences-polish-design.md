# Bibliotheksansicht und Einstellungen — visuelle Konsolidierung

## Ziel

Die Bibliotheksansicht und die Einstellungen werden auf den bereits festgelegten
Reprise-Look aus dem Master-Design und dem GTK4-Mockup 7a–7c zurückgeführt. Die
Etappe ergänzt keine neuen Musikfunktionen. Sie ordnet vorhandene Bedienwege,
nutzt den verfügbaren Platz besser und macht die bereits implementierten
Einstellungen als zusammengehörige native Adwaita-Oberfläche verständlich.

Die Bibliothek bleibt das dichte Arbeitsfenster für große Sammlungen. Die
Einstellungen bleiben ein fokussierter Dialog, in dem jede sichtbare Option
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

## Bibliotheksansicht

### Fensteraufbau

Die bestehende `AdwNavigationSplitView`-, `AdwToolbarView`- und
`GtkColumnView`-Architektur bleibt erhalten. Die Etappe ändert weder Quellen,
Queries, Queue noch Playbackzustand.

```text
┌ Sidebar ───────┬ Header: Suche        Musik        [Kompakt] [Scan] [Menü] ┐
│ BIBLIOTHEK     ├ Filter:  Genre       Interpret       Album                │
│  Musik     504 │ Titel      Interpret      Album        Jahr  Länge  ★     │
│  Queue      12 │ …                                                       │
│ PLAYLISTEN     │ …                                                       │
│  Training  48  │ …                                                       │
│ INTELLIGENT    ├──────────────────────── 504 Titel · 1 Tag, 8 Std. ────────┤
└────────────────┴ Playerleiste: Cover · Metadaten · Transport · Lautstärke ┘
```

### Headerbar

- Links steht die bestehende Suche. Sie erhält eine begrenzte natürliche
  Breite, expandiert nicht über den Quelltitel und bleibt über `Ctrl+F`
  fokussierbar.
- Der aktuelle Quelltitel bleibt zentriert und ist der einzige hervorgehobene
  Text in der Headerbar.
- Rechts folgen der im Kompaktplayer-Plan definierte direkt sichtbare
  Kompaktknopf, ein icon-only Scan-Knopf mit Tooltip und das Hauptmenü.
- „Playlist importieren…“ liegt im Hauptmenü. Es bleibt eine globale Aktion,
  wird aber nicht dauerhaft als großer Textknopf gezeigt.
- Der Scan-Knopf bleibt sichtbar, weil Bibliothekspflege eine häufige und bei
  leerer Bibliothek primäre Handlung ist. Während eines Scans verwendet er
  weiterhin den einen vorhandenen `ScanControls`-Zustand.
- Im kollabierten Split-View kommt der Sidebar-Knopf ganz links hinzu. Alle
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

### Dialog und Navigation

Der Dialog bleibt modal zur Library und verwendet ausschließlich native
Adwaita-Widgets. Sein Standardinhalt ist ungefähr 760 × 680 logische Pixel,
mit einem sicheren kleineren Minimum und vertikal scrollenden Seiten.

- Reihenfolge: **Wiedergabe · Darstellung · Layout · Bibliothek · Plugins**.
- Ab ungefähr 720 Pixel Breite sitzt ein textlicher `AdwViewSwitcher` in der
  Headerbar wie im Mockup 7b/7c. Die aktive Seite besitzt zusätzlich den
  nativen Auswahlhintergrund.
- Unterhalb dieser Breite verwendet der Dialog die native kompakte
  Icon-/Text-Navigation am unteren Rand. Der Seiten-Scrollbereich endet oberhalb
  dieser Leiste; Navigation darf niemals Equalizer oder Aktionszeilen
  überdecken.
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
- Gruppe „Bibliotheksfenster“ enthält Schalter für Sidebar, Browse-Leiste und
  Statuszeile sowie die Dichteauswahl. Die Begriffe beschreiben sichtbar genau
  die Elemente des Hauptfensters.
- Gruppe „Spalten“ zeigt die aktuell sichtbaren Spalten in der Subtitle-Zeile
  und öffnet denselben vorhandenen Spalteneditor. Keine zweite
  Spaltenkonfiguration wird aufgebaut.
- Gruppe „Kompaktansicht“ wird erst nach Abschluss der parallelen
  Kompaktplayer-Etappe gegen deren finalen `CompactLayout`-Vertrag eingebunden.
  Diese Etappe dupliziert weder Layoutkarten noch Persistenzactions des
  Kompaktplayer-Plans.

### Bibliothek

- Gruppe „Musikordner“ zeigt den aktuellen Pfad als Subtitle. „Ordner wählen…“
  bleibt die eine hervorgehobene Aktion der Zeile.
- „Bibliothek neu scannen“ liegt direkt darunter und verwendet den gemeinsamen
  Scanfortschritt. Eine laufende Operation ist in Dialog und Hauptfenster
  derselbe Zustand, kein zweiter Worker.
- Gruppe „Import“ enthält „Rhythmbox-Spaltenlayout importieren“. Die Zeile
  erklärt ausdrücklich, dass Reprise nur liest und Rhythmbox nicht verändert.
- Coverdownload bleibt unter Plugins, weil er eine optionale
  Netzwerk-Integration ist. Ordnerliste, mehrere Bibliothekswurzeln und
  Dateiverwaltung werden nicht vorgetäuscht.

### Plugins

- Eine einleitende Beschreibung erklärt, dass Plugins optionale Integrationen
  sind und feste Playbackfunktionen nicht hier erscheinen.
- Jede Integration bleibt eine `AdwSwitchRow` mit Name, kurzer Wirkung und
  gegebenenfalls Neustarthinweis. MPRIS und Coverdownload behalten ihre
  bestehenden Defaultwerte und Lebenszyklen.
- Laufender Coverdownload-Fortschritt steht direkt unter der zugehörigen Zeile
  und bleibt beim Seitenwechsel erhalten. Es entsteht kein zweiter Subscriber,
  solange der vorhandene schwache Subscriber ausreicht.

## Architektur und Dateigrenzen

- `window.rs` bleibt Composition Root und wird nicht über 800 Zeilen erweitert.
  Headeraufbau und Library-Layoutverkabelung werden in ein fokussiertes
  Geschwistermodul extrahiert.
- `track_list_columns.rs` bleibt Eigentümer der Spaltenwidgets;
  Fill-/Ellipsierungsregeln landen dort, nicht im Composition Root.
- `browse_bar.rs` bleibt Eigentümer der kaskadierenden Filter. Eine kleine pure
  Breakpoint-/Layoutentscheidung wird getrennt testbar gehalten.
- `preferences.rs` wird in Dialog-Shell, Appearance/Layout und Playback
  aufgeteilt. `preference_library.rs`, `preference_effects.rs` und
  `preference_cover_download.rs` bleiben fokussierte Seitenhelfer.
- Der neue Browse-Sichtbarkeitswert lebt typisiert in
  `reprise_core::library::settings`. Core erhält keine GTK-, Adwaita-,
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
- Der Dialog hält Callbacks schwach oder klont sie vor dem Aufruf aus
  `RefCell`s. Seitenwechsel und Dialogschließen erzeugen keine Referenzzyklen.

## Tests und Verifikation

- Reine Tests prüfen Browse-Sichtbarkeits-Fallback/Roundtrip, adaptive
  Browse-Anordnung, Headeraktionsreihenfolge, Spalten-Fillregeln,
  Preferences-Seitenreihenfolge und Auswahl-Rollback.
- Displaytests prüfen Library bei 1.440, 900 und 640 Pixel Breite: keine
  ungenutzte rechte Tabellenfläche, kein abgeschnittener Header, korrekter
  Browse-Umbruch und erreichbare Sidebar.
- Je ein Displaytest prüft breite und schmale Preferences-Navigation, sichere
  Content-Unterkante, die beiden Playerleisten-Vorschauen, alle zehn
  Equalizer-Skalen und Accessible Names.
- Der vorhandene PTR-Harness nimmt Library sowie alle fünf Preferences-Seiten
  auf und bedient Scan, Layout, Spalteneditor und Plugin-Schalter über echte
  Pointer-/Tastaturpfade.
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
- Kein frei gestaltbares Theme, keine benutzerdefinierten Farben, keine
  Transparenz, kein Blur und keine schwebende Playerleiste.
- Keine neue Queue-, Sortier-, Browse-, Such-, Tag-, Datei- oder
  Playbacksemantik.
- Keine mehreren Bibliotheksordner, keine Dateiverwaltung und kein Schreiben
  fremder GSettings.
- Keine Fremd-Plugin-Installation und keine Geräte-Synchronisationsseite ohne
  implementiertes Sync-Backend.
- Keine Änderungen an den vier Kompaktplayer-Layouts oder deren gemeinsamem
  Controllerzustand.
