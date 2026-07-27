# Affiliate-Kauflinks in der Releases-Ansicht

Status: provisionsfreier Bandcamp-Direktlink umgesetzt; Anbieter- und
Vertragsentscheidung für Affiliate-Monetarisierung ausstehend
(Stand 2026-07-27).

## Ziel

Fehlende Alben und EPs dürfen in der Releases-Ansicht einen klar
gekennzeichneten Kauflink anbieten. Reprise kann daran nur verdienen, wenn
der konkrete Partner installierbare Linux-Desktop-Apps ausdrücklich zulässt
und Reprise für das Programm freigeschaltet ist.

## Sichere aktuelle Basis

- Die Zeilenaktivierung öffnet weiterhin die von MusicBrainz gelieferte
  Release-URL im Standardbrowser.
- Eine echte Bandcamp-Relation auf eine direkte `/album/…`-Seite wird in der
  Releases-Tabelle zusätzlich als sichtbarer `Bandcamp`-Kauflink angeboten.
  Reprise öffnet die unveränderte URL, hängt keine Tracking-Parameter an und
  behauptet keine Vergütung.
- Andere Kaufrelationen bleiben über die neutrale Zeilenaktivierung
  erreichbar und werden nicht als Bandcamp oder „Buy" beschriftet.
- Ohne echte Kaufrelation bleibt die MusicBrainz-Release-Group-Seite der
  ehrliche Fallback. Eine Homepage oder Streaming-Seite wird nicht als
  „Buy" beschriftet.
- Es gibt kein Klick-Tracking und keine Übertragung von Bibliotheksdaten an
  einen Affiliate-Anbieter.

## Aktuell geprüfte Anbietergrenzen

### Amazon Associates

Die Standard-Teilnahmebedingungen untersagen Special Links in
clientseitig installierbarer Software einschließlich Desktop-Anwendungen;
ausgenommen sind nur ausdrücklich genehmigte Mobile Apps oder eine separate
schriftliche Vereinbarung. Darum darf Reprise keinen normalen Amazon-
Affiliate-Tag in die Desktop-App einbauen.

Falls Amazon Reprise später schriftlich zulässt, verlangen die Bedingungen
zusätzlich eine klare Link-Kennzeichnung sowie die vorgeschriebene
Associate-Offenlegung.

Quellen:

- https://affiliate-program.amazon.com/help/operating/agreement/
- https://affiliate-program.amazon.com/help/operating/participation/
- https://affiliate-program.amazon.com/help/node/topic/GHQNZAU6669EZS98

### Qobuz über Linkfire

Das dokumentierte Qobuz-Programm bei Linkfire vergütet Streaming-
Abonnements und setzt einen bezahlten Business- oder Enterprise-Tarif
voraus. Es ist deshalb kein passender Vertrag für den Kauf einzelner Alben
oder EPs in Reprise.

Quelle:

- https://help.linkfire.com/hc/en-us/articles/360019950319-Qobuz-Affiliate-Program-with-Linkfire

### Bandcamp

Bandcamp bleibt fachlich der beste direkte Kauf-Fallback, weil Fans dort
Musik und Merchandise unmittelbar von Artists und Labels kaufen. In den
geprüften öffentlichen Bandcamp-Unterlagen wurde jedoch kein für Reprise
freigegebenes Desktop-Affiliate-Programm identifiziert; Links bleiben daher
bewusst provisionsfrei.

Quelle:

- https://bandcamp.com/about

## Verbindliche Implementierungsgrenze

Affiliate-Kauflinks werden erst aktiviert, wenn alle folgenden Punkte
belegt sind:

1. Schriftlicher Vertrag oder öffentlich eindeutige Programmbedingung für
   installierbare Linux-Desktop-Apps.
2. Festgelegte Märkte, Auszahlungsempfänger und steuerliche Zuständigkeit.
3. Vom Owner freigegebener Offenlegungstext in der Releases-Ansicht und in
   den rechtlichen Hinweisen.
4. Provider-spezifischer URL-Builder mit Domain-Allowlist und Tests, der
   nur echte Kaufziele verändert.
5. Providerneutraler Fallback auf die unveränderte MusicBrainz-Relation,
   wenn Konfiguration, Markt oder Ziel nicht passen.
6. Keine geheimen Schlüssel im Repository, keine Bibliotheksdaten im Link
   und keine Telemetrie ohne eine eigene Opt-in-Entscheidung.

## Offene Stage-Review-Entscheidung

Der Owner wählt und genehmigt zuerst einen Partner, der Albumkäufe in einer
Linux-Desktop-App vertraglich erlaubt. Danach bekommt der Link eine eigene
kleine Implementierungsstage mit Offenlegung, URL-Allowlist, Tests und
manueller Browserprüfung.
