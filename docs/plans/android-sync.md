# Vereinfachter Android-Sync (MTP) — gepflegter Implementierungsplan

Status: **Stage implementiert; manuelle Geräteprüfung offen**
Branch: `feature/simplified-android-sync`
Feature-Basis: `ea1b3dc7c1`
Integriertes `dev`: `9f9fbd809b`
Stand: 2026-07-27

Dieser Plan ersetzt den früheren Entwurf für Device-View, Preferences-Sync-Tab,
„Entire Library“, Pins, Ratings-Back, Opus und einen globalen Sync. Der
verbindliche Ausführungsstand steht in `.superpowers/sdd/progress.md`; die
Commits bleiben Ground Truth.

## Produktziel

Reprise spiegelt eine explizite Auswahl manueller und smarter Playlists auf
verbundene Android-MTP-Geräte. Die Bedienung bleibt absichtlich klein:

- Eine Gerätekarte in der Sidebar zeigt Zustand, Delta und Fortschritt.
- Ein Klick öffnet einen kompakten gerätebezogenen Dialog.
- Der Hauptmenü-Eintrag öffnet bei einem Gerät direkt diesen Dialog und bei
  mehreren Geräten zuerst eine Geräteauswahl.
- Im Dialog werden Playlists und MP3-Qualität gewählt, Delta und
  Speicherprojektion geprüft sowie Sync, Cancel und Eject ausgelöst.
- Änderungen werden sofort gerätebezogen persistiert; es gibt keinen Apply-
  Schritt und keine zweite Sync-Oberfläche.

Nicht Teil dieser Stage sind:

- gesamte Bibliothek als Sync-Quelle;
- Device-View im Hauptfenster;
- Sync-Seite in Preferences;
- Keep-on-device-Pins und Ratings-/Playcount-Rücksync;
- Opus-Profile oder parallele Encoder;
- Zugriff auf beliebige Telefoninhalte außerhalb des von Reprise verwalteten
  Bereichs;
- Companion-App oder bidirektionale Synchronisation.

## Architektur

### `reprise-core`

Die pure Core-Schicht besitzt die plattformneutralen Verträge:

- `device_sync/profile.rs`: MP3 128/192/256/320 kbit/s, Standard 256 kbit/s,
  Copy-vs.-Transcode-Entscheidung, konservative Zielgröße und stabiler
  Profil-Fingerprint.
- `device_sync/snapshot.rs`: manuelle und smarte Playlist-Snapshots,
  Wiederholungen in M3U-Reihenfolge sowie explizite Verfügbarkeit.
- `device_sync/mirror.rs`: deterministischer Mirror-Plan mit Add, Replace,
  Remove, Playlist-Write und Playlist-Remove; unsichere oder unbekannte
  verwaltete Einträge werden nie gelöscht.
- `device_sync/storage.rs`: aktuelle und projizierte Zusammensetzung aus
  Reprise-Musik, anderer Musik, sonstigem belegtem Platz und freiem Platz,
  jeweils mit vollständigem, unbekanntem oder inkonsistentem Wissensstand.
- `device_sync/page.rs`: toolkit-neutrale Projektion für Dialog und Sidebar.
- `device_sync/settings.rs`: gerätebezogene Auswahl sowie Track- und
  Playlist-Inventare.

`reprise-core` bleibt frei von GTK, libadwaita, GStreamer und zbus.

### `reprise-platform-linux`

- `device_sync.rs` erkennt ausschließlich MTP-Ziele, löst den verwalteten
  `Music/Reprise`-Bereich auf, inspiziert nur diesen Bereich und liefert andere
  Musik ausschließlich aggregiert.
- `device_transfer.rs` kopiert vorhandene passende MP3-Dateien oder
  transkodiert genau eine Datei zur Zeit über
  `uridecodebin → audioconvert → audioresample → lamemp3enc → id3v2mux`.
- Die Pipeline benötigt GStreamer Good Plug-ins. Fehlt eine Factory, wird der
  gesamte Plan vor dem ersten destruktiven Schritt blockiert.
- Der MTP-Transfer schreibt zuerst `<Ziel>.part`, prüft die erwartete Größe und
  veröffentlicht erst danach atomar auf den finalen verwalteten Pfad.
- Partials werden ausschließlich unter `Music/Reprise` bereinigt.

### `reprise-gnome`

- `device_sync_runtime.rs` hält je Gerät State, Generation, Cancel-Token,
  Storage-Snapshot, Inventar, Mirror-Plan und Projektion.
- `device_sync_planned.rs` führt pro Gerät seriell aus; verschiedene Geräte
  dürfen unabhängig parallel laufen.
- `device_sync_dialog.rs` ist die einzige editierbare Sync-Oberfläche.
- `device_sync_launcher.rs` ist der Hauptmenü-Einstieg und die
  Mehrgeräteauswahl.
- `sidebar_device_card.rs` projiziert denselben State in-place, ohne Widgets
  bei Progress-Events neu aufzubauen.
- `device_sync_feedback.rs` besitzt Connected-, Disconnected-, Cancel- und
  Completion-Feedback sowie den Header-Spinner, wenn die Sidebar nicht
  sichtbar ist.

## Persistenz und Migration

Der zusammengeführte Schema-Stand ist `user_version = 36`:

- v34: Podcasts/Radio;
- v35: Recently Added;
- v36: Android-Sync-Inventar.

v36:

- ergänzt `device_settings.mp3_quality` mit 128/192/256/320 und Standard 256;
- normalisiert den alten `"entire_library"`-Wert auf eine leere
  Playlist-Auswahl;
- macht `device_files` mit Source-Pfad, Source-Größe, Mtime, Device-Pfad,
  Device-Größe und Profil-Fingerprint explizit;
- führt `device_playlists` mit stabiler Quelle und eindeutigem Device-Pfad
  ein;
- markiert altes Opus-Inventar mit `legacy-opus-v1`, damit es beim nächsten
  ausgewählten Sync sicher ersetzt wird.

Alte Felder und Enum-Varianten dürfen für DB-/API-Kompatibilität noch
dekodierbar sein, sind aber inert und besitzen keine Benutzeroberfläche.
Inventarzeilen kaskadieren absichtlich nicht mit Bibliothekstracks: Ein lokal
vorübergehend fehlender Track darf keine Information über die vorhandene
Gerätedatei vernichten.

## Deterministischer Mirror-Plan

1. Persistierte manuelle und smarte Playlist-Auswahl laden.
2. Jede Playlist in stabiler Reihenfolge materialisieren.
3. Physische Tracks über alle Playlists deduplizieren; M3U-Wiederholungen
   bleiben erhalten.
4. Zielpfade FAT-sicher und kollisionsstabil unter
   `Music/Reprise/<Album Artist>/<Album>/<NN Title>.mp3` ableiten.
5. Source-Fingerprint, Profil und Inventar vergleichen:
   - unverändert und passend: behalten;
   - neu: kopieren/transkodieren;
   - geändert oder altes Profil: ersetzen;
   - nicht mehr ausgewählt: entfernen;
   - lokal nicht verfügbar, aber auf dem Gerät inventarisiert: behalten;
   - unbekannt/unsicher: warnen und unangetastet lassen.
6. Root-Level-M3U-Snapshots planen und alte verwaltete Playlist-Dateien
   explizit entfernen.
7. Delta, Transferbytes und Speicherzustand projizieren. Blocker enthalten
   keine lokalen oder Gerätepfade.

Für Transcodes reserviert die Projektion den nominellen MP3-Audiostrom, die
vollständige Source-Größe für source-abgeleitete Tags/Cover und zusätzlichen
Container-/Frame-Overhead. Eine unbekannte Dauer hat deshalb keine künstlich
kleine, begrenzte Schätzung.

## Ausführungsreihenfolge und Sicherheitsinvarianten

Vor destruktiver Arbeit werden Geräteidentität, Verbindung, Mirror-Plan,
MP3-Pipeline und aktueller freier Speicher erneut geprüft.

Die Reihenfolge eines gerätebezogenen Laufs ist:

1. verwaiste `.part`-Dateien bereinigen;
2. Tracks nacheinander transkodieren/kopieren;
3. die verifizierte neue Datei inventarisieren, alte abweichende Zielpfade
   aber zunächst behalten;
4. alle neuen Playlist-Snapshots veröffentlichen;
5. nur nach vollständiger Playlist-Veröffentlichung obsolete Playlist-Dateien,
   nicht mehr ausgewählte Tracks und ersetzte alte Zielpfade entfernen;
6. Inhalt und verfügbaren Speicher neu inspizieren;
7. Idle-/Synced-State veröffentlichen.

Weitere Invarianten:

- Nie Dateien außerhalb `Music/Reprise` löschen oder überschreiben.
- Nie die echte Reprise-Datenbank oder Musikbibliothek in Tests verwenden.
- Pro Gerät höchstens eine Dateioperation; geräteübergreifende Parallelität
  ist erlaubt.
- Cancel wirkt nur auf das benannte Gerät und stoppt auch weitere
  Playlist-Veröffentlichungen und -Löschungen.
- Generationen verwerfen verspätete Scan-, Progress- und Completion-Events.
- Während eines Laufs sind Playlist- und Qualitätsänderungen vor der
  Persistenz gesperrt.
- Eine fehlgeschlagene Inventar-Transaktion darf einen alten abweichenden
  Zielpfad nicht löschen.
- Eine fehlgeschlagene oder abgebrochene Playlist-Veröffentlichung darf keine
  Pfade entfernen, auf die der vorherige Snapshot noch verweist.
- Unbekannte Kapazität wird als unbekannt dargestellt, nicht als passend.

## UI-Vertrag

Der Dialog zeigt:

- Gerätename und MTP-Verbindung;
- MP3-Qualität 128/192/256/320 kbit/s;
- jede manuelle und smarte Playlist mit Entry-, Unique-, Missing- und
  Größenprojektion;
- deduplizierte Gesamttracks und physische Zielgröße;
- verständliche Change-, Blocker- und Warning-Zusammenfassungen ohne Pfade;
- eine Storage-Zusammenfassung und Segmentleiste für Music, After-sync-Delta,
  Other und Free;
- laufenden Schritt, Track, Dateifortschritt und geglättete MTP-Kopierrate;
- primäre Aktionen `_Sync now` beziehungsweise `_Cancel` mit Mnemonics;
- Eject nur bei verbundenem, inaktivem Gerät;
- lesbaren Disconnected-Status bei Kabelverlust.

Playlist-Zeilen werden nur neu aufgebaut, wenn sich ihre Quellen ändern.
Dabei wird der aktuelle Fokus auf derselben Quelle oder der nächstgelegenen
verbleibenden Zeile wiederhergestellt. Kein `RefCell`-Borrow bleibt über
GTK-Setter oder Signalpfade bestehen.

Die lokale Agent-/MCP-Schnittstelle verwendet dieselben MP3-Qualitäten. Eine
Konfiguration ändert den expliziten Transfer-Profile-State; das alte
`opus_bitrate`-Kompatibilitätsfeld bleibt dabei null.

## Abschlussstand

Die Tasks 1 bis 11 sowie die Dev-Integration und die beiden adversarialen
Safety-/Storage-Follow-ups sind abgeschlossen. Der Agent-, D-Bus- und
MCP-Vertrag entspricht dem kompakten Dialog:

- `music_get_device_sync_state` liefert manuelle und smarte
  Playlist-Identitäten, MP3-Qualität, deduplizierte Summen, Änderungen,
  Storage-Zusammensetzung, Blocker, Warnungen, Controls und Fortschritt ohne
  Seriennummern oder Pfade.
- `music_device_sync` akzeptiert `configure`, `start` und `cancel`.
  `configure` erhält Quellen als stabile Paare aus `kind`
  (`playlist` oder `smart`) und `id` sowie `quality_kbps` mit
  128/192/256/320. Alle Mutationen benötigen `device:sync`.
- Die kompatiblen alten Opus-Felder bleiben inert und werden von der neuen
  Konfiguration nicht als Produktfunktion reaktiviert.

Die Commits und Gate-Ergebnisse sind im Fortschrittsledger einzeln
nachgewiesen. Offen bleiben ausschließlich die unten aufgeführten Prüfungen
mit einem ausdrücklich freigegebenen Testgerät sowie die Owner-Entscheidung
über den geplanten UX-Regelvorschlag MTP-7.

## Verifikation

Vor jedem Stage-Commit müssen passieren:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'
scripts/check-architecture.sh
scripts/check-accessibility-semantics.sh
scripts/check-input-parity.sh
scripts/check-motion-tokens.sh
scripts/check-ux-traceability.sh
scripts/check-device-sync-gstreamer.sh
```

Die Core-Purity-Ausgabe muss leer sein. Alle wesentlich geänderten Code-Dateien
bleiben unter 800 Zeilen.

## Manuelle Stage-Review-Checks

Diese Checks benötigen ausdrückliche Freigabe und ein Testgerät; sie sind nicht
durch Headless-Tests ersetzbar:

1. reales Android-Gerät verbinden und Connected-Toast/Karte prüfen;
2. 128/192/256/320-kbit/s-MP3-Resultate und eingebettete Tags/Cover prüfen;
3. Copy-Fortschritt und Rate auf dem realen GVfs-MTP-Backend beobachten;
4. Kabelverlust während Copy sowie Reconnect/Partial-Cleanup prüfen;
5. Eject im Idle und Disabled-Zustand während Sync prüfen;
6. zwei reale Geräte unabhängig starten und eines abbrechen;
7. Fokus, Mnemonics, Storage-Segmente, High Contrast und reduzierte
   Animationen visuell prüfen.

Ohne diese Freigabe greift die Implementierung weder auf ein Telefon noch auf
reale Musikdateien oder die reale Datenbank zu.
