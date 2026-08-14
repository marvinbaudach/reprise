---
slug: deezer-placeholder-portraits
worktree: /home/marvin/Projects/reprise-deezer-placeholder-portraits
branch: feature/deezer-placeholder-portraits
phase: shipped
codex_session:
created: 2026-08-13
---
# Deezer liefert Platzhalter statt Bandbildern

## Problem und Ursache

In *My Stats* tragen Rang 3 („The Devil Wears Prada") und Rang 10 („Oceano") ein
graues Personen-Icon. Das ist **kein Rückfallzustand der App**. Die Bildkette in
`StatsArtistImage::load` fällt bei fehlendem Porträt auf ein Album-Cover und
zuletzt auf Initialen zurück — nie auf ein Personen-Symbol. Was dort zu sehen
ist, ist Deezers eigenes Platzhalterbild: eine graue Silhouette, die als
reguläres Porträt heruntergeladen, validiert und im Cache abgelegt wurde. Die
App zeigt es, weil sie glaubt, ein Porträt zu haben.

Dahinter liegen zwei Fehler im selben Pfad, plus ein bereits eingetretener
Folgeschaden im Zwischenspeicher.

### Fehler 1 — die Platzhalter-Erkennung greift zu kurz

`is_placeholder_url` in `crates/reprise-core/src/artist_portrait/deezer.rs` ist
genau `url.contains("/artist//")`. Sie erkennt einen *fehlenden*
Pfadabschnitt. Deezers tatsächlicher Platzhalter trägt aber eine Bildkennung:
`d41d8cd98f00b204e9800998ecf8427e`, den MD5 des leeren Strings — das Ergebnis
davon, dass Deezer ein nicht vorhandenes Bild trotzdem in sein URL-Schema
einsetzt. Diese URL läuft an der Prüfung vorbei und gilt als echtes Bild.

### Fehler 2 — der erste Namenstreffer gewinnt bedingungslos

`parse_best_artist` iteriert über `data`, überspringt Kandidaten mit abweichendem
normalisiertem Namen und liefert beim ersten Namenstreffer sofort zurück. Ob
dieser Kandidat überhaupt ein Bild hat, ändert daran nichts — `picture_url` ist
ein `Option`, das bei erkanntem Platzhalter schlicht `None` wird. Ein zweiter,
exakter Namenstreffer mit echtem Bild wird nie angesehen.

### Die beiden Fehler greifen ineinander

Fehler 1 lässt den Platzhalter als echtes Bild durch — er wird geladen und im
Cache eingefroren. Fehler 2 sorgt dafür, dass ein *korrekt* erkannter Platzhalter
nicht zum nächsten Kandidaten führt, sondern in `write_negative` mündet, also in
einen `.notfound`-Marker statt in das vorhandene echte Bild. Wer nur Fehler 1
behebt, tauscht ein falsches Bild gegen ein fehlendes. Wer nur Fehler 2 behebt,
sortiert weiterhin Platzhalter nach vorn, weil er sie nicht als solche erkennt.
Beide gehören in einen Wurf.

### Frisch nachgemessen (13.08.2026, live gegen `api.deezer.com`)

Diese Zahlen sind neu erhoben und tragen die Entscheidungen unten. Gezeigt ist
jeweils die Bildkennung aus dem `picture_xl`-Pfad; `picture_big` trägt in allen
beobachteten Antworten dieselbe Kennung.

**`q=The Devil Wears Prada&limit=5` → 3 Treffer**

| # | Name | Alben | Fans | Bildkennung |
|---|------|-------|------|-------------|
| 1 | The Devil Wears Prada | 2 | 11 | `d41d8cd9…427e` → Platzhalter |
| 2 | The Devil Wears Prada | 47 | 90 772 | `ce8738d5…c62a` → echtes Bild |
| 3 | The Devil Wears Prada Original West End Cast | 3 | 126 | echtes Bild, **Name nicht exakt** |

**`q=Oceano&limit=5` → 5 Treffer, erster exakter Treffer:**
Oceano, 36 Alben, 16 388 Fans, `415714b6…afe4` → **echtes Bild**.
Deezer liefert für Oceano heute an erster Stelle ein gültiges Porträt. Der graue
Kreis bei Rang 10 stammt also nicht aus der heutigen Auswahl, sondern aus dem
vergifteten Cache-Eintrag vom Juli. **Ein reiner Code-Fix ändert für Oceano
nichts.** Das ist der wichtigste neue Befund dieser Voruntersuchung.

**`q=ONI&limit=5` → 5 Treffer**

| # | Name | Alben | Fans | Bildkennung |
|---|------|-------|------|-------------|
| 1 | Oni | 90 | 9 | `d41d8cd9…427e` → Platzhalter |
| 2 | ONI | 21 | 14 | echtes Bild |
| 3 | Oni | 10 | 1 | echtes Bild |
| 4 | ONI | 63 | 164 | echtes Bild |
| 5 | ONI | 7 | 2 558 | echtes Bild |

Alle fünf sind nach `normalize` exakte Namenstreffer. Eine Regel „erster Treffer
mit Bild gewinnt" würde hier Kandidat 2 mit 14 Fans nehmen — mit hoher
Wahrscheinlichkeit die falsche Band. Der plausible Treffer steht auf Platz 5,
also exakt an der Grenze des heutigen `limit=5`.

Nebenbefund: Deezer antwortet heute mit dem Host `cdn-images.dzcdn.net`, die
Test-Fixtures im Repo benutzen `e-cdns-images.dzcdn.net`. `is_deezer_image_url`
prüft nur auf `*.dzcdn.net`, es liegt also kein Fehler vor — aber Fixtures
sollten die reale Form tragen und dürfen den Host nicht enger festnageln.

### Ein Test zementiert den Irrtum

`is_placeholder_detects_empty_md5_segment` heißt, als prüfe er die
Leerstring-MD5, prüft aber `/artist//` und behauptet ausdrücklich, dass eine URL
mit gefüllter Kennung *kein* Platzhalter sei. Der Name lügt über den Inhalt.
Ebenso bauen die `PLACEHOLDER`-Konstanten in den Tests von `deezer.rs` und
`mod.rs` sowie `parse_treats_deezer_placeholder_as_no_picture` und
`parse_falls_back_to_real_big_when_xl_is_a_placeholder` auf einer Form auf, die
in echten Antworten nicht vorkommt. Diese Tests müssen mitgezogen werden, sonst
sind sie nach der Reparatur doppelt irreführend.

### Der Zwischenspeicher — Zustand am 13.08.2026

`~/.cache/reprise/artist-portraits`, 162 Einträge: 159 Bilder, 3
`.notfound`-Marker. 156 Dateien tragen mtime 18.07.2026, 6 tragen 19.07.2026 —
seither wurde nichts nachgeholt. Die positive TTL ist 30 Tage, die negative 7
Tage. Der gesamte Bildbestand läuft damit am **17. bzw. 18.08.2026** ab, die
Negativ-Marker sind längst abgelaufen. Nach Inhalts-Hash gruppiert sind genau
**neun** Bilder Platzhalter: siebenmal eine Variante, zweimal eine zweite.

Harte Randbedingung: Der Cache legt nur die Bildbytes unter einem Namens-Hash ab
— keine Quell-URL, keine Herkunft. Ein vergifteter Eintrag ist **aus dem Cache
heraus nicht erkennbar**. Eine Erkennung über Datei-Hashes ist unzulässig:
Deezer hat den Platzhalter zwischenzeitlich neu gezeichnet, es liegen zwei
byte-verschiedene Varianten vor, eine dritte kann jederzeit entstehen. Stabil ist
allein die Kennung in der URL — und die steht im Cache nicht.

### Wer den Abruf überhaupt auslöst

Wichtig für jede Abnahme: Die Stats-Ansicht selbst liest nur (`load_cached`).
Findet sie nichts, zeigt sie sofort ein Album-Cover **und** stellt parallel eine
Anfrage an `ArtistPortraitRuntime`, die `load_or_fetch` auf einem Worker
ausführt; ein später eintreffendes Porträt überschreibt das Cover. Der Abruf
hängt also am Öffnen von *My Stats*, nicht am Abspielen. Er passiert aber **nur**,
wenn das Artwork-Modul (`module.artwork.enabled`, Voreinstellung **aus**) und das
globale Online-Quellen-Gate ihn erlauben. Wer das übersieht, misst eine Abnahme,
in der nie ein Netzabruf stattgefunden hat, und hält das Ergebnis für grün.

**Und *My Stats* ist die einzige Oberfläche.** Nachgeprüft am 13.08.2026: Das
Porträtergebnis wird ausschließlich unter `crates/reprise-gnome/src/ui/stats/`
verbraucht (plus die Verdrahtung in `window.rs`). Dass die Worker-Datei unter
`ui/now_playing/` liegt, führt in die Irre — *Now Playing* zeigt kein Porträt.
Der Abnahme-Zuschnitt auf *My Stats* ist damit vollständig und nicht zu eng; es
gibt keine zweite Ansicht, die stillschweigend mitbetroffen wäre.

---

## Entscheidungen mit Begründung

### E1 — Erkennung über die Bildkennung, nicht über die ganze URL

**Beschluss.** Die Erkennung arbeitet künftig auf dem Bezeichner-Segment des
Deezer-Bildpfads (`…/images/artist/<kennung>/<größe>-…jpg`), nicht auf einem
Teilstring der ganzen URL. Ein Kandidat gilt als bildlos, wenn seine Kennung
leer ist **oder** in einer benannten, zentral abgelegten Sentinel-Menge steht.
Diese Menge enthält zum Start genau zwei Einträge: das leere Segment (die
bisherige `/artist//`-Form) und `d41d8cd98f00b204e9800998ecf8427e`.

**Begründung.** Der Hash ist kein willkürlicher Zauberwert, sondern die
zwangsläufige Folge davon, dass Deezer ein *nicht vorhandenes* Bild durch
dieselbe Hash-Funktion schickt: MD5 des leeren Strings. Er ist damit so stabil
wie Deezers URL-Schema selbst — deutlich stabiler als die gerenderten Pixel, die
bereits zweimal existieren. Er gehört als benannte Konstante mit genau diesem
erklärenden Kommentar in den Code, nicht als zur Laufzeit berechneter MD5: die
Berechnung wäre Cleverness ohne Gewinn und würde eine Hash-Abhängigkeit
einschleppen.

**Warum nicht inhaltsbasiert.** Eine Erkennung an den heruntergeladenen Bytes
(Hash-Vergleich, Erkennung einer einfarbigen Fläche) kostet erst den Download,
ist gegen Neuzeichnungen blind und wurde ausdrücklich ausgeschlossen. Sie kommt
auch als Zusatzsicherung nicht in Frage.

**Wenn Deezer eine dritte Kennung einführt.** Dann sieht der Code sie als
gewöhnliches Bild. Drei Dinge fangen das ab, in dieser Reihenfolge: (1) die
Auswahlregel aus E2 sorgt dafür, dass ein Platzhalter auf einem unbedeutenden
Doppelgänger-Eintrag den populären Kandidaten nicht mehr verdrängt — der
häufigste Fall verschwindet damit ganz unabhängig von der Erkennung; (2) das
Hinzufügen einer weiteren Kennung ist eine Ein-Zeilen-Änderung an einer
benannten Stelle; (3) die 30-Tage-TTL begrenzt den Schaden auf einen Monat.
Ein automatischer Erkennungsmechanismus für *unbekannte* Platzhalter wird
bewusst nicht gebaut (YAGNI, und jede plausible Heuristik wäre inhaltsbasiert).

**`limit`.** Wird von 5 auf 10 angehoben. Bei „ONI" steht der plausible Treffer
heute auf Platz 5 — also exakt am Rand; ein weiterer Deezer-Eintrag würde ihn aus
dem Fenster schieben. Die Kosten sind null zusätzliche Anfragen und eine
geringfügig größere Antwort, weit unterhalb des bestehenden 4-MB-Deckels.
Paginierung wird **nicht** eingeführt: ein exakter Namenstreffer jenseits von
Platz 10 ist ein hypothetisches Problem, und die Auswahlregel wird mit mehr
Kandidaten eher besser als schlechter.

### E2 — Auswahl: unter den exakten Namenstreffern gewinnt der populärste mit echtem Bild

**Beschluss.** Die Namensprüfung bleibt unverändert hart: nur Kandidaten, deren
`normalize(name)` exakt dem gesuchten Namen entspricht, kommen überhaupt in
Frage. Innerhalb dieser Menge wird nicht mehr „der erste" genommen, sondern
sortiert:

1. **Primär:** Kandidaten mit echtem Bild vor Kandidaten ohne (Platzhalter,
   leere Felder, fehlende Felder).
2. **Sekundär:** absteigend nach `nb_fan`.
3. **Tertiär:** stabil, also Deezers eigene Reihenfolge, wenn 1 und 2 nicht
   entscheiden.

Fehlt `nb_fan` oder ist es `null`, zählt es als 0 — kein Panic, kein Ausschluss.
Die Sortierung muss **deterministisch** sein: dieselbe Antwort muss über Läufe
hinweg dieselbe Wahl ergeben, sonst flattert der Cache-Inhalt.

**Begründung.** Deezers eigene Reihenfolge ist nachweislich nicht nach Richtigkeit
sortiert — bei „ONI" steht ein Eintrag mit 9 Fans und Platzhalter ganz vorn,
der plausible mit 2 558 Fans ganz hinten. Popularität unter *exakt gleich
benannten* Künstlern ist der beste verfügbare Näherungswert dafür, welchen
Künstler ein Hörer meint, und sie ist im Gegensatz zur heutigen Wahl
nachvollziehbar und reproduzierbar. Die Regel prüft an den Messwerten: Bei
„The Devil Wears Prada" gewinnt der Eintrag mit 90 772 Fans und echtem Bild
gegen den mit 11 Fans und Platzhalter; bei „Oceano" bleibt der heutige erste
Treffer (16 388 Fans, echtes Bild) auch der gewählte; bei „ONI" gewinnt der
Eintrag mit 2 558 Fans statt des zufälligen mit 14.

**`nb_fan` kostet keinen Zusatzabruf.** Nachgeprüft am 13.08.2026 gegen
`api.deezer.com/search/artist`: Die Objekte der Suchantwort tragen bereits
`nb_fan` und `nb_album` (Felder: `id`, `link`, `name`, `nb_album`, `nb_fan`,
`picture`, `picture_big`, `picture_medium`, `picture_small`, `picture_xl`,
`radio`, `tracklist`, `type`). Es wird also **keine** zusätzliche Anfrage pro
Kandidat nötig — die Regel arbeitet auf Daten, die ohnehin schon da sind. Ohne
diesen Befund wäre E2 eine ganz andere, teurere Entscheidung gewesen.

**Warum `nb_fan` und nicht `nb_album`.** Die beiden denkbaren
Popularitätssignale widersprechen sich in den Messdaten: Bei „ONI" hat der
Eintrag mit 2 558 Fans nur 7 Alben, während ein Eintrag mit 9 Fans deren 90
führt. Eine Regel über `nb_album` würde hier also einen anderen Künstler wählen.
Fans messen Hörerresonanz; Albumzahlen blähen sich durch Kompilationen, Singles
und Katalogimporte auf. `nb_fan` ist damit das ehrlichere Signal für „wen meint
ein Hörer, der diesen Namen sagt".

**Warum „echtes Bild" vor „populär" und nicht umgekehrt.** Das Ziel ist ein
sichtbares Porträt. Ein populärer Kandidat ohne Bild bringt keins; ein etwas
weniger populärer mit Bild schon. Die Namensklasse wird dabei nie verlassen, das
Risiko bleibt also innerhalb der Gleichnamigen.

Diese Reihenfolge hat eine Konsequenz, die ausdrücklich gewollt ist und im Grill
gegen die Gegenoption entschieden wurde: Hat ausgerechnet der mit Abstand
populärste Gleichnamige bei Deezer kein Porträt, ein unbedeutender Namensvetter
aber schon, dann zeigt die App das Gesicht des Namensvetters. Die Alternative
wäre gewesen, den populärsten die Identität bestimmen zu lassen und den Künstler
andernfalls als bildlos zu führen — ehrlicher, aber sie hätte genau die Ränge
leer gelassen, wegen derer dieses Vorhaben existiert. Der Fehlerfall bleibt
kosmetisch und auf Gleichnamige begrenzt.

**Falscher Künstler mit Bild.** Das kann passieren und ist ein bewusst
akzeptiertes Restrisiko: Bei Namensdubletten trifft die Regel den populäreren.
Für eine wirklich obskure Band mit prominentem Namensvetter wird das falsch sein
— aber es ist nicht *schlechter* als heute, wo Deezers Reihenfolge entscheidet
und ebenfalls jeden der Namensvettern liefern kann. Zusätzliche Signale
(`nb_album`, Genre, Abgleich gegen die lokale Bibliothek) werden hier **nicht**
eingeführt: sie kosten Komplexität und Netzabrufe für einen Fall, der in den
sichtbaren zwanzig Rängen nicht belegt ist. Die Abnahme aus E4 enthält
stattdessen eine Regressionsprüfung über alle sichtbaren Ränge.

**Alle exakten Treffer sind Platzhalter.** Dann bleibt es bei heutigem Verhalten:
`write_negative`, also `.notfound` mit 7 Tagen TTL, und **kein** Download. Das ist
richtig — es lädt keine graue Fläche herunter, zeigt korrekt Cover oder
Initialen und probiert es in einer Woche erneut. Die TTLs werden nicht angefasst.

**Rückfallkette `picture_xl` → `picture_big`.** Bleibt erhalten, aber ihre Rolle
wird geschärft: Sie entscheidet, **welche Größe** genommen wird, nicht, **ob** der
Kandidat ein Bild hat. Ob ein Kandidat ein echtes Bild besitzt, ist eine
Eigenschaft des Kandidaten (seiner Bildkennung), nicht eines einzelnen Feldes —
in allen beobachteten Antworten tragen `picture_xl` und `picture_big` dieselbe
Kennung. Die Kette fängt weiterhin fehlende oder leere Felder ab. Der Test
`parse_falls_back_to_real_big_when_xl_is_a_placeholder` konstruiert einen Fall,
den Deezer so nicht liefert (unterschiedliche Kennungen für xl und big); er ist
auf den realen Fall umzuschreiben — `picture_xl` fehlt oder ist leer, `picture_big`
trägt ein echtes Bild.

### E3 — Die neun vergifteten Einträge: einmalig manuell löschen, kein Code

**Beschluss.** Es wird **kein** Invalidierungs-, Epochen- oder
Versionsmechanismus in den Cache eingebaut. Stattdessen wird der
Porträt-Cache-Ordner der echten Installation *einmalig, von Hand und erst nach
ausdrücklicher Freigabe des Nutzers* geleert, nachdem der Fix installiert ist.
Der Plan behandelt das als Betriebsschritt in der letzten Welle, nicht als
Liefergegenstand des Codes.

**Warum nicht (b), eine Cache-Epoche.** Weil der Cache keine Herkunft speichert,
kann eine ausgelieferte Invalidierung nicht zwischen den 9 schlechten und den
150 guten Einträgen unterscheiden — die einzige umsetzbare Form wäre ein
pauschales „alles vor Datum X verwerfen". Das ist dauerhaft im Code stehender
Aufwand für ein Problem, das genau einmal auftritt und am 17./18.08.2026 ohnehin
verfällt, und es wirft 150 gültige Einträge weg, um 9 zu treffen. Der
Nutzen-Zeitraum rechtfertigt die bleibende Komplexität nicht.

**Warum nicht (a), einfach ablaufen lassen.** Das ist die verlockende Antwort,
aber sie hängt an einem Zeitfenster. Die Bedingung, unter der (a) trägt, lautet
**ausdrücklich**: *Der reparierte Build muss auf dem Rechner des Nutzers laufen,
bevor die Einträge am 17./18.08.2026 verfallen, und bis dahin darf kein Lauf des
alten Builds sie auffrischen.* Landet der Fix später, sind die Folgen
kandidatenabhängig und nicht einheitlich: Für „Oceano" würde selbst der alte Code
beim Verfall ein echtes Bild holen, weil Deezers erster exakter Treffer heute
eins hat. Für „The Devil Wears Prada" holt der alte Code erneut den Platzhalter
und friert ihn für weitere 30 Tage ein — dann ist derselbe Fehler bis Mitte
September sichtbar. Auf ein solches Fenster darf sich die Abnahme nicht stützen.

**Warum (c) das Richtige ist.** Ein `rm -rf` auf
`~/.cache/reprise/artist-portraits` braucht überhaupt keine Erkennung — es
umgeht die Randbedingung, statt gegen sie anzurennen —, kostet null bleibenden
Code und macht die Reparatur vom Kalender unabhängig. Der Preis ist, dass die
150 gesunden Einträge ebenfalls neu geholt werden; das geschieht faul und nur
für tatsächlich angezeigte Künstler, je einmal Suche plus Download, mit 300 ms
Mindestabstand auf einem Worker. Angesichts der am 17./18.08.2026 ohnehin
fälligen Komplettauffrischung ist dieser Preis nahe null. Die Löschung macht die
Reparatur in der Installation des Nutzers **sofort sichtbar**, statt sie am
Kalender hängen zu lassen.

**Auch die gezielte Löschung der neun wurde erwogen und verworfen.** Die neun
Dateien sind über ihren Inhalts-Hash exakt bestimmbar (7 + 2 Varianten, am
13.08.2026 gemessen), und das Hash-Verbot gilt nur für *ausgelieferte* Erkennung,
nicht für einen einmaligen Handgriff — sie ließen sich also gezielt entfernen und
die 150 gesunden Einträge behalten. Verworfen zugunsten des pauschalen Leerens:
Der Unterschied ist ein einmaliges, fauliges Nachladen von 150 Bildern, und ein
Befehl ohne Auswahlkriterium kann nicht das Falsche treffen.

**Der Sonderfall „ausgelieferte Nutzer" besteht nicht.** Das Projekt hat keine
einzige Veröffentlichung (`gh release list` ist leer); betroffen ist genau eine
Installation, die des Nutzers. Die Erwägung, dass ein ausgelieferter Bestand eine
Epoche verhältnismäßig machen würde, ist damit gegenstandslos und **kein**
Prüfauftrag an den Implementierer. Sollte sich das vor der Umsetzung ändern,
gehört der Beschluss neu bewertet — nicht eigenmächtig ein Mechanismus gebaut.

### E4 — Beweisführung: zwei Ebenen, und grüne Unit-Tests sind nur die untere

Grüne Tests belegen in diesem Projekt kein sichtbares Verhalten. Die Abnahme
besteht daher aus einer Unit-Ebene mit den injizierbaren Closures und einer
sichtbaren Ebene in *My Stats*, die die echte Bibliothek des Nutzers nicht
verändert und dessen laufende App nicht anfasst.

**Ebene A — deterministisch und offline.** `load_or_fetch_with` nimmt `search`
und `download` als Closures; damit ist der gesamte Deezer-Pfad ohne Netz
prüfbar. Die Fixtures sind Abschriften der oben gemessenen realen Antworten,
nicht erfundene Formen. Verlangt werden mindestens:

- *Gestalt „The Devil Wears Prada"*: Platzhalter zuerst, echtes Bild danach →
  die URL des echten Treffers wird heruntergeladen, die Datei liegt im Cache,
  **kein** `.notfound`-Marker entsteht.
- *Gestalt „ONI"*: Platzhalter zuerst, danach vier exakte Treffer mit Bild, der
  populärste zuletzt → gewählt wird der populärste.
- *Alle exakten Treffer sind Platzhalter* → `NotFound` und Marker; die
  Download-Closure paniert, wenn sie doch aufgerufen wird.
- *Nicht exakte Namen gewinnen nie*, auch nicht mit weit höherer Fan-Zahl (der
  „Original West End Cast"-Eintrag ist der reale Beleg dafür).
- *Fehlendes oder `null`-`nb_fan`* → kein Panic, stabile und wiederholbare Wahl.
- *Erkennung*: die Leerstring-MD5 **ist** ein Platzhalter, das leere Segment
  ebenso, eine gewöhnliche 32-stellige Hex-Kennung **nicht**. Der heute
  falschnamige Test darf in seiner jetzigen Form nicht überleben — entweder er
  prüft, was sein Name behauptet, oder er wird nach dem umbenannt, was er prüft.
  Der Fall „leeres Segment" muss dabei erhalten bleiben.

Für jeden dieser Tests gilt die Mutationsdisziplin: **Vor** der Reparatur gegen
die unveränderte Logik laufen lassen und protokollieren, dass er rot ist. Ein
Test, der vorher wie nachher grün ist, beweist nichts. Beim Auswerten zählt nur
die Zeile `test result: FAILED`; ein Namensfilter, der keinen Test trifft, meldet
ebenfalls Erfolg.

**Ebene B — sichtbar in *My Stats*.** Als Vorlage dient die bereits erprobte
Strecke aus `.tmp/stats23-visual/run-accept.sh`: privates Xvfb plus openbox,
`dbus-run-session`, eigene `XDG_DATA_HOME` / `XDG_CACHE_HOME` / `XDG_CONFIG_HOME`
/ `XDG_STATE_HOME` / `XDG_RUNTIME_DIR`, `GDK_BACKEND=x11` mit leerem
`WAYLAND_DISPLAY`, `REPRISE_AUDIO_SINK=fakesink`, Screenshot per `import`.
Verbindlich sind dabei:

1. **Kopie statt Original.** Die Bibliotheksdatenbank wird in die isolierte
   Wurzel kopiert, damit dieselben Top-Ten erscheinen. Die `-wal`- und
   `-shm`-Dateien gehören mitkopiert (oder es wird vorher ein Checkpoint
   erzwungen), sonst zeigt die Kopie einen veralteten Stand. Es wird nie in das
   Original zurückgeschrieben und die laufende App des Nutzers nicht angefasst.
2. **Leerer Porträt-Cache.** Die isolierte `XDG_CACHE_HOME` startet **ohne**
   Porträtverzeichnis. Nur dann läuft der Abruf überhaupt; ein frischer
   Cache-Treffer kurzschließt ihn und die Abnahme misst nichts.
3. **Modul eingeschaltet.** In der kopierten Datenbank muss
   `module.artwork.enabled` gesetzt sein und das globale Online-Quellen-Gate
   Netz erlauben. Ist das nicht der Fall, wird nichts abgerufen, alle Ränge
   zeigen Cover oder Initialen — und das sieht aus wie „behoben". Das ist die
   wahrscheinlichste Falsch-Grün-Falle dieses Vorhabens und muss vor der
   Bewertung positiv belegt werden, etwa über `REPRISE_LOG=info` und den
   Nachweis, dass Porträtanfragen tatsächlich stattfanden.
4. **Zwei Läufe.** Ein *Vorher*-Lauf mit dem unveränderten Binary aus
   `origin/dev` und derselben leeren Cache-Vorbedingung, und ein *Nachher*-Lauf
   mit dem reparierten Binary. Der Vorher-Lauf muss die graue Silhouette
   mindestens bei „The Devil Wears Prada" reproduzieren. Erwartet und
   unproblematisch ist, dass „Oceano" schon im Vorher-Lauf richtig erscheint —
   das ist genau der oben gemessene Befund und kein Widerspruch, sondern der
   Beleg dafür, dass dessen Fehler ausschließlich im Cache saß.

**Orakel der sichtbaren Ebene.**

- In den Rängen 1–10 ist **keine graue Personensilhouette** mehr zu sehen.
- Rang 3 und Rang 10 zeigen eine **Fotografie** — nicht Initialen, nicht ein
  Album-Cover. Für beide Namen hat Deezer nachweislich ein echtes Bild; Initialen
  oder Cover bedeuten, dass der Abruf nicht lief oder nichts fand, und sind ein
  Grund zur Untersuchung, nicht zur Abnahme.
- **Nur Rang 3 belegt die Logik.** Rang 10 („Oceano") kann den Code-Fix
  grundsätzlich nicht beweisen: Deezer liefert dort schon beim ersten exakten
  Treffer ein echtes Bild, der alte Code holt es bei leerem Cache also ebenfalls.
  Rang 10 ist der Beleg dafür, dass sein Fehler **ausschließlich** im Cache saß —
  er gehört in die Abnahme, aber wer ihn als Beweis für E1/E2 wertet, hat nichts
  gemessen. Der sichtbare Beweis der Auswahllogik ist Rang 3 und sonst nichts.
- **Regressionsprüfung:** Die übrigen Ränge zeigen dieselben Gesichter wie im
  Vorher-Lauf. Verändert sich dort etwas, ist die Auswahlregel auf einen
  Namensvetter umgeschwenkt und der Befund gehört in den Bericht.
- **Dateiebene:** Nach dem Lauf enthält das isolierte Porträtverzeichnis eine
  Bilddatei für die Cache-Schlüssel der beiden Namen, und deren Bytes
  unterscheiden sich von den beiden im echten Cache liegenden
  Platzhalter-Varianten. Dieser Byte-Vergleich ist ausdrücklich zulässig — er ist
  einmalige Beweisführung durch einen Menschen, kein ausgelieferter
  Erkennungsmechanismus.

**Ebene C — Gates.** Die Testsuite von `reprise-core`, Clippy und
`scripts/check-frontend-thinness.sh`. Erwartung: Der Eingriff bleibt vollständig
in der portablen Schicht, das GNOME-Frontend wird nicht angefasst, die
Thinness-Budgets bewegen sich nicht. Sollte der Implementierer eine
Frontend-Änderung für nötig halten, ist das ein Zeichen, dass der Entwurf
abgedriftet ist — dann anhalten und melden, statt die Baseline anzuheben.

---

## Umsetzung in Wellen

Grundlage ist `origin/dev`; vorher `git fetch origin dev`, damit nicht auf einem
veralteten Stand aufgesetzt wird. Der Worktree entsteht erst mit der Code-Phase;
im Hauptcheckout wird nicht gebaut.

Der Plan schreibt keine Dateilisten vor. Er benennt lediglich
**Verantwortungsbereiche**, damit die parallelen Stränge sich nicht in dieselbe
Datei schreiben — innerhalb seines Bereichs entscheidet der Implementierer
selbst, was er anfasst.

### Welle 0 — Vorbereitung (kurz, seriell)

Worktree von `origin/dev` anlegen, Wake-Lock für den Lauf nehmen, den heutigen
Cache-Zustand als Ausgangsbefund festhalten (Anzahl, mtimes, die neun
Platzhalter-Dateien). Ausdrücklich **lesend** — im echten Cache wird in dieser
Welle nichts gelöscht.

### Welle 1 — zwei unabhängige Stränge, parallel

**Strang A — Kernlogik und Unit-Beweise.**
Verantwortungsbereich: das Porträt-Modul in `reprise-core` samt seiner Tests.
Inhalt: E1 und E2 umsetzen, `limit` anheben, die Fixtures auf die realen
Antwortformen umstellen, den falschnamigen Test in Ordnung bringen, die Tests aus
Ebene A schreiben — und für jeden vor der Reparatur belegen, dass er rot ist.
Randbedingung: kein GTK, keine Frontend-Typen, keine neue Abhängigkeit.

**Strang B — Abnahmestrecke.**
Verantwortungsbereich: das Abnahmeverzeichnis des Worktrees (Skript, Screenshots,
Protokolle). Kein Rust-Quelltext. Inhalt: die isolierte Umgebung nach E4/Ebene B
aufbauen (DB-Kopie inklusive WAL, leerer Porträt-Cache, Artwork-Modul in der
Kopie eingeschaltet), den *Vorher*-Lauf mit dem unveränderten `origin/dev`-Binary
fahren und die Bildbelege sichern. Gebaut wird dafür im Worktree, nicht im
Hauptcheckout.

Beide Stränge sind echt unabhängig: A fasst nur Rust an, B nur die
Abnahmeumgebung. Sie dürfen sich nicht gegenseitig prüfen — ein Strang, der eine
fremde Datei bewerten müsste, kann nie grün werden.

**Review-Punkt R1.** Zwei Dinge, beide bevor Welle 2 beginnt:
ein Rust-Review über den Diff von Strang A — Determinismus der Sortierung,
Verhalten bei fehlenden oder `null`-Feldern, keine Panics, keine
Frontend-Berührung, und ob die neuen Tests wirklich das prüfen, was ihr Name
behauptet. Und die Kontrolle, dass der Vorher-Lauf aus Strang B die graue
Silhouette tatsächlich zeigt. Tut er das nicht, ist die spätere Abnahme wertlos —
dann anhalten und die Vorbedingung neu herstellen, statt weiterzulaufen.

### Welle 2 — sichtbare Abnahme (seriell, braucht A und B)

Reparierten Build im Worktree erzeugen, die Strecke aus Strang B damit erneut
fahren, alle vier Orakel aus E4 auswerten und die Belege ablegen: Vorher- und
Nachher-Screenshots derselben Ränge, das Listing des isolierten
Porträtverzeichnisses, der Byte-Vergleich gegen die Platzhalter-Varianten, sowie
der Nachweis, dass Porträtanfragen überhaupt liefen.

**Review-Punkt R2.** Ein unabhängiger Blick auf die *Belege*, nicht auf den
Bericht des Implementierers. Berichte sind Behauptungen; hier zählen die Bilder,
das Verzeichnislisting und die Testausgabe.

### Welle 3 — Gates, Landung, Nachsorge

Testsuite, Clippy und das Thinness-Gate fahren; Ausgaben in Dateien umleiten und
gezielt auswerten statt vollständig zurücklesen. PR eröffnen und landen.

Erst **danach** und nur mit ausdrücklicher Freigabe des Nutzers: der einmalige
`rm -rf` auf `~/.cache/reprise/artist-portraits` der echten Installation, mit
dem Hinweis, dass die Porträts anschließend beim nächsten Öffnen von *My Stats*
neu geholt werden und das Artwork-Modul dafür eingeschaltet sein muss.

Ebenfalls in dieser Welle zu **prüfen und zu entscheiden** (nicht blind
auszuführen): ob eine der UX-Regeln in `docs/ux-rules.md` — die STATS-23- und die
Artwork-/Online-Quellen-Regeln sprechen über Porträts — eine Aussage über die
Trefferauswahl enthält, die durch E2 überholt ist. Falls ja, wird sie
mitgezogen; falls nein, bleibt die Regeldatei unangetastet.

---

## Abnahmekriterien

Der Vorgang gilt als erledigt, wenn **alle** Punkte belegt sind:

1. Die Platzhalter-Erkennung greift auf der Bildkennung und erkennt die
   Leerstring-MD5 sowie das leere Segment; eine gewöhnliche Kennung wird nicht
   fälschlich verworfen.
2. Unter mehreren exakten Namenstreffern gewinnt reproduzierbar der populärste
   mit echtem Bild; nicht exakte Namen gewinnen nie.
3. Sind alle exakten Treffer Platzhalter, entsteht ein Negativ-Marker und **kein**
   Download.
4. Kein Test im Modul trägt mehr einen Namen, der etwas anderes behauptet als
   sein Inhalt; die Fixtures bilden reale Deezer-Antworten ab und nageln den
   CDN-Host nicht enger fest als `*.dzcdn.net`.
5. Für jeden neuen Verhaltenstest ist protokolliert, dass er gegen die
   unreparierte Logik rot war.
6. Der Nachher-Screenshot von *My Stats* zeigt in den Rängen 1–10 keine graue
   Silhouette, und auf Rang 3 sowie Rang 10 je eine Fotografie.
7. Für denselben Lauf ist belegt, dass Porträtabrufe tatsächlich stattfanden
   (Modul an, Netz erlaubt) — die Abnahme darf nicht auf einem stillen
   Nicht-Abruf beruhen.
8. Die übrigen Ränge haben ihr Bild gegenüber dem Vorher-Lauf nicht gewechselt,
   oder jede Abweichung ist einzeln begründet.
9. Die echte Bibliothek des Nutzers ist unverändert, seine laufende App wurde
   nicht angefasst, und alle Testinstanzen sind beendet.
10. Testsuite, Clippy und `scripts/check-frontend-thinness.sh` laufen durch, ohne
    dass eine Budget-Baseline angehoben wurde.
11. Es ist **kein** Cache-Invalidierungs-, Epochen- oder Versionsmechanismus
    entstanden.

---

## Risiken

**Die Popularitätsregel trifft den falschen Namensvetter.** Bewusst akzeptiert
und nicht schlechter als der heutige Zufall aus Deezers Reihenfolge. Aufgefangen
wird es durch die Regressionsprüfung über die sichtbaren Ränge in Welle 2; ein
dort auffallender Wechsel gehört in den Bericht statt in die stille Abnahme.

**Deezer führt eine dritte Platzhalter-Kennung ein.** Dann gilt sie als echtes
Bild. Der Schaden ist durch die Auswahlregel stark eingeschränkt (ein Platzhalter
auf einem unbedeutenden Eintrag verdrängt nichts mehr) und durch die 30-Tage-TTL
zeitlich begrenzt. Nachrüsten ist eine Ein-Zeilen-Änderung.

**Falsch-grüne Abnahme durch abgeschaltetes Artwork-Modul.** Die
wahrscheinlichste Fehlerquelle des ganzen Vorhabens: Voreinstellung ist *aus*,
und ohne Abruf sehen alle Ränge unauffällig aus. Deshalb ist der positive
Nachweis stattgefundener Abrufe eigenes Abnahmekriterium.

**Deezer nicht erreichbar oder drosselt während der Abnahme.** Dann beweist der
Lauf nichts. „Kein Abruf gelaufen" und „Abruf lief, fand nichts" müssen im
Protokoll unterscheidbar sein, sonst wird ein Netzproblem als Fix verbucht.

**Der Zeitpunkt der Landung.** Verfällt der Cache (17./18.08.2026), bevor der Fix
läuft, holt der alte Code für „The Devil Wears Prada" erneut den Platzhalter und
friert ihn bis Mitte September ein. Genau deshalb hängt die Reparatur an der
manuellen Löschung aus E3 und nicht am Kalender.

**Der Stale-Fallback verlängert die Vergiftung.** `stale_or` behält ein
abgelaufenes Cache-Bild, wenn die Auffrischung scheitert. Ein vergifteter Eintrag
überlebt damit jeden fehlgeschlagenen Netzabruf. Das ist gewolltes Verhalten und
wird hier **nicht** geändert — es ist aber der Grund, warum die einmalige
Löschung der zuverlässigere Hebel ist als jedes Warten auf Ablauf.

**Kosten der Cache-Löschung.** 150 gesunde Einträge werden neu geholt, faul und
nur für angezeigte Künstler, je eine Suche plus ein Download im Abstand von
mindestens 300 ms auf einem Worker. Angesichts der ohnehin fälligen
Komplettauffrischung ist das vernachlässigbar.

**Fixture-Drift.** Deezer hat den CDN-Host bereits gewechselt
(`e-cdns-images.dzcdn.net` → `cdn-images.dzcdn.net`). Tests dürfen sich nicht an
konkrete Hostnamen binden, sonst brechen sie beim nächsten Wechsel, ohne dass
sich am Verhalten etwas geändert hätte.

---

## Ausdrücklich nicht Gegenstand

Keine Änderung an den TTLs. Keine Herkunfts- oder Metadatenspeicherung im
Porträt-Cache. Keine Änderung an der Album-Cover-Kette oder an den Initialen.
Keine Frontend-Änderung, keine Android-Änderung. Keine Paginierung der
Deezer-Suche. Kein zusätzlicher Anbieter neben Deezer.
