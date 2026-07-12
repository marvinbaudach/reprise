# GUI-D: First-run-Wizard und Session-Restore — Design

## Ziel

Ein neuer Nutzer wird beim ersten Start verständlich und sicher zur ersten
Bibliothek geführt. Ein wiederkehrender Nutzer findet Fenster, Navigation,
Filter, Sortierung und Queue wieder, ohne dass Reprise ungefragt Audio startet.

## First-run-Wizard

Der Wizard erscheint nur, wenn `onboarding.completed` nicht gesetzt ist und
keine bestehende `library_root` vorhanden ist. Eine vorhandene Bibliothek gilt
als Upgrade/Bestandsinstallation: Reprise markiert Onboarding still als erledigt
und zeigt keinen nachträglichen Wizard.

Der native `AdwDialog` enthält:

- kurze Welcome-/Privacy-Copy: lokale Bibliothek, Musikdateien werden nur auf
  explizite Tag-/Trash-Aktion verändert;
- optionalen, standardmäßig ausgeschalteten Schalter für Online-Cover;
- optionale, standardmäßig ausgeschaltete Übernahme des Rhythmbox-
  Spaltenlayouts; der Text sagt ausdrücklich „read-only“;
- `Set Up Library` und `Skip for Now`.

`Set Up Library` persistiert die beiden bewussten Optionen, markiert Onboarding
als abgeschlossen, schließt den Wizard und aktiviert denselben bestehenden
`Scan folder…`-Button. Damit gibt es nur einen Portal-/Scan-/Watcher-Pfad.
Abbruch des Portal-Dialogs ist erlaubt; der Nutzer kann jederzeit den normalen
Header-Button verwenden. `Skip for Now` markiert ebenfalls abgeschlossen und
öffnet keinen Dialog. Der Wizard scannt und schreibt niemals selbst Dateien.

## Session-Zustand

Ein versionierter JSON-Wert unter `ui.session.v1` enthält ausschließlich:

- Fensterbreite/-höhe und maximized;
- `ViewSource` als stabilen Typ inklusive Playlist-/Smart-ID;
- Suche, `BrowseFilter` und Sortfeld/-richtung;
- exakten Queue-Zustand: ursprüngliche IDs, aktuelle Shuffle-Order,
  Queue-Position, Shuffle und Repeat.

Nicht gespeichert: laufender/pausierter Playback-Zustand, Lautstärke,
Abspielposition, geöffnete Now-Playing-Navigation, Zeilenselektion,
Scrollposition oder Dialogzustände. Beim Restore bleibt der Player **Stopped**.
Die Queue, Current-Track-Metadaten, Cover und Transportfähigkeit werden sichtbar,
aber Audio beginnt erst nach einer neuen Play-Aktion. Dadurch kann ein
Desktop-Login niemals überraschend Musik abspielen.

## Validierung und Fallback

- JSON trägt `version: 1`; unbekannte Version, kaputtes JSON oder ungültige
  Werte ergeben `SessionState::default()` und eine Warnung, nie Panic.
- Fenstermaße werden auf sinnvolle Mindest-/Maximalwerte begrenzt.
- Suchtext und Queue sind größenbegrenzt; überlange/handeditierte Werte werden
  gekürzt oder verworfen.
- Queue-Snapshot validiert `order` als exakte Permutation und `position` gegen
  die Länge. Nicht mehr vorhandene Track-IDs werden beim Frontend-Restore über
  den bestehenden Queue-Remove-Pfad entfernt.
- Verschwundene Playlist-/Smart-ID fällt über Sidebars bestehende
  `resolve_select_source`-Logik auf Library zurück.
- Unbekanntes Sortfeld fällt auf Title/ascending zurück. Browse wird nur in
  Library angewandt.
- Save-Fehler beim Schließen werden geloggt; das Fenster schließt trotzdem.

## Architektur

### Core

`queue::QueueSnapshot` und `Queue::{snapshot,restore_snapshot}` besitzen die
Queue-Invarianten. `library::session` definiert die serde-Typen, Versionierung,
Grenzen und `load/save` über den generischen Settings-Store. Keine GTK-
Abhängigkeit gelangt in Core.

### Frontend

`ui/session_restore.rs` ist der Orchestrator für GTK-Geometrie, TrackList,
Sidebar und PlayerController. Die einzelnen Widgets bieten kleine plain-data-
Snapshot/Restore-Seams; das Modul greift nicht über private GTK-Child-Suchen zu.

`ui/first_run.rs` baut den Wizard. Es ruft für Cover und Rhythmbox dieselben
Window-Actions aus `primary_menu.rs` auf und für den Ordner exakt den
bestehenden Scan-Button. So können Wizard und normales UI nie unterschiedliche
Persistenz- oder Fehlerpfade entwickeln.

`window.rs` bleibt unter 800 Zeilen: Aufbau/Wiring wandert bei Bedarf in die
beiden neuen Geschwistermodule; die Composition Root enthält nur Aufrufe.

## Verifikation

- Core-TDD: Queue-Snapshot exakt, ungültige Permutation abgelehnt,
  Session-JSON roundtrip, Version/korrupt/Bounds-Fallback.
- Frontend-TDD: ViewSource-/Sort-Fallback, Onboarding-Entscheidung und
  No-autoplay-Restore-Entscheidung als pure Helper.
- Isolierter First-run-Smoke: frische Scratch-DB zeigt Wizard-Hook, setzt
  completed, optional Cover/Rhythmbox über echte Actions; keine echte dconf-
  oder Musikdatei.
- Isolierter Zwei-Start-Smoke mit gemeinsamem temporären XDG_DATA_HOME:
  erster Start speichert Fenster/View/Search/Browse/Queue, zweiter Start loggt
  dieselben Werte und `playback=Stopped`; keine Audioausgabe (`fakesink`).
- Manuell: Wizard-Copy/Layout, Portal-Folderpicker, reale Fenstergeometrie,
  GNOME-Maximize, Restore nach normalem Schließen und kein Autoplay.

## Explizit nicht Teil dieser Etappe

- Resume an alter Abspielposition oder automatisches Weiterplaying.
- Persistenz von Pointer-Selektion, Scrollposition oder offenen Dialogen.
- Cloud-/Account-Onboarding, Telemetrie oder Netzwerkzugriff ohne Cover-Opt-in.
- Schreiben in Rhythmbox-GSettings.
- Mehrere parallele Sessions/Fenster.

## Nicht verhandelbare Regeln

Core bleibt frei von GTK/libadwaita/GStreamer/zbus. Alle Tests/Smokes nutzen
Scratch-DB, Scratch-Cache, eigenen D-Bus/X11/fakesink und niemals reale Musik.
Alle UI-Texte/Logs/Code-Kommentare sind Englisch. Kein Restore startet Audio.
Jede bearbeitete Datei bleibt unter 800 Zeilen.
