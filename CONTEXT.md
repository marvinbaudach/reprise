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
