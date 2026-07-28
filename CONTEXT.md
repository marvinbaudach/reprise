# Reprise-Domänensprache

Dieses Glossar benennt Begriffe der Musikbibliothek, die in Core, nativen
Frontends und künftigen Agenten-Adaptern dasselbe bedeuten müssen.

## Audioverständnis

**Audio-Evidenz**:
Lokal aus dem dekodierten Audiosignal gemessene, versionierte Deskriptoren wie
Tempo, Lautheitsverteilung, spektrale Helligkeit und Onset-Dichte.
_Vermeiden_: Stimmung, Emotion, Atmosphäre

**Klangprofil**:
Eine versionierte, normalisierte Projektion der Audio-Evidenz auf wenige stabile
Dimensionen, über die Menschen und Auswahllogik Titel vergleichen können.
_Vermeiden_: Mood-Tag, Genre, Audio-Features

**Atmosphäre**:
Eine menschenlesbare, unsichere Interpretation eines Klangprofils, niemals eine
objektive Tatsache über den Titel oder seinen Hörer.
_Vermeiden_: Emotion, Ground Truth, Mood-Label

**Analyseabdeckung**:
Der Anteil geeigneter Bibliothekstitel oder Plays mit aktuellem Klangprofil,
immer zusammen mit der beschriebenen Grundgesamtheit genannt.
_Vermeiden_: Fertigstellung unter Einschluss veralteter oder ungeeigneter Titel

## Playlistplanung

**Mix-Absicht**:
Eine deklarative Menge harter Bedingungen und weicher Wünsche für eine
geordnete Musikauswahl; sie enthält keinen natürlichsprachlichen Prompt und
verändert keine Userdaten.
_Vermeiden_: Prompt, Abfrage, Playlist

**Mix-Entwurf**:
Eine unveränderliche, an ihren Quellsnapshot gebundene, geordnete Auswahl aus
einer Mix-Absicht samt Coverage-Diagnostik und strukturierten Auswahlgründen.
_Vermeiden_: Playlist, Preview-Abfrage

**Auswahlgrund**:
Eine strukturierte Aussage darüber, welche Profildimension, Bedingung oder
Diversitätsregel einen Titel in einen Mix-Entwurf gebracht hat.
_Vermeiden_: Chain of Thought, freie Begründung

**Entwurfsfreigabe**:
Explizite Befugnis, genau einen unveränderten Mix-Entwurf als manuelle Playlist
zu persistieren.
_Vermeiden_: Tool-Aufruf, implizite Zustimmung

## Agentenzugriff

**Agenten-Capability**:
Eine separat erteilte Klasse von Operationen über einen Agenten-Adapter; Lesen,
Mixplanung und Playlist-Erzeugung sind verschiedene Freigaben.
_Vermeiden_: Serverzugriff, Alles-oder-nichts-Berechtigung

## Bibliotheksnavigation

**Browser-Ort**:
Ein navigierbares Ziel samt eigenem Verfeinerungs-, Sortier-, Anker-, Auswahl-
und Inhaltsfokuszustand. Zurück und Vorwärts restaurieren denselben Ort; eine
frische Navigation erzeugt einen frischen Ort.
_Vermeiden_: Ansicht, Tab, globaler Filterzustand

**Track-Quelle**:
Die fachliche Herkunft einer Trackmenge, etwa Bibliothek, Playlist, Smart
Playlist oder Queue.
_Vermeiden_: Ansicht, Scope

**Library-Scope**:
Ein navigierbarer, aus Track-Metadaten abgeleiteter Ausschnitt der Bibliothek:
alle Tracks, ein Album, ein Interpret oder ein Genre. Ein Scope verwendet dieselbe
Trackliste und ist keine eigene Darstellungsart oder dauerhafte Entität.
_Vermeiden_: Modus, Tab, Filterchip, Albumobjekt

**Verfeinerung**:
Eine lokale Einschränkung der sichtbaren Ergebnismenge eines Browser-Orts,
etwa Textsuche, Genre, Jahr oder Bewertung.
_Vermeiden_: Scope, Queue, globaler Filter

**Wiedergabe-Snapshot**:
Die beim Start eingefrorene geordnete Menge stabiler Track-IDs samt Cursor.
Spätere Navigation, Verfeinerung oder Quellmutation berechnet sie nicht neu.
_Vermeiden_: sichtbare Liste, Live-Query

**Wiedergabe-Ursprung**:
Der strukturierte Browser-Ort und eingefrorene Anzeigename, aus dem ein
Wiedergabe-Snapshot gestartet wurde. Er dient dem spaeteren Aufdecken, besitzt
aber nicht die Wiedergabe selbst.
_Vermeiden_: aktuelle Ansicht, Queue

## Veröffentlichungsabgleich

**Veröffentlichungsbesitz**:
Eine konkrete Album- oder EP-Veröffentlichung gilt nur dann als vorhanden,
wenn die Bibliothek sie als diese Veröffentlichung vollständig enthält.
Einzelne Aufnahmen, die separat oder zuvor als Singles erschienen sind,
begründen keinen Besitz des späteren Albums oder der EP.
_Vermeiden_: vorhandene Songs, Track-Überschneidung

**Diskografielücke**:
Ein reguläres Album oder eine EP eines Bibliotheksinterpreten, für das kein
Veröffentlichungsbesitz besteht. Einzelne vorhandene Aufnahmen oder Singles
schließen die Lücke nicht.
_Vermeiden_: fehlender Song, neue Veröffentlichung

## KI-Fassungen und Provenienz

**Instrumental-Fassung**:
Eine explizit beauftragte, dauerhafte Variante eines Bibliothekstitels, aus der
per ML-Stem-Separation der Gesang entfernt wurde; ein regulärer, klar als
KI-manipuliert gekennzeichneter Titel mit dem Titelsuffix „(Instrumental)", kein
flüchtiger Effekt beim Abspielen und keine Regel-Playlist.
_Vermeiden_: Karaoke-Spur, Remix, flüchtiger Render, Vocal-Toggle

**KI-Provenienz**:
Die offengelegte Herkunft eines KI-erzeugten oder -manipulierten Titels, doppelt
hinterlegt: primär als Zeile in der Provenance-Registry der Datenbank (Flag und
optionaler Quelltitel) und sekundär in menschenlesbaren Datei-Tags, damit die
Kennzeichnung Rescans und den Export aus Reprise überlebt. Der Ausblende-Filter
schlüsselt auf das DB-Flag, nie auf den Ablageordner.
_Vermeiden_: Wasserzeichen, versteckte Markierung, App-interne ID im Tag

## Änderungspropagation

**change_log (Outbox)**:
Die transaktionale Outbox: eine je Mutation in derselben Transaktion angehängte
Zeile, die das *Was* einer Änderung total geordnet festhält (Entität,
Entitäts-ID, Operation, Writer-Token). Sie ist die Wahrheit über Änderungen
zwischen Prozessen, nicht der Weckruf selbst; Konsumenten spielen sie nicht
nach, sondern lesen daraus den aktuellen Zustand.
_Vermeiden_: Log-Datei, Audit-Trail, Nachrichten-Queue, Event-Sourcing

**Notifier**:
Der prozessübergreifende Weckruf: ein Hintergrund-Thread mit eigener Connection,
der Datenbank und WAL beobachtet und nach kurzer Beruhigung `PRAGMA
data_version` prüft — die sich nur bei Commits *anderer* Connections ändert. Er
meldet nur, *dass* etwas geschah, worauf Konsumenten das change_log lesen; lässt
sich kein Dateisystem-Watch armieren, degradiert er auf 2-Sekunden-Polling statt
aufzugeben.
_Vermeiden_: Daemon, Push-Dienst, Socket-Signal, IPC-Kanal
