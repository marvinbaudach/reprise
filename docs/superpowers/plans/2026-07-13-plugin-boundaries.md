# Plugin-Grenzen — Implementierungsplan

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI, deutsche interne Dokumentation; keine realen
Nutzerdaten; Core-Reinheit; jede bearbeitete Datei unter 800 Zeilen; vollständige
Gates vor dem Commit; niemals pushen.

## Aufgabe 1 — Registry und Preferences korrigieren

Tests zuerst so ändern, dass `ALL_MODULES` keine Wiedergabe-Kernfunktionen enthält
und nur der Online-Coverabruf als live umschaltbares Plugin gilt. Die Tests rot
sehen. Danach Equalizer-/ReplayGain-Deskriptoren sowie ihre doppelten Plugin-Zeilen
und Synchronisationslogik entfernen. Die Playback-Seite und die Audio-Pipeline
bleiben unverändert. Veraltete Übersetzungsquellen entfernen.

Commit: `fix: keep core playback features out of plugins`.

## Aufgabe 2 — Architektur und QA nachführen

Master-Design, aktuelle Preferences-Spezifikation, Release-Anleitung und manuelle
QA auf die neue Grenze korrigieren. MTP/iPod fest unter Synchronisation verorten
und den späteren Plugin-Backlog dokumentieren. Vollständige Gates und Core-Purity
ausführen, Ledger/STATUS aktualisieren und Lock freigeben.

Commit: `docs: define plugin and core feature boundaries`.

## Folgeetappen — verbindlicher Implementierungs-Backlog

Die folgenden Punkte gehören ausdrücklich in die weitere Implementierungsplanung,
werden aber nicht als Teil der bereits abgeschlossenen Grenzkorrektur ausgegeben.
Vor jeder Gruppe entstehen eine eigene deutsche Design-Spezifikation und ein
ausführbarer TDD-Plan mit Datenschutz-, Offline-, Fehler- und UI-Verhalten.

### Kernfunktionen außerhalb des Pluginsystems

1. **Wiedergabe:** Equalizer und ReplayGain bleiben feste Funktionen auf der
   Wiedergabeseite. Es entstehen keine Plugin-Deskriptoren oder doppelten Schalter.
2. **Synchronisation:** MTP-, iPod- und später WLAN-Geräte-Support werden als feste
   Synchronisationsetappe geplant. Geräteadapter teilen ein gemeinsames Sync-Modell,
   erscheinen aber nie in der Pluginliste.

### Plugin-Etappe P1 — Laufzeit und lokale optionale Funktionen

1. Einen sicheren Plugin-Lebenszyklus mit Start/Stop, Status, Fehlerisolation,
   Berechtigungsanzeige und moduleigener Konfiguration entwerfen. Keine beliebigen
   nativen Bibliotheken oder ungeprüften Fremd-Binaries zur Laufzeit laden.
2. **MPRIS** vom aktuell neustartpflichtigen Schalter auf sicheres Live-Start/Stop
   umstellen.
3. **Native Desktop-Notifications** optional schaltbar machen, ohne die eigentliche
   Wiedergabe zu beeinflussen.
4. **Einschlaf-Timer** mit Stop/Pause am Fristende und optionalem Ende-des-Titels-
   Verhalten umsetzen.
5. **File-Writer für Streamer** mit atomarem Schreiben eines begrenzten,
   dokumentierten Now-Playing-Formats umsetzen.
6. **Lyrics** aus eingebetteten Tags und lokalen `.lrc`-Dateien bereitstellen; keine
   Netzwerkfreigabe ohne aktivierte Onlinequelle.

### Plugin-Etappe P2 — Konten, Präsenz und synchronisierte Inhalte

1. **Scrobbling** über ein gemeinsames Backend für ListenBrainz, Last.fm und
   Libre.fm mit expliziter Anmeldung, lokaler Offline-Warteschlange und Widerruf.
2. **Discord Rich Presence** mit eigenem Datenschutzhinweis und standardmäßig
   deaktivierter Übertragung des aktuellen Titels.
3. **Synchronisierte Lyrics** mit Zeitstempeln, lokalem Cache, klarer Quellenangabe
   und separat aktivierbarer Onlinequelle.

### Plugin-Etappe P3 — Metadaten, Radio und Entdeckung

1. **MusicBrainz-/Discogs-Tagger** mit Vorschau, konservativem Matching und
   ausdrücklicher Bestätigung vor jedem Schreiben in Musikdateien.
2. **Webradio-Browser** für Shoutcast/Icecast mit Suche, Favoriten und robustem
   Streamfehler-Verhalten.
3. **Tourdaten „Konzerte in meiner Nähe“** über Bandsintown/Songkick mit
   opt-in Standort, Cache und extern geöffneten Ticketlinks.
4. **News- und Release-Feed** über MusicBrainz/Bandcamp mit Quellenangabe,
   Aktualisierungszeit und lokalem Cache.

### Plugin-Etappe P4 — entfernte Bibliotheken und Feeds

1. **Subsonic-/Navidrome-/Jellyfin-Client** hinter einem gemeinsamen Remote-Library-
   Vertrag mit sicherer Tokenablage, Offline-Cache-Grenzen und Fehlerisolation.
2. **Podcast-Catcher und RSS-Feeds** mit Abonnements, episodischem Fortschritt,
   kontrollierten Downloads und Speicherlimit.

### Plugin-Etappe P5 — Netzwerk-Fallback

1. **YouTube-Streamer** zuletzt und separat planen. Er darf nur nach expliziter
   Aktivierung suchen und Audio streamen, wenn die lokale Datei fehlt; kein stiller
   Netzwerk-Fallback. Vor Umsetzung müssen API-/Nutzungsbedingungen, Distribution,
   Datenschutz, Altersbeschränkungen und robuste URL-/Prozessisolation geklärt sein.

### Gemeinsame Abnahmekriterien aller Plugin-Etappen

- Standardmäßig deaktiviert, sofern Netzwerk, Konto, Standort oder externe
  Übertragung beteiligt ist.
- Aktivierung zeigt benötigte Netzwerk-, Konto-, Standort- und Dateirechte klar an.
- Pluginfehler dürfen Player, Bibliothek, Queue und Startvorgang nicht blockieren.
- Netzwerk- und Parse-Arbeit läuft nie auf dem GTK-Main-Thread; UI-Ergebnisse nutzen
  Generationstoken gegen veraltete Updates.
- Zugangsdaten landen weder in Logs noch unverschlüsselt in der Bibliotheksdatenbank.
- Jede Etappe erhält isolierte Tests ohne reale Konten, Musikdateien oder Nutzerdaten.
