# Plugin-Grenzen — Designkorrektur

## Ziel

Die Einstellungen unterscheiden dauerhaft zwischen Kernfunktionen und optionalen
Integrationen. Funktionen, die ein lokaler Musikplayer ohne Konto oder Fremddienst
vollständig bereitstellen muss, dürfen nicht zusätzlich als Plugin erscheinen.

## Feste Kernbereiche

- „Wiedergabe“ besitzt Equalizer und ReplayGain.
- „Synchronisation“ besitzt später MTP-, iPod- und WLAN-Geräte-Support.
- Diese Funktionen haben keine zweite Zeile und keinen zweiten Schalter unter
  „Plugins“.

## Plugins

Plugins sind optional oder binden externe APIs/Dienste an. Der vorgesehene Ausbau
umfasst Lyrics, Scrobbling, Discord Rich Presence, MPRIS, MusicBrainz-/Discogs-Tagger,
synchronisierte Lyrics, Webradio-Browser für Shoutcast/Icecast, Subsonic-/Navidrome-/
Jellyfin-Clients, Podcasts/RSS, Einschlaf-Timer, Tourdaten über Bandsintown/Songkick,
News- und Release-Feeds über MusicBrainz/Bandcamp, Desktop-Benachrichtigungen,
File-Writer für Streamer sowie einen YouTube-Audiostream-Fallback.

Der bereits vorhandene Online-Coverabruf bleibt als optionale Netzwerkfunktion im
Pluginsystem. Diese Liste ist ein Produkt-Backlog, keine Zusage, alle Einträge jetzt
zu implementieren.

## Migration und Verhalten

Die bestehenden Playback-Settings bleiben unverändert und wirken weiterhin live.
Historische `module.equalizer.enabled`- oder `module.replaygain.enabled`-Zeilen dürfen
in alten Datenbanken folgenlos liegen bleiben; sie werden nicht mehr gelesen und
müssen nicht destruktiv migriert werden. MPRIS behält bis zu einem sicheren
Hot-Reload seinen Neustart-Hinweis.

## Tests und Sicherheit

Ein Core-Test beweist, dass die Plugin-Registry Equalizer und ReplayGain nicht mehr
enthält. UI-Tests beweisen, dass nur der Online-Coverabruf live umschaltbar ist.
Weder Musikdateien noch reale Nutzerdaten werden für diese Korrektur berührt.

## Explizit nicht Teil

Keine der zukünftigen Plugin-Ideen wird in diesem Schritt implementiert. Es entsteht
noch keine Fremd-Plugin-ABI, kein Plugin-Download und keine Geräte-Synchronisation.
