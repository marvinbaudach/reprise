# Einmaliger Rhythmbox-Import bei der Ersteinrichtung

## Ziel

Der read-only Rhythmbox-Import erscheint ausschließlich während der ersten
Einrichtung. Reprise bietet ihn nur an, wenn das bekannte Rhythmbox-GSettings-
Schema samt `visible-columns`-Schlüssel erkannt wurde. Nach Abschluss oder
Überspringen des Onboardings gibt es weder im Hauptmenü noch in den Einstellungen
einen erneuten Import-Einstieg.

## Umfang

- Der bestehende First-run-Dialog bleibt das einzige Importfenster.
- Bei erfolgreicher Erkennung zeigt er einen klar benannten Bereich
  `Import from Rhythmbox` mit einer explizit auswählbaren Option `Column layout`.
- Die Option bleibt standardmäßig ausgeschaltet. Erst die bewusste Auswahl und
  der Abschluss der Einrichtung aktivieren den vorhandenen read-only Importpfad.
- Ohne erfolgreiche Erkennung fehlt der gesamte Rhythmbox-Bereich.
- `Skip for Now` und ein abgeschlossenes Onboarding verhindern jede spätere
  automatische oder sichtbare Importaufforderung.
- Der interne Window-Action-Pfad bleibt für First-run und isolierte Tests bestehen,
  wird aber nicht mehr in Hauptmenü oder Preferences veröffentlicht.

## Architektur und Datenfluss

`first_run.rs` besitzt die Angebotsentscheidung und baut den Auswahlbereich im
vorhandenen modalen `AdwDialog`. Eine reine Policy-Funktion kombiniert
`FirstRunDecision` und die erkannte Rhythmbox-Verfügbarkeit, sodass Tests beweisen,
dass nur ein frischer First Run ein Angebot erzeugt.

`primary_menu.rs` behält `win.import-rhythmbox-columns` als internen, nicht im
Menü modellierten Ausführungspfad. Dadurch verwendet das Onboarding weiterhin
exakt denselben Import-, Persistenz- und Fehlerpfad. `preference_library.rs`
entfernt die bisher dauerhaft sichtbare Importzeile.

Die Erkennung und der Import lesen weiterhin nur Rhythmbox-GSettings. Reprise
schreibt nie nach Rhythmbox und liest im Test keine echten Benutzerdaten.

## Fehlerbehandlung

- Fehlendes Schema, fehlender Schlüssel oder Lesefehler bedeuten: kein Angebot.
- Ein Importfehler verwendet den bestehenden Toast und lässt das Onboarding
  trotzdem sicher abschließbar.
- Ein Persistenzfehler des Reprise-Spaltenlayouts verändert Rhythmbox nicht.

## Tests und QA

- Reine Tests beweisen First-run-only, Erkennungs-Gating, Default-off und dass das
  persistente Hauptmenü keine Rhythmbox-Action mehr modelliert.
- Ein isolierter GTK-Test prüft den erkannten Auswahlbereich mit genau der
  unterstützten Option `Column layout`.
- Der bestehende First-run-Smoke beweist explizite Auswahl und Import mit einer
  Scratch-Fixture; ein zweiter Start zeigt kein Onboarding und keinen Importdialog.
- Vollständige Gates, Core-Purity und die 800-Zeilen-Regel bleiben verpflichtend.

## Explizit nicht Teil dieser Änderung

- Import von Playlists, Bewertungen, Wiedergabezählern oder RhythmDB-Metadaten.
- Zugriff auf die echte Rhythmbox-Datenbank oder echte Musikdateien in QA.
- Ein später erneut aufrufbarer Migrationseintrag.
- Schreiben oder Zurücksetzen von Rhythmbox-GSettings.
