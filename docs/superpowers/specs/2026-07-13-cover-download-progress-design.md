# Sichtbarer Cover-Download — Design

## Ziel

Das Aktivieren von „Fehlende Albumcover herunterladen“ muss eine sofort sichtbare
und verständliche Reaktion auslösen. Reprise startet einen Hintergrundlauf über die
Bibliothek und zeigt Fortschritt sowie Ergebnis an, statt nur still ein Flag zu
setzen.

## Verhalten

- Beim Einschalten beginnt sofort ein serieller Hintergrundlauf.
- Die Plugins-Seite zeigt einen Fortschrittsbalken, „geprüft/gesamt“ und die Zahl
  neu heruntergeladener Cover.
- Vorhandene eingebettete, Ordner- oder bereits heruntergeladene Cover werden nur
  geprüft und nicht erneut geladen.
- Mehrere Titel desselben Albums verursachen höchstens einen Netzabruf.
- Nach neuen Downloads werden Trackliste, Playerleiste und Now-Playing-Cover ohne
  Wiedergabeunterbrechung aktualisiert.
- Ausschalten stoppt das Einreihen weiterer Abrufe. Ein bereits laufender HTTP-
  Request darf seinen begrenzten Timeout beenden; danach lautet der Status
  „gestoppt“. Bereits gespeicherte Cache-Cover bleiben erhalten.
- Ein erneutes Einschalten startet einen neuen Lauf. Der vorhandene positive und
  negative Cache verhindert unnötige Wiederholungen.
- Ist das Plugin beim App-Start bereits aktiviert, startet der Lauf automatisch.

## Architektur

`cover_download_batch.rs` besitzt Zustand, Generation zur Abbruchkontrolle,
Bibliothekspfad-Abfrage und die GTK-Main-Loop-Orchestrierung. Es sendet weiterhin
an genau den vorhandenen seriellen Worker; es entsteht kein zweiter Netzwerkpfad.

Der Worker liefert ein typisiertes Ergebnis: bereits vorhanden, heruntergeladen
oder nicht verfügbar. Lazy-Downloads einzelner sichtbarer Zellen verwenden
denselben Ergebnistyp. Der Batch-Controller publiziert immutable Fortschrittswerte
an eine schwache UI-Callback-Grenze. `preference_cover_download.rs` baut ausschließlich
die native Statuszeile und den Fortschrittsbalken.

Der zustandsbehaftete Fenster-Action-Schalter bleibt die einzige Stelle für
Persistenz und Runtime-Flag. Nach erfolgreicher Zustandsübernahme benachrichtigt er
den Batch-Controller; damit verhalten sich Hauptmenü und Preferences identisch.

## Sicherheit und Fehler

Keine Musikdatei wird geschrieben. Downloads landen weiterhin ausschließlich im
XDG-Cache. Tag-/Cover-Prüfung und Netzwerk laufen im Worker, nie auf dem GTK-Main-
Thread. Zugang zum UI erfolgt ohne gehaltenen `RefCell`-/SQLite-Borrow.

Netzfehler blockieren den Player nicht und werden als „nicht verfügbar“ gezählt.
Ein DB-Abfragefehler zeigt einen abgeschlossenen Fehlerstatus. Der Fortschritt darf
nie über 100 Prozent steigen und veraltete Läufe dürfen durch Generationen keine
neue UI überschreiben.

## Tests

- Reine Tests für Fortschrittsübergänge, Bruchteile und Stopzustand.
- DB-Test für aktive, nicht fehlende Trackpfade.
- Worker-Tests für vorhandene Cover und Album-Deduplizierung ohne Netz.
- GTK-Konstruktionstest für Statuszeile/Progressbar.
- Vollständige Gates und ein vollständig isolierter Offline-Smoke ohne reale Musik,
  reale Datenbank oder echten Netzwerkabruf.

## Explizit nicht Teil

Kein paralleler Massendownload, kein Schreiben von Coverdateien in Albumordner,
kein Löschen des Covercaches, keine manuelle Match-Auswahl und keine Änderung des
konservativen MusicBrainz-Matchings.
