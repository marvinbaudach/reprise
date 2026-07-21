# ADR 001: Music verwendet einen gescopten TrackList-Browser

## Status

Angenommen am 2026-07-20.

## Context

Music zeigt Tracks, Albums und Artists derzeit als synchronisierte visuelle
Modi, obwohl Album- und Artist-Details die gemeinsame TrackList bereits über
`ViewSource` abfragen. Filter, Playing-Marker, Fokus, Scroll-Gedächtnis,
History und Metadatenlinks des Players muessen dadurch fuer drei Darstellungen
derselben Bibliothek gesondert koordiniert werden. Die entstehenden
Interaktionen sind mehrdeutig und haben sichtbare Fokus- und Scrollfehler
verursacht.

Album- und Interpret-Identität wird aus Track-Metadaten abgeleitet. Reprise
besitzt keine dauerhaften Album- oder Interpret-Entitäten, deren Lebenszyklus
diese Projektionen als eigene App-Orte rechtfertigen würde.

## Decision

Music besitzt einen virtualisierten TrackList-Browser. Die Library-Collection
hat drei Scopes: alle Tracks, ein Album oder einen Interpreten. Album- und
Interpret-Scopes sind navigierbare History-Einträge mit lokalen
Verfeinerungen und stabilen View-Bookmarks, aber keine dauerhaften
Datenbankentitäten. My Stats bleibt ein eigenes Dashboard, das zu Scopes
navigieren oder Wiedergabe-Snapshots erzeugen kann.

Die kompatiblen Query-Varianten `ViewSource::Album` und `ViewSource::Artist`
bleiben während der Migration bestehen; das Domain-Interface bilden jedoch
`BrowserPlace`, `TrackCollection` und `LibraryScope`.

## Consequences

- Tracks, Alben und Interpreten teilen eine Playing-Marker-, Auswahl-, Fokus-
  und Scroll-Implementierung.
- Frische Scope-Navigation erbt nie versteckte Filter; Back und Forward
  restaurieren den vollstaendigen vorherigen Ort.
- Ein Scope, dessen letztes Mitglied verschwindet, bleibt in der Sitzung als
  ehrlicher Leerzustand erhalten und faellt nach einem Neustart auf Music
  zurueck, wenn er nicht mehr aufloesbar ist.
- Tracks/Albums/Artists-Switcher, gridspezifisches Fokussystem und
  modusübergreifende Synchronisierung entfallen, sobald alle Aufrufer das
  neue Interface verwenden.
- Ein künftiger Cover-Browser ist ein separates Feature, kein vierter
  synchronisierter Music-Modus.

## Alternatives considered

- Die drei Modi behalten und jede Synchronisierungskante reparieren. Verworfen,
  weil drei Implementierungen derselben Library-Zustandsmaschine bleiben.
- Album und Interpret als normale Filterchips behandeln. Verworfen, weil
  History, Scope-Header, kanonische Containeraktionen und klares
  Back-Verhalten verloren gehen.
- Dauerhafte Album- und Interpret-Tabellen einfuehren. Verworfen, weil
  Tag-Änderungen Merge-/Split-Identitätsregeln und einen Lebenszyklus leerer
  Entitäten ohne Produktnutzen erfordern würden.
