# Android-Gerätesynchronisation — Designspezifikation

## Ziel

Reprise erhält in den Einstellungen eine feste Seite **Synchronization** für
per USB angeschlossene Android-Geräte. Ein von GNOME/GVfs als MTP-Mount
bereitgestelltes Gerät erscheint als native Gerätekarte mit Systemicon, Name,
Verbindungszustand und — sofern verfügbar — freiem/gesamtem Speicher. Ein Klick
öffnet die auf dem Gerät erkannte Musik und die von Reprise verwalteten
Handy-Playlists.

Tracks aus der Reprise-Bibliothek können per Drag-and-drop in eine
Handy-Playlist gelegt werden. Reprise kopiert sie in einen eigenen, klar
begrenzten Bereich auf dem Gerät und ergänzt eine portable `.m3u8`-Datei. Der
Kopiervorgang ist jederzeit sichtbar. Mehrere Vorgänge für dasselbe Gerät
laufen strikt sequenziell in Einfügereihenfolge.

## Festgelegtes Produktverhalten

- Android wird über die vorhandene GNOME-GIO/GVfs-MTP-Schicht erkannt. Reprise
  greift nicht direkt mit `libmtp` oder rohen USB-Dateideskriptoren zu.
- Die Einstellungen erhalten den Tab **Synchronization** zwischen Library und
  Plugins.
- Nur gemountete `mtp://`-Geräte sind in dieser ersten Version synchronisierbar.
  Ein noch nicht entsperrtes oder nicht im Dateiübertragungsmodus befindliches
  Telefon erhält eine klare Hilfestellung statt eines stillen Fehlers.
- Das Gerätebild ist das vom System gelieferte `GIcon`; es gibt keinen
  Netzwerkabruf eines produktspezifischen Telefonfotos.
- Der verwaltete Zielbereich ist
  `Music/Reprise/<Playlist>/`. Die Playlist liegt als
  `Music/Reprise/<Playlist>.m3u8` daneben und enthält relative UTF-8-Pfade.
- Eine neue Handy-Playlist wird über eine Plus-Aktion mit Namensdialog
  angelegt. Ordner und `.m3u8` werden erst beim ersten erfolgreichen Drop
  geschrieben; ein leerer UI-Eintrag allein verändert das Gerät nicht.
- Bestehende `.m3u8`-Einträge werden in Reihenfolge erhalten. Erfolgreich
  kopierte, noch nicht referenzierte Pfade werden angehängt. Mehrfaches Droppen
  desselben Reprise-Tracks erzeugt keinen doppelten Eintrag.
- Dateinamen sind deterministisch `<track-id>-<bereinigter-originalname>`.
  Dadurch kollidieren gleichnamige Titel nicht. Playlistnamen und Dateinamen
  bewahren Unicode, entfernen aber Trennzeichen, Steuerzeichen, `.`/`..` und
  andere Traversal-Möglichkeiten.
- Reprise schreibt ausschließlich in `Music/Reprise`. Es löscht weder
  fremde Musik noch fremde Playlists, spiegelt nicht und entfernt auch beim
  Aktualisieren einer Playlist keine vorhandenen Gerätedateien.

## Fortschritt und Warteschlange

Fortschritt ist Teil des fachlichen Vertrags, kein rein visueller Zusatz.

- Pro stabiler Geräte-ID existiert genau eine FIFO-Warteschlange.
- Innerhalb eines Geräts läuft höchstens ein Auftrag und innerhalb eines
  Auftrags höchstens eine Dateikopie gleichzeitig.
- Unterschiedliche Geräte dürfen unabhängig voneinander arbeiten.
- Ein Drop wird atomar als Auftrag mit Zielplaylist und validierter Trackliste
  eingereiht. Ein späterer Drop auf dasselbe Gerät hängt sich sichtbar hinten
  an und kann den laufenden Auftrag nicht überholen.
- Die Gerätekarte und Detailansicht zeigen:
  - Zustand: Idle, Preparing, Copying, Paused/Disconnected, Cancelling,
    Complete oder Failed;
  - aktuellen Dateinamen;
  - Fortschritt der aktuellen Datei in Bytes und Prozent, soweit die Größe
    bekannt ist;
  - Gesamtfortschritt in Bytes und `x von y Titeln`;
  - Anzahl wartender Aufträge;
  - übersprungene, erfolgreiche und fehlgeschlagene Titel.
- Der Runtime lebt auf Anwendungsebene. Schließen der Einstellungen bricht
  nichts ab; beim erneuten Öffnen wird der aktuelle Snapshot sofort projiziert.
- **Cancel current transfer** bricht nur den aktiven Auftrag ab. Bereits
  vollständig kopierte Dateien bleiben bestehen, eine unvollständige
  temporäre Zieldatei wird bestmöglich entfernt, wartende Aufträge bleiben
  erhalten und der nächste startet danach.
- Wird ein Gerät getrennt, stoppt die laufende I/O sofort über `GCancellable`.
  Die Queue bleibt im Arbeitsspeicher als Paused sichtbar. Bei Wiedererkennung
  derselben stabilen Geräte-ID beginnt der nicht abgeschlossene Titel erneut;
  vollständig abgeschlossene Titel werden nicht nochmals kopiert.
- Fehlt eine stabile UUID, kann Reprise das Gerät innerhalb der aktuellen
  Verbindung benutzen, nimmt aber nach Wiederanstecken aus Sicherheitsgründen
  keine automatische Zuordnung vor. Die Queue wird dann als fehlgeschlagen
  mit verständlichem Hinweis beendet.
- Warteschlangen werden in Version 1 nicht über einen App-Neustart persistiert.
  Beim Beenden wird aktive GIO-Arbeit abgebrochen.

## Konflikt- und Fehlerverhalten

- Vor dem Einreihen werden alle Track-IDs erneut gegen die Datenbank aufgelöst.
  Fehlende oder als missing markierte Tracks gelangen nicht in einen Auftrag.
- Existiert die deterministische Zieldatei bereits mit derselben bekannten
  Größe, wird sie als bereits vorhanden übersprungen und in die Playlist
  aufgenommen.
- Bei abweichender Größe schreibt Reprise zunächst eine verwaltete
  `.reprise-part`-Datei und ersetzt erst nach vollständiger Kopie die
  deterministische Zieldatei. Fremde Namen werden nie überschrieben.
- Kann freier Speicher zuverlässig gelesen werden und reicht er nicht, startet
  der Auftrag nicht. Ist die Angabe unbekannt, darf der Kopierversuch beginnen
  und ein Backendfehler wird titelgenau gemeldet.
- Ein fehlerhafter Titel beendet nicht die gesamte Queue. Er wird gezählt und
  der Auftrag fährt mit dem nächsten Titel fort. Ein nicht mehr erreichbares
  Gerät pausiert dagegen den Auftrag.
- Die `.m3u8` wird nach den Dateikopien über eine temporäre Datei ersetzt. Ein
  Fehler dabei lässt kopierte Audiodateien bestehen und markiert den Auftrag
  ehrlich als fehlgeschlagen.

## Architektur und Grenzen

### `reprise-core`

Ein neues reines Modul `device_sync` enthält:

- sichere Namens- und relative Zielpfadprojektion;
- `SyncTrack`, `SyncJob`, `DeviceQueue`, `SyncSnapshot` und Zustandsübergänge;
- FIFO-, Fortschritts-, Abbruch-, Pause- und Resume-Regeln;
- das Zusammenführen bestehender und neuer relativer M3U8-Einträge.

Das Core-Modul kennt keine GTK-, GIO-, GVfs-, MTP- oder Linux-Typen und führt
keine Dateisystemoperationen aus.

### `reprise-platform-linux`

Ein neues Modul `device_sync` kapselt GIO:

- `GVolumeMonitor` und Mount-Signale;
- Projektion gemounteter `mtp://`-Roots in stabile Gerätebeschreibungen;
- asynchrones Auflisten, Speicherabfragen, Verzeichnisanlage, Kopieren mit
  Fortschrittscallback, Lesen/Ersetzen der M3U8 und bestmögliches Entfernen
  eigener temporärer Dateien.

Der Adapter akzeptiert ausschließlich bereits validierte relative Pfade unter
dem übergebenen Reprise-Root. Er kennt keine Datenbank und keine Widgets.

### `reprise-gnome`

`DeviceSyncRuntime` verbindet Datenbank, Core-Queue und Plattformadapter. Er
lebt so lange wie das Hauptfenster, verwaltet einen Worker je Gerät, gibt
immutable Snapshots an Beobachter aus und hält keine `RefCell`-Ausleihe über
GTK-, GIO- oder Callbackaufrufe.

`preference_sync` baut die Geräteübersicht und Detailansicht. Es verwendet den
bestehenden String-DnD-Payload des Track-Lists, löst daraus nur Track-IDs und
ruft den Runtime auf. Die UI führt selbst keine Dateioperationen aus.

Synchronisationsstrings leben wegen der 800-Zeilen-Grenze in
`device_sync_strings.rs` und werden als eigene gettext-Quelle registriert.

## Geräteinhalt

Beim Öffnen eines Geräts wird `Music` rekursiv über GIO durchsucht. Angezeigt
werden bekannte Audio-Endungen (`mp3`, `flac`, `ogg`, `opus`, `m4a`, `aac`,
`wav`) mit Dateiname, relativem Pfad und Größe. MTP-Dateien werden in Version 1
nicht vollständig in einen lokalen Cache kopiert, nur um Tags auszulesen;
deshalb basiert die erste Geräteansicht auf sicheren Dateiinformationen und
Ordnerstruktur. Reprise-eigene `.m3u8`-Dateien werden gelesen und als
Drop-Ziele dargestellt.

Ein Scan zeigt eigenen indeterminierten/bestimmten Fortschritt und kann durch
erneutes Öffnen nicht parallel dupliziert werden. Mount-Entfernung verwirft
stale Ergebnisse über eine Generation.

## Flatpak

Die Manifestberechtigungen werden auf die dokumentierten GVfs-Rechte für
GTK/GIO-Anwendungen begrenzt:

- `--talk-name=org.gtk.vfs.*`
- `--filesystem=xdg-run/gvfsd`

Es gibt kein `--device=all`, kein pauschales Host-Dateisystem und keinen
direkten USB-Zugriff. Die Release-Dokumentation erklärt, dass der Host einen
GVfs-MTP-Backend und ein vom Benutzer entsperrtes Telefon im
Dateiübertragungsmodus benötigt.

## Tests

- Reine Core-Tests für Namen, Traversalschutz, relative Pfade, M3U8-Merge,
  FIFO-Reihenfolge, sequenzielle Einzelaktivität, Fortschrittsmonotonie,
  Abbruch, Disconnect und Resume.
- Plattformtests mit lokalen `gio::File`-Roots als kontrolliertem Fake-Backend
  für Verzeichnisanlage, sequenzielles Kopieren, Fortschritt, Skip/Replace,
  M3U8-Schreiben und Cancel-Cleanup. Kein echtes USB-Gerät.
- Pure Mount-Projektionstests für `mtp` gegenüber fremden URI-Schemata.
- GTK-Displaytest für sechsten Preferences-Tab, Gerätekarte, Detailansicht,
  Drop-Ziel, Queue-/Fortschrittsprojektion und Wiederöffnung.
- Isolierter App-Smoke mit einem ausschließlich temporären lokalen
  Geräte-Fixture; keine echte Session, Nutzerdaten oder Musik.
- Vollständige Workspace-Gates, gettext/Release-Check, Core-Purity und
  Dateigrößen.

## Explizit nicht tun

- kein bidirektionales oder automatisches Spiegeln;
- kein Löschen von Handydateien oder Entfernen alter Playlistdateien;
- kein Android-MediaStore-, ADB-, Hersteller- oder App-spezifisches API;
- kein direkter `libmtp`-/USB-Zugriff;
- kein Transcoding oder Format-/Codec-Kompatibilitätsprofil;
- keine WLAN-Synchronisation, iPod-Unterstützung oder Queue-Persistenz;
- kein Tag-Download vollständiger MTP-Dateien für die Geräteansicht;
- kein Zugriff auf ein reales Telefon in automatischen Tests.

## Manuelle Prüfung

Ein echter GNOME-/Wayland-Test mit einem ausdrücklich bereitgestellten Android-
Gerät bleibt manuell: Entsperren/Dateiübertragungsmodus, Geräteicon, reales
MTP-Tempo, Abziehen/Wiederanstecken, Speicherangabe sowie Erkennung der `.m3u8`
durch mindestens eine Android-Musik-App. Automatisierte Tests behaupten diese
Hardwareeigenschaften nicht.

