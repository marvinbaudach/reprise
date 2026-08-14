---
slug: portrait-placeholder-fingerprint
worktree: /home/marvin/Projects/reprise-portrait-placeholder-fingerprint
branch: feature/portrait-placeholder-fingerprint
phase: reviewed
codex_session:
created: 2026-08-14
---
# Platzhalter erkennt man am Bild, nicht an der Kennung

## Problem und Ursache

`docs/plans/deezer-placeholder-portraits.md` (gelandet als #469) erkennt Deezers
graue Silhouette an der **Bildkennung** im URL-Pfad
(`…/images/artist/<kennung>/<größe>-…jpg`) gegen die feste Liste
`MISSING_IMAGE_IDENTIFIERS`. E1 jenes Plans begründete das damit, dass die eine
Kennung strukturell erzwungen ist — `d41d8cd98f00b204e9800998ecf8427e` ist die
MD5 des leeren Strings, die Deezer emittiert, wenn *kein* Bild existiert — und
rechnete eine dritte Kennung als seltenes Ereignis ab.

**Die Prämisse ist widerlegt, und zwar breit.** Eine Messung über die gesamte
Bibliothek am 14.08.2026 (195 Künstler, 238 abgerufene Kandidatenbilder) findet
**18 Platzhalter-Instanzen**. Zehn laufen über die
Leerstring-MD5, die der gelandete Code kennt. **Acht laufen über gewöhnliche,
künstlerspezifische Kennungen**, die er nicht kennt:

| Kennung | Künstler | Abstand zur nächsten Referenz (32×32) |
|---|---|---|
| `895abde025e12b5703f7e5c75cc63d6c` | Aetheriality | 0,00000 |
| `05c328e5c382f929e1c4d392858cd2b9` | Caliban | 0,00000 |
| `790f849972c0966b9494944b5ef513f6` | In Your Grave | 0,00017 |
| `5dbfc32ce4dc3ef5dbd9904721d6b846` | Our Vices | 0,00017 |
| `8cbdfabc63757997f761091d55a10c0c` | Shiva | 0,00017 |
| `37d44076a187d4fd03c2e60d80c359f8` | The Narrator | 0,00017 |
| `e02c0c8d515882697bd7711cdd1bb30e` | Wake Me | 0,00017 |
| `415714b66a5de709809dd3d05f58afe4` | Oceano | 0 (zweite Referenzzeichnung) |

**Dieselbe Zeichnung kommt nicht als dieselben Bytes.** Zwei Sweeps im Abstand
von zweieinhalb Stunden liefern für dieselben Kennungen mal 0,00000 und mal
0,00017 — Deezer kodiert je Abruf minimal anders. Der Byte-Hash scheitert daran,
die Miniatur nicht: die Marge blieb in beiden Läufen bei 330×.

Die Kennungsliste bräuchte für **eine** Bibliothek heute zehn Einträge statt
zwei, und sie wüchse weiter mit jedem neuen Künstler und jeder Umstellung durch
Deezer — bei „Oceano" nachweislich innerhalb eines Tages (13.08. echtes Foto,
14.08. Silhouette). Ein Fingerabdruck des Bildes wächst dagegen **pro
Platzhalter-Zeichnung** — zwei sind bekannt — und deckt mit jeder Zeile alle
Künstler gleichzeitig.

### Was die Messung sonst noch ergab

Rohdaten und Skripte: `docs/evidence/portrait-placeholder-fingerprint/`.

**Der erste Entwurf dieses Plans war falsch.** Er stand auf 22 Bildern der
Ränge 1–18 und schlug 16×16-Miniaturen mit Schwelle 0,15 vor. Die breite Messung
zeigt, warum das nicht trägt: die Spitzenkünstler haben durchweg dunkle,
kontrastreiche Pressefotos, helle und flächige Cover sitzen im Schwanz der
Verteilung. Das nächste echte Bild („Currents", ein Teller mit Besteck auf hellem
Grund) liegt bei 0,0507 — praktisch auf dem Abstand, den die beiden bekannten
Silhouetten-Fassungen *zueinander* haben (0,0493). Schwelle 0,15 hätte **sieben
echte Cover verworfen**, darunter eine Totenkopf-Illustration und ein
Vinyl-Label.

Die richtige Trennung liegt zwei Größenordnungen tiefer, weil Deezer die
Silhouette pro Zeichnung **byte-nahezu identisch** ausliefert:

| Auflösung | schlechtester Platzhalter | nächstes echtes Bild | Verhältnis |
|---|---|---|---|
| 16×16 | 0,00042 | 0,0507 | 119× |
| **32×32** | **0,00017** | **0,0573** | **330×** |
| 64×64 | 0,00033 | 0,0605 | 183× |

**Verworfene Alternativen, mit Zahlen.**

- **dHash**: 16 Bit zwischen zwei Fassungen desselben Motivs, 21 Bit zum nächsten
  echten Foto. Ein Gradienten-Hash auf einer fast flachen Fläche misst
  JPEG-Rauschen. Unbrauchbar.
- **Farbanzahl / Flachheit**: die Silhouetten haben 213 und 236 eindeutige Farben
  bei 1000×1000 — „Falling in Reverse" hat 256 und ist ein echtes Bandfoto.
  Fehlalarm bereits in der ersten Stichprobe.
- **SHA-256 der Bytes**: hätte in jedem Sweep die Mehrheit der Instanzen verfehlt
  — fünf bis sieben der acht liegen bei 0,00017 statt 0,00000, also dieselbe
  Zeichnung minimal anders komprimiert, und *welche* das sind, wechselt zwischen
  zwei Abrufen. Die Miniatur erkennt sie alle mit 30-fachem Sicherheitsabstand.

---

## Entscheidungen mit Begründung

### E0 — Welcher Fehler der teure ist

**Beschluss.** Abgesichert wird gegen den **Fehlalarm**: ein fälschlich
verworfenes echtes Bild wiegt schwerer als ein durchgelassener Platzhalter.

**Begründung.** Ein verworfenes echtes Bild ist unsichtbar — niemand meldet „hier
fehlt ein Bild, das es gäbe", und der Fehler bleibt dauerhaft. Ein durchgelassener
Platzhalter fällt beim nächsten Blick auf *My Stats* auf und kostet eine Zeile.
Alle folgenden Beschlüsse fallen im Zweifel zugunsten des echten Bildes; wo dieser
Plan eine Wahl trifft, ist das der Grund.

### E1 — Ablehnung am heruntergeladenen Bild, gegen hinterlegte Referenz-Miniaturen

**Beschluss.** Nach dem Download und **vor** dem Schreiben in den
Zwischenspeicher wird das Bild auf **32×32 Graustufen** verkleinert und per
normalisiertem RMSE gegen jede hinterlegte Referenz-Miniatur verglichen. Liegt
der kleinste Abstand bei oder unter `PLACEHOLDER_RMSE_MAX`, gilt das Bild als
Platzhalter. Der Wert steht seit der Rust-Messung vom 14.08. fest:
`PLACEHOLDER_RMSE_MAX = 0.0025` (Herleitung in E6).

**Warum das kein verbotenes Raten ist.** Der alte E1 verbot „inhaltsbasierte"
Erkennung, weil jede plausible Heuristik über *Bildeigenschaften* geurteilt hätte
— flach, einfarbig, wenige Farben. Das hier urteilt über nichts: es fragt nicht
„sieht das langweilig aus?", sondern „ist das exakt dieses Bild?". Bei 0,005
gegen ein nächstes echtes Bild bei 0,0573 ist das eine Identitätsprüfung, keine
Heuristik. Die Messung zeigt zudem, dass die verbotene Heuristik tatsächlich
danebengegriffen hätte (Falling in Reverse).

**Warum 32×32 Graustufen.** Größenunabhängig (funktioniert für `picture_xl` wie
`picture_big`), unempfindlich gegen Neukodierung — genau das, woran Kennungsliste
und Byte-Hash scheitern —, beste gemessene Marge der drei Auflösungen, und
1024 Byte pro Referenz, die als `const`-Array im Code stehen. Keine neue
Abhängigkeit: `image = "0.25"` ist in `reprise-core` bereits vorhanden.

**Referenzen im Code.** Genau zwei — die beiden bekannten Zeichnungen. Pro
Referenz ein `const`-Array mit Kommentar: Kennung, Datum der Beobachtung,
Herkunft. Erzeugt vom Rust-Generator aus E6, damit dieselbe Verkleinerung und
dieselbe Luma-Formel benutzt werden wie zur Laufzeit.

**Deezers Bilder werden nicht eingecheckt.** Nur die abgeleiteten Miniaturen
(1 KB, kein wiedererkennbares Bild).

### E2 — Kein Rückfall auf den nächsten Kandidaten

**Beschluss.** Ein Download pro Künstler. Wird das Bild abgelehnt, bekommt der
Künstler **kein Porträt** — Initialen. Es wird kein zweiter Kandidat probiert.
`parse_best_artist` bleibt wie es ist; es gibt keine Kandidatenliste, keine
Schleife, kein `MAX_PORTRAIT_CANDIDATES`.

**Begründung, gemessen.** Der Entwurf hatte den Rückfall empfohlen. Die Messung
zeigt, was er kauft: von 195 Künstlern haben 8 einen Platzhalter unter
gewöhnlicher Kennung, bei 5 sitzt er auf Platz 1 — und von diesen 5 hat genau
**einer** überhaupt einen bebilderten Kandidaten: Oceano (Platzhalter mit 16 388
Fans, Namensvetter mit 2 347). Die anderen vier haben jeweils einen einzigen
Kandidaten und enden ohnehin bei Initialen. Der ganze Apparat kauft ein einziges
Bild, und dieses Bild ist das Gesicht eines Fremden — nach E0 der teure, weil
unsichtbare Fehler. Initialen sind ehrlich.

**Folge.** Rang 10 „Oceano" zeigt künftig Initialen statt des
Namensvetter-Fotos, das seit #469 dort steht. Das ist eine beabsichtigte
Verhaltensänderung, keine Regression.

### E3 — `MISSING_IMAGE_IDENTIFIERS` schrumpft auf die zwei strukturellen Einträge

**Beschluss.** In der Liste bleiben genau das leere Pfadsegment und
`d41d8cd98f00b204e9800998ecf8427e`. `415714b66a5de709809dd3d05f58afe4` wird
**entfernt**. Die Liste behält ihre heutige Rolle in der Auswahl: ein Kandidat
mit einer dieser Kennungen gilt als bildlos und verdrängt keinen bebilderten
Namensvetter.

**Begründung.** Die beiden verbliebenen Einträge heißen wörtlich „Deezer hat
kein Bild" — das weiß man vor jedem Download, ohne ein Byte zu sehen, und es
spart den Abruf (10 von 195 Künstlern in dieser Bibliothek). Eine
künstlerspezifische Kennung dagegen heißt „Deezer hat ein Bild", und ob es die
Silhouette ist, entscheidet ab jetzt ausschließlich der Fingerabdruck. Deshalb
fliegt `415714b6…` raus — zumal Deezer dort am 13.08. noch ein echtes Foto
lieferte und es jederzeit wieder tun kann; der Fingerabdruck folgt dem, die
Liste nicht.

**Die Liste wächst nie wieder.** Neue Kennungen von Hand nachzutragen ist ab
jetzt falsch. Wächst sie doch, ist das der Beweis, dass der Fingerabdruck
versagt.

### E4 — Eine Ablehnung darf kein vorhandenes echtes Bild vernichten

**Beschluss.** Wird das Bild abgelehnt und liegt bereits ein — auch veraltetes —
Porträt im Zwischenspeicher, bleibt dieses Bild erhalten und sein Zeitstempel
wird erneuert; die App zeigt es weiter und fragt für weitere 30 Tage nicht nach.
Nur wenn **kein** Bild vorliegt, wird die Negativ-Marke geschrieben.

**Begründung.** `cache::write_negative` löscht heute alle Bilddateien des
Künstlers (`cache.rs:92-94`). Ohne diesen Beschluss verlöre jeder Künstler, den
Deezer auf die Silhouette umstellt, sein seit einem Monat korrekt angezeigtes
Foto — unsichtbar und endgültig, alle 7 Tage neu versucht. Das ist exakt der
Fehler aus E0. Die Auffrischung heißt semantisch „nachgesehen, Deezer hat nichts
Besseres" und bringt die Wiederholungsbremse gratis aus der bestehenden TTL mit.

**Umsetzung ohne neue Abhängigkeit.** Die zwischengespeicherten Bytes lesen und
erneut durch `cache::store_image` schreiben — der Pfad ist atomar, entfernt eine
etwaige Negativ-Marke und erneuert den Zeitstempel. Kein `filetime`, kein
`utimes`.

### E5 — Beide Fehlerrichtungen werden sichtbar

**Beschluss.** `artist_portrait` bekommt zwei WARN-Zeilen:

1. bei jeder Ablehnung — Künstlername, Bildkennung, gemessener Abstand;
2. wenn ein **angenommenes** Bild im Graubereich zwischen der Schwelle und 0,05
   liegt.

**Begründung.** Nach E0 ist der unsichtbare Fehler der teure, und der
Fingerabdruck kann in beide Richtungen still versagen: greift er nicht mehr
(Deezer zeichnet neu), sieht niemand etwas; rückt ein echtes Cover näher an die
Referenz, verschwindet es kommentarlos. Der Graubereich ist heute **leer** — das
nächste echte Bild liegt bei 0,0573 —, eine Meldung von dort ist also kein
Rauschen, sondern das Signal, dass die Grundlage der Schwelle sich verschoben
hat. Kosten: ein paar Zeilen pro TTL. Das Modul loggt heute nichts, `reprise-core`
sonst an 113 Stellen.

### E6 — Die Schwelle wird mit dem Rust-Code festgelegt, nicht mit der Sonde

**Beschluss.** Ein dev-only Einstieg (`#[ignore]`-Test in `reprise-core`, Korpus
und Ausgabepfad über Umgebungsvariablen) läuft mit der **ausgelieferten
Implementierung** über das lokal liegende Bildkorpus und gibt je Bild Kennung und
Abstand aus. Aus diesen Zahlen wird `PLACEHOLDER_RMSE_MAX` gesetzt. Derselbe
Einstieg erzeugt die Referenz-Miniaturen. Vorbild für Form und Begründungstext
ist `library/library_doctor/remote/diagnostics.rs:12`
(`#[ignore = "reads a real database through a read-only URI and contacts
MusicBrainz"]`) — dieselbe Sorte umgebungsabhängiger Messeinstieg, die das
Repository bereits kennt.

**Begründung.** Die Vorerkundung lief mit PIL: LANCZOS und Rec. 601
(`0.299/0.587/0.114`). Die `image`-Kiste rechnet Graustufen mit Rec. 709
(`0.2126/0.7152/0.0722`) und hat einen anderen Lanczos-Kern. Beides verschiebt
jede Miniatur und damit jeden Abstand. Bei Faktor 330 überlebt die Trennung das
sehr wahrscheinlich — „sehr wahrscheinlich" ist aber keine Messung, und die
Referenzen müssen ohnehin mit dem Code entstehen, der sie später vergleicht.

**Margenregel, korrigiert am 14.08.** Die ursprüngliche Fassung verlangte
beidseitig 20× und war arithmetisch unerfüllbar: das setzt 400× Gesamttrennung
voraus, gemessen sind 241,8×. Der erste Codex-Lauf hat deshalb korrekt angehalten
(Protokoll unten). Die Regel lautet jetzt, nach E0 asymmetrisch:

- mindestens **10×** über dem schlechtesten bekannten Platzhalter,
- mindestens **20×** unter dem nächsten echten Bild.

Der größere Abstand gehört auf die Seite der echten Bilder, weil ein verworfenes
Cover unsichtbar ist und ein durchgelassener Platzhalter nicht. Aus den Rust-Zahlen
(0,000245098 / 0,059268005) folgt **0,0025** — 10,2× beziehungsweise 23,7×.

Die Regel bleibt eine Latte, keine Formel: wird sie bei einer künftigen Messung
verfehlt, hält der Lauf wieder an und die Entscheidung fällt neu. Eine Schwelle,
die nachträglich passend gelegt wird, misst nichts mehr.

**Korpus.** Die 238 Bilder liegen außerhalb des Repositories
(`~/.cache/reprise-portrait-corpus/`, per Skript aus
`docs/evidence/portrait-placeholder-fingerprint/` reproduzierbar). Eingecheckt
werden die **Messergebnisse**, nicht Deezers Bytes.

---

## Umsetzung in Wellen

### Welle 0 — Korpus und Referenzen

Bildkorpus an den stabilen Pfad bringen (Skript aus der Evidenz, holt fehlende
Bilder nach). Rust-Einstieg aus E6 bauen, Referenz-Miniaturen erzeugen, ins
Modul übernehmen.

### Welle 1 — Erkennung

Neues Modul `crates/reprise-core/src/artist_portrait/placeholder.rs`: Referenzen,
Schwelle, `fn placeholder_distance(bytes: &[u8]) -> Option<f64>` plus
`fn looks_like_placeholder(bytes: &[u8]) -> bool`. Unit-Tests mit synthetischen
Bildern: eine hochskalierte Referenz muss abgelehnt, ein deutlich anderes Muster
angenommen, ein nicht dekodierbares Bild unverändert durchgereicht werden (die
vorhandene `validated_image_extension`-Prüfung bleibt zuständig).

### Welle 2 — Einbau in den Abrufpfad

In `load_or_fetch_with` nach dem Download und vor `store_image`: Abstand messen,
bei Treffer nach E4 verfahren (vorhandenes Bild auffrischen, sonst Negativ-Marke),
WARN-Zeilen nach E5. `415714b6…` aus `MISSING_IMAGE_IDENTIFIERS` entfernen (E3).
Tests für beide Zweige von E4.

### Welle 3 — Messung, Abnahme, Landung

E6-Lauf über das Korpus, Schwelle setzen, Ergebnis als Evidenz. Orakel in
`acceptance/deezer-placeholder-portraits/run-accept.sh:789` korrigieren: Rang 10
zeigt künftig Initialen, nicht ein Foto (E2) — mit Begründung im Skript. Gates
(`fmt`, `clippy`, `cargo test -p reprise-core`, Core-Purity). Landen wie üblich.

---

## Abnahmekriterien

1. Der Rust-Lauf über das Korpus zeigt mindestens 10× Marge über dem
   schlechtesten Platzhalter und mindestens 20× unter dem nächsten echten Bild;
   die gesetzte Schwelle liegt in diesem Fenster. Wird das verfehlt, hält die
   Arbeit an (E6).
2. Alle 18 gemessenen Platzhalter-Instanzen werden abgelehnt, gemessen mit dem
   Rust-Code.
3. Kein echtes Bild der 219 gemessenen wird abgelehnt — insbesondere nicht
   „Currents" (0,0573, das nächste), „Falling in Reverse" (256 Farben, echtes
   Foto), „Signs Of Humanity" und „Shiva" (helle Illustrationen).
4. Ein Künstler, dessen einziger Kandidat die Silhouette trägt, bekommt genau
   eine Negativ-Marke und kein Bild — auch wenn seine Kennung **nicht** in
   `MISSING_IMAGE_IDENTIFIERS` steht. Das ist der Kern des Plans.
5. Liegt ein veraltetes echtes Bild vor und werden alle Kandidaten abgelehnt,
   bleibt die Bilddatei bestehen und ihr Zeitstempel ist erneuert; es entsteht
   **keine** Negativ-Marke (E4).
6. Genau ein Bildabruf pro Künstler, nachgewiesen über die gezählten
   Download-Aufrufe im Test (E2).
7. Die WARN-Zeilen aus E5 erscheinen im vorgesehenen Fall und sonst nicht.
8. Die Abnahmestrecke aus #469 läuft mit korrigiertem Orakel durch: Rang 3 zeigt
   ein Foto, Rang 10 Initialen, die übrigen Ränge unverändert.
9. Jeder neue Test war vor seiner Implementierung rot, mit Beleg.

---

## Risiken

**Die Messung kennt nur eine Bibliothek.** 195 Künstler, 238 Bilder, ein Genre.
Die Trennung ist mit Faktor 330 groß genug, dass ein einzelnes ungewöhnliches
Cover sie nicht schließt — aber ein anderer Musikgeschmack ist nicht vermessen.
Gegenmittel: E5 meldet den Graubereich, bevor daraus ein Fehlalarm wird.

**Deezer zeichnet die Silhouette neu.** Dann greift der Fingerabdruck nicht mehr
und es braucht eine dritte Referenz — eine Zeile, die dann alle Künstler auf
einmal abdeckt. Genau dieser Unterschied zur Kennungsliste ist der Zweck des
Plans. Sichtbar wird es über E5, sobald ein Bild im Graubereich landet.

**Vier Künstler verlieren nichts, aber Oceano schon.** Aetheriality, In Your
Grave, Our Vices und Wake Me zeigen heute die Silhouette und künftig Initialen —
eine Verbesserung. Oceano zeigt heute das Foto eines Namensvetters und künftig
Initialen. Wer das für einen Rückschritt hält, muss E2 kippen, nicht den
Fingerabdruck.

**Der Umbau bleibt im Modul.** Geprüft am 14.08.: neun Fundstellen von
`parse_best_artist`, alle unter `crates/reprise-core/src/artist_portrait/`, davon
acht in Tests. Kein Aufrufer außerhalb.

---

## Ausdrücklich nicht Gegenstand

- Kein Erkennen *unbekannter* Platzhalter über Bildeigenschaften (flach, wenige
  Farben). Die Messung zeigt, warum: „Falling in Reverse" wäre ein Fehlalarm.
- Keine Kandidatenliste, keine Mehrfach-Downloads, keine Änderung der
  Auswahlordnung aus E2 des Vorgängerplans.
- Kein Löschen bestehender Zwischenspeicher-Einträge im Code. Der Ordner wurde am
  14.08. einmalig von Hand geleert.
- Kein GNOME-Code. Die Änderung ist vollständig in `reprise-core`.

---

## Protokoll: E6-Abbruch vom 14.08.2026 (erledigt)

Der vorgeschriebene Rust-Lauf über alle 237 messbaren Korpusinstanzen hat die
20-fache Marge auf beiden Seiten **verfehlt**. Der schlechteste Platzhalter liegt
bei `0,000245098`, das nächste echte Bild bei `0,059268005`; die Gesamttrennung
beträgt `241,813×`. Damit müsste eine zulässige Schwelle zugleich mindestens
`0,004901961` und höchstens `0,002963400` sein. Dieses Intervall ist leer.

Die vorläufige Schwelle `0,005` liegt `20,400×` über dem schlechtesten
Platzhalter, aber nur `11,854×` unter dem nächsten echten Bild. Gemäß E6 wurde
sie nicht nachträglich passend gelegt. Fingerabdruck und Messeinstieg bleiben
deshalb ausschließlich testgebunden; es gibt keine ausgelieferte Schwelle. Der
Einbau aus Welle 2 und die nachfolgenden Abnahmeschritte aus Welle 3 wurden nicht
umgesetzt. Die Margenregel wurde daraufhin korrigiert (siehe E6); die Schwelle steht jetzt
bei 0,0025 und die Wellen 2 und 3 sind wieder freigegeben.
Der vollständige Rust-Lauf einschließlich aller Einzelabstände steht unter
`docs/evidence/portrait-placeholder-fingerprint/rust-separation.txt`.
