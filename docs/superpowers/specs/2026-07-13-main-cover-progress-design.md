# Cover-Download-Fortschritt im Hauptfenster — Design

## Ziel

Ein aktivierter Hintergrundlauf für fehlende Albumcover darf außerhalb der
Plugins-Einstellungen nicht unsichtbar sein. Das Hauptfenster zeigt denselben
Fortschritt wie die Preferences, ohne einen zweiten Zustand, Worker oder
Netzwerkpfad einzuführen.

## Verhalten

- Direkt unter den oberen Leisten erscheint während eines Cover-Laufs eine
  schmale native Fortschrittszeile.
- Während `Running` zeigt sie „Fehlende Albumcover werden geprüft …“, den
  bestimmten Balken sowie geprüft/gesamt, heruntergeladen und nicht verfügbar.
- `Complete`, `Stopped` und `Failed` bleiben kurz als Ergebnis sichtbar und
  werden danach automatisch ausgeblendet. Die Plugins-Seite behält ihren
  bisherigen persistenten Endzustand.
- Ein neuer Lauf ersetzt einen noch wartenden Ausblend-Timer per Generation;
  ein alter Timer darf niemals eine neue Anzeige verstecken.
- Setup, Hauptmenü und Plugins-Schalter verwenden weiterhin dieselbe Action und
  denselben seriellen Batch-Controller.
- Nach einem erfolgreichen Bibliotheksscan startet der Cover-Lauf erneut, falls
  das Plugin aktiviert ist. Damit funktioniert die Default-off-Option aus dem
  First-Run-Dialog auch dann, wenn die Bibliothek beim Aktivieren noch leer war.
- Scan- und Cover-Fortschritt sind getrennte Zeilen und dürfen bei echter
  Überlappung beide sichtbar sein; kein Vorgang verdrängt oder verfälscht den
  Zustand des anderen.

## Architektur

`CoverDownloadBatch` ersetzt seinen einzelnen überschreibenden Callback durch
mehrere Subscriber. Jeder Subscriber besitzt eine nebenwirkungsfreie schwache
Lebensprüfung und einen getrennten Zustands-Callback; nur der neu registrierte
Subscriber erhält sofort den aktuellen immutable `BatchProgress`. Tote schwache
GTK-Subscriber werden beim nächsten Zustand oder bei einer neuen Registrierung
entfernt, ohne dabei lebende Anzeigen erneut einzublenden. Die Einträge werden vor
dem Aufruf aus dem `RefCell` kopiert, sodass keine Borrow-Grenze über re-entrante
GTK-Aufrufe gehalten wird.

Das neue `main_cover_download_progress.rs` besitzt ausschließlich die kompakte
GTK-Darstellung, den Ausblend-Generationstimer und die Composition-Wiring-Funktion.
Sie registriert genau einen Hauptfenster-Subscriber, hängt die Zeile an den
vorhandenen `ToolbarView` und verbindet erfolgreichen Scanabschluss mit
`CoverDownloadBatch::start_if_enabled`. `window.rs` erhält nur diesen einen
Composition-Aufruf und bleibt unter 800 Zeilen.

`preference_cover_download.rs` bleibt die persistente Detailansicht und wird nur
auf die neue Subscriber-Grenze umgestellt. Der Downloadworker, Matching,
Netzwerkzugriff, Cache und Musikdateien bleiben unverändert.

## Sicherheit und Skalierung

Es entsteht kein zusätzlicher Download und keine parallele Verarbeitung. Musik-
dateien werden weiterhin nur gelesen; Cover landen ausschließlich im XDG-Cache.
Der Hauptthread erhält nur kleine `Copy`-Fortschrittswerte. Subscriber halten nur
schwache Widget-Referenzen oder fensterlebenslange View-Handles und bilden keinen
`Rc`-Zyklus.

## Tests

- Reine Batchtests beweisen, dass mehrere Subscriber denselben aktuellen und
  folgenden Zustand erhalten und tote Subscriber entfernt werden.
- Reine Präsentationstests prüfen Idle, Running und terminale Zustände inklusive
  Bruchteil und Auto-Hide-Entscheidung.
- Ein isolierter GTK-Displaytest prüft Revealer, Texte und ProgressBar.
- Ein vollständig isolierter Offline-Smoke verwendet nur kopierte FLAC-Fixtures
  plus lokales Sidecar-Cover, schaltet das Modul über den bestehenden Smoke-Hook
  ein und prüft Hauptfenster-Running/Complete ohne Netzwerk.
- Vollständige Gates, Core-Purity, gettext und Dateigrößen bleiben verpflichtend.

## Explizit nicht Teil

Keine zweite Downloadqueue, kein paralleler Download, kein Abbrechen-Button, keine
ETA, kein Schreiben in Albumordner und keine Änderung am konservativen
MusicBrainz-Matching.
