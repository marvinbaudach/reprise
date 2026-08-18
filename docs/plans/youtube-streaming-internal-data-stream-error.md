---
slug: youtube-streaming-internal-data-stream-error
worktree: /home/marvin/Projects/reprise-youtube-streaming-internal-data-stream-error
branch: feature/youtube-streaming-internal-data-stream-error
phase: planned
codex_session:
created: 2026-08-18
---
# Plan: YouTube-Streaming scheitert an googlevideos Range-Zwang

Ersetzt den Befund gleichen Namens vom 16.08.2026, der ausdrücklich „keine
Diagnose" war. Die Diagnose liegt vor und ist reproduzierbar.

**Dieser Plan behandelt nur den Transport.** Das zweite, unabhängige Problem —
die App zeigt die dritte Meldung einer Fehlerkette statt der ersten — hat einen
eigenen Plan: `playback-errors-report-the-first-cause.md`.

## Die Ursache, gemessen am 18.08.2026

Der Fehler, den der Nutzer sieht — „Internal data stream error." — ist nicht der
Fehler. Er ist die dritte Meldung einer Kette. Das Journal des Laufs vom
16.08.2026 um 08:00:55 (`journalctl --user`, PID 12694) zeigt die Reihenfolge:

```
ERROR player_pipeline: GStreamer bus error error=Forbidden
        debug=… gstsouphttpsrc.c(1848): Forbidden (403),
        URL: https://rr3---sn-1giz7n7l.googlevideo.com/videoplayback?…&c=ANDROID_VR&…
ERROR player_pipeline: GStreamer bus error error=Internal data stream error.
        debug=… gstbasesrc.c(3187): streaming stopped, reason error (-5)
ERROR player_pipeline: GStreamer bus error error=Stream doesn't contain enough data.
```

**Zuerst ein HTTP 403.** Alles danach ist Folgerauschen aus einer Pipeline, die
nie Bytes bekommen hat.

### Warum 403

Die von yt-dlp aufgelöste URL war gültig: eine Sekunde alt zum Fehlerzeitpunkt
(`met=1786860054`, Fehler `06:00:55Z`), ablaufend erst sechs Stunden später
(`expire=1786881654`), ausgestellt auf dieselbe IP, von der die Anfrage kam
(`ip=82.140.144.13`). Weder Ablauf noch IP-Bindung noch ein veralteter Extraktor.

Am 18.08.2026 gegen eine **frisch aufgelöste** URL derselben Episode gemessen —
mit exakt dem Aufruf aus `ytdlp.rs:192-198`
(`yt-dlp --no-warnings -f bestaudio -j`):

| Anfrage | Antwort |
| --- | --- |
| `curl -r 0-1000` | **206** |
| `curl -r 0-524287` | **206** |
| `curl -r 0-999999` | **206**, 1 000 000 Bytes |
| `curl -H "Range: bytes=0-1048574"` | **403** |
| `curl -H "Range: bytes=0-"` (offen) | **403** |
| `curl` ganz ohne Range | **403** |
| `gst-launch-1.0 souphttpsrc location=<url> ! fakesink` | **403 → Internal data stream error.** |
| `souphttpsrc` auf `<url>&range=0-999999` | **läuft durch** |

Kontrolliert gegen die naheliegende Selbsttäuschung: frische URL, umgekehrte
Reihenfolge, identisches Ergebnis. Die Grenze entsteht nicht durch meine eigenen
Vorabfragen.

Ein mitgeschnittener `souphttpsrc`-Request (gegen einen lokalen Server) sendet:

```
User-Agent: GStreamer souphttpsrc 1.28.6 libsoup/3.6.6
Accept-Encoding: identity
Connection: Keep-Alive
icy-metadata: 1
```

— **kein `Range`**. Einzeln ausgeschlossen: User-Agent (`curl/…`, `Mozilla/5.0`,
GStreamer-UA — jeweils identisches Ergebnis), `compress`, `keep-alive`,
`icy-metadata`, HTTP/1.1 gegen HTTP/2, DLNA-Header.

**Damit steht fest:** googlevideo beantwortet nur noch begrenzte
Bereichsanfragen unterhalb von rund 1 MiB. `playbin3` holt über `souphttpsrc`
die Datei mit **einem einzigen offenen GET**. Das ist seit dieser
Serverumstellung strukturell unmöglich — nicht sporadisch, nicht
episodenabhängig, nicht netzabhängig.

Der Code kennt das Symptom bereits, nicht aber die Ursache:
`external_media.rs:397-405` hält als Kommentar fest, die HTTP-Antwort sei
*„commonly a 403 on a freshly signed googlevideo url"* — er beschreibt einen
Dauerzustand als Eigenart.

**Nicht betroffen:** heruntergeladene Episoden (`EpisodeSource::File` →
`filesrc`), Podcast-Enclosures und Radio über gewöhnliches HTTP (ein Musiktitel
lief zur selben Sitzung mit „Streaming · 12 % loaded"), sowie die Android-App —
sie hat keinen YouTube-Weg (kein `googlevideo`/`yt-dlp` im Android-Baum).

## Die Lösung: ein lokaler Range-Proxy

Entschieden im Grilling am 18.08.2026, gegen „yt-dlp als Transport" und gegen
„Streamen heißt Herunterladen".

Ein prozessinterner HTTP-Server auf `127.0.0.1` nimmt die aufgelöste
googlevideo-URL unter einem Einmal-Token entgegen. `playbin3` spielt
`http://127.0.0.1:<port>/<token>`. Der Proxy holt intern Fenster über den
Query-Parameter `&range=start-end` und reicht sie als **einen** Strom durch.

```
playbin3 ─GET─► 127.0.0.1:PORT/<token>
                     │
                     ├─ &range=0-999999      → 206
                     ├─ &range=1000000-…     → 206
                     └─ …                    → ein Strom

Seek: Range: bytes=N-  →  Fenster ab N
```

**Wirkungsbereich: ausschließlich yt-dlp-aufgelöste URLs.** Podcast-Enclosures
und Radio gehen unverändert direkt an `playbin3`. Sie funktionieren
nachweislich, und ein Umbau würde zwei laufende Wege durch neuen Code schicken.

**Ort: `reprise-core`.** `ureq = "3"` liegt dort (`reprise-core/Cargo.toml:43`),
`reprise-gnome` hat keinen HTTP-Client, und ohne GTK ist der Proxy testbar.
Kein Async-Laufzeitsystem im Projekt — also `std::net::TcpListener`, ein Thread
für den Listener, einer je Verbindung.

## Aufgaben

1. **Neues Modul** `crates/reprise-core/src/podcasts/stream_proxy.rs`.
   - Lauscht auf `127.0.0.1:0` (vom Kernel vergebener Port), Start beim ersten
     Bedarf, nicht beim Programmstart.
   - Registrierung: `register(url, total_len) -> token`; `revoke(token)`.
     Ein Token gilt für eine Sitzung und wird beim Wechsel ungültig.
   - Bedient **nur** registrierte Tokens; alles andere 404. Keine Weiterleitung
     beliebiger URLs — der Proxy ist kein offener Relay, auch nicht auf
     Loopback.
2. **Fenstermechanik.**
   - Fenstergröße als benannte Konstante, `1_000_000` Bytes, mit der Messung als
     Begründung im Kommentar: 1 000 000 tragfähig, 1 048 574 abgelehnt.
   - Aufeinanderfolgende Fenster über `&range=start-end` an den Ursprung; die
     Antworten fortlaufend in die offene Antwort an GStreamer schreiben.
   - **Ein Fenster Vorlauf:** das nächste Fenster wird geholt, während das
     aktuelle noch ausgeliefert wird. Ohne Vorlauf entsteht alle ~60 Sekunden
     eine Lücke an der Fenstergrenze.
3. **Antwortkopf richtig setzen.**
   - Ohne `Range` vom Client: `200`, `Content-Length` = Gesamtlänge,
     `Accept-Ranges: bytes`. Die Gesamtlänge kommt aus der yt-dlp-Auflösung
     (`filesize` bzw. `clen` in der URL) — ohne sie kann GStreamer weder die
     Dauer noch die Seek-Fähigkeit bestimmen.
   - Mit `Range: bytes=N-`: `206` samt `Content-Range`, Auslieferung ab `N`.
     Das ist der Seek-Weg und **keine Kür**: `start_podcast_source` löst bei
     `resume_ms > 0` unmittelbar nach dem Start ein `seek_to` aus
     (`external_media.rs:405-408`).
4. **Fehlerbehandlung im Proxy.**
   - 5xx oder Verbindungsabbruch auf ein Fenster: dasselbe Fenster erneut
     versuchen, mit Obergrenze.
   - **403 mitten im Strom:** die signierte URL kann abgelaufen sein. Einmal
     neu auflösen lassen und ab demselben Offset weitermachen. Dafür braucht
     der Proxy einen Rückruf zum Neuauflösen, keine eigene yt-dlp-Kenntnis.
   - Bricht GStreamer die Verbindung ab (Trackwechsel), endet der
     Verbindungsthread, ohne den Listener zu stören.
5. **Anschluss in der Oberfläche.**
   `crates/reprise-gnome/src/ui/playback/external_media.rs`: `resolve_youtube`
   reicht `audio.stream_url` nicht mehr direkt an `start_podcast_source`,
   sondern registriert sie beim Proxy und spielt die lokale URL. Das Token wird
   beim Sitzungswechsel widerrufen.
6. **Bindefehler ist ein Fehler, kein stiller Rückfall.** Kann der Listener
   nicht starten, scheitert die Wiedergabe mit einer Meldung. Ein Rückfall auf
   die direkte URL wäre ein Rückfall auf einen gemessen kaputten Weg.

## Tests

Ohne Netz, gegen einen lokalen Fake-Ursprung, der **genau das gemessene
Verhalten** nachbildet: offene und ≥ 1 MiB große Anfragen mit 403, begrenzte mit
206 samt `Content-Range`.

1. Ein vollständiger Durchlauf setzt aus mehreren Fenstern einen Strom zusammen,
   der byteweise der Quelldatei entspricht.
2. Eine Anfrage mit `Range: bytes=N-` liefert `206`, korrektes `Content-Range`
   und die richtigen Bytes ab `N`.
3. Ein unbekanntes Token bekommt 404; ein widerrufenes ebenfalls.
4. Ein Fenster, das einmal mit 500 antwortet, wird wiederholt; der Strom bleibt
   lückenlos.
5. Ein Fenster, das mit 403 antwortet, löst genau **einen** Neuauflösungsversuch
   aus und setzt am selben Offset fort.
6. Der Fake-Ursprung protokolliert die angefragten Bereiche: **keine** Anfrage
   ist offen oder ≥ 1 MiB. Das ist der Test, der die eigentliche Regression
   festnagelt.

## Nachweis vor dem Abschluss

Die Testsuite dieses Repos ist ausdrücklich kein hinreichender Beleg. Zusätzlich,
an der laufenden App:

1. `gst-launch-1.0` gegen eine frisch aufgelöste URL zeigt weiterhin 403 — die
   Umgebung hat sich nicht von selbst geheilt, der Test misst noch etwas.
2. Eine **nicht heruntergeladene** YouTube-Episode spielt über mindestens 90
   Sekunden hinaus (mehr als ein Fenster) mit fortlaufender Position.
3. Ein Sprung auf Minute 2 spielt von dort weiter.
4. Eine angefangene Episode wieder aufnehmen: sie setzt an der gespeicherten
   Position ein (der Seek-Weg direkt nach dem Start).
5. Journal des Laufs: kein `Forbidden`, kein `Internal data stream error`.
6. Ein heruntergeladenes Stück und ein Radiosender spielen unverändert — der
   Proxy hat sich nicht in fremde Wege gedrängt.

## Parallelität

**Nicht teilbar.** Der Proxy und sein einziger Anschlusspunkt
(`external_media.rs`) sind eine Einheit: der Anschluss ohne Proxy baut nicht,
der Proxy ohne Anschluss ist unerreichbar und unbeweisbar. Ein Schnitt entlang
der Crate-Grenze erzeugte zwei Stränge, von denen einer bis zum Merge des
anderen nicht grün werden kann — genau der Fehler, den ein Schnitt vermeiden
soll.

Der zweite Fehler des ursprünglichen Befunds — die Fehlermeldung — ist deshalb
ein **eigener Plan** (`playback-errors-report-the-first-cause.md`) mit eigenem
Branch, statt ein Strang hier.
