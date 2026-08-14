---
slug: portrait-placeholder-fingerprint
worktree: /home/marvin/Projects/reprise-portrait-placeholder-fingerprint
branch: feature/portrait-placeholder-fingerprint
phase: planned
codex_session:
created: 2026-08-14
---
# Platzhalter erkennt man am Bild, nicht an der Kennung

## Problem und Ursache

`docs/plans/deezer-placeholder-portraits.md` (gelandet als #469) erkennt Deezers
graue Silhouette an der **Bildkennung** im URL-Pfad
(`…/images/artist/<kennung>/<größe>-…jpg`) gegen eine feste Liste
`MISSING_IMAGE_IDENTIFIERS`. E1 jenes Plans begründete das damit, dass die eine
Kennung strukturell erzwungen ist — `d41d8cd98f00b204e9800998ecf8427e` ist die
MD5 des leeren Strings, die Deezer emittiert, wenn *kein* Bild existiert — und
rechnete eine dritte Kennung als seltenes Ereignis ab.

**Die Prämisse ist widerlegt.** Am 14.08.2026 fand der Abnahmelauf dieselbe
Silhouette unter `415714b66a5de709809dd3d05f58afe4`, einer gewöhnlichen
künstlerspezifischen Kennung des populärsten „Oceano"-Eintrags (16 388 Fans) —
und dieselbe Kennung war am 13.08. noch ein echtes Foto. Deezer tauscht das Bild
unter stabiler Kennung aus.

Damit greift keines der drei Auffangnetze aus E1 im beobachteten Fall:

1. Die Auswahlregel (E2) rettet nur, wenn der Platzhalter auf dem
   *unbedeutenden* Doppelgänger sitzt. Bei „Oceano" sitzt er auf dem populären.
2. „Eine Zeile nachtragen" setzt voraus, dass ein Mensch das falsche Gesicht
   bemerkt. Niemand sieht sich 200 Porträts an.
3. Die 30-Tage-TTL begrenzt nichts: beim Ablauf wird derselbe Platzhalter erneut
   geholt.

Die Liste wächst also **pro betroffenem Künstler**, unbemerkt, mit der
Bibliothek. Ein Fingerabdruck des Bildes wächst dagegen **pro
Platzhalter-Variante** — drei sind in einem Jahr bekannt geworden — und deckt
mit jeder Zeile alle Künstler gleichzeitig.

### Frisch nachgemessen (14.08.2026, live gegen `api.deezer.com`)

Grundgesamtheit: alle exakten Namenstreffer der Ränge 1–18 dieser Bibliothek,
22 Kandidatenbilder, plus die zwei bekannten Silhouetten-Varianten als Referenz.
Jedes Bild auf 16×16 Graustufen verkleinert, verglichen per normalisiertem RMSE
(0…1) und per aHash-Hammingabstand. Rohdaten:
`docs/evidence/portrait-placeholder-fingerprint/`.

| Vergleich | RMSE 16×16 | aHash | dHash |
|---|---|---|---|
| Silhouette ↔ Silhouette (die beiden bekannten Varianten) | **0,049** | 2 | 16 |
| nächstes echtes Foto (Asking Alexandria) | **0,311** | 17 | 21 |
| entferntestes echtes Foto (Oceano, der Namensvetter) | 0,830 | 45 | — |

Die beiden Populationen liegen bei RMSE um Faktor 6 auseinander und überlappen
nirgends. Das ist die ganze Begründung für den Mechanismus; alles Weitere folgt
daraus.

**dHash fällt durch.** 16 Bit zwischen den beiden Varianten desselben Motivs,
21 Bit zum nächsten echten Foto — ein Schwellenwert müsste zwischen 16 und 21
liegen. Der Grund ist strukturell: ein Gradienten-Hash auf einer fast flachen
Fläche misst JPEG-Rauschen, nicht Bildinhalt. Nicht verwendbar.

**Flachheit allein fällt ebenfalls durch.** Die Silhouetten haben 213 bzw. 236
eindeutige Farben bei 1000×1000 — aber „Falling in Reverse" hat 256 und ist ein
echtes Bandfoto (RMSE 0,74). Eine Erkennung an der Farbanzahl hätte in dieser
Stichprobe von 22 Bildern bereits einen Fehlalarm.

---

## Entscheidungen mit Begründung

### E1 — Ablehnung am heruntergeladenen Bild, gegen hinterlegte Referenz-Miniaturen

**Beschluss.** Dieser Beschluss löst E1 aus `deezer-placeholder-portraits.md`
ab. Nach dem Download und **vor** dem Schreiben in den Zwischenspeicher wird das
Bild auf 16×16 Graustufen verkleinert und per normalisiertem RMSE gegen jede
hinterlegte Referenz-Miniatur verglichen. Liegt der kleinste Abstand bei oder
unter `PLACEHOLDER_RMSE_MAX = 0.15`, gilt das Bild als Platzhalter.

**Warum das kein verbotenes Raten ist.** Der alte E1 verbot „inhaltsbasierte"
Erkennung, weil jede plausible Heuristik eine Heuristik *über Bildeigenschaften*
gewesen wäre — flach, einfarbig, wenige Farben. Das hier ist keine: es ist ein
Abgleich gegen ein konkretes, benanntes Motiv, das wir besitzen. Die Frage lautet
nicht „sieht das langweilig aus?", sondern „ist das dieses Bild?". Der gemessene
Abstand entscheidet, nicht ein Urteil über Bildqualität.

**Warum 0,15.** Drei Mal über dem Abstand zwischen den bekannten Varianten
(0,049), zwei Mal unter dem nächsten echten Foto (0,311); das geometrische Mittel
der beiden Populationen liegt bei 0,12. Die Schwelle ist eine benannte Konstante
mit genau dieser Herleitung im Kommentar.

**Warum 16×16 Graustufen.** Größenunabhängig (funktioniert für `picture_xl` wie
`picture_big`), unempfindlich gegen Neukodierung — genau das, woran die
Kennungsliste scheitert —, und 256 Byte pro Referenz, die als `const`-Array im
Code stehen können. Keine neue Abhängigkeit: `image = "0.25"` ist in
`reprise-core` bereits vorhanden.

**Warum nicht SHA-256 der Bytes.** Blind gegen Neukodierung. Von demselben Motiv
sind bereits drei byteverschiedene Fassungen bekannt; jede Neukodierung durch
Deezer bräuchte wieder eine neue Zeile.

**Referenzen im Code.** Pro bekannter Variante ein `const`-Array mit Kommentar:
Kennung, Datum der Beobachtung, Herkunft. Erzeugt vom mitgelieferten Generator
(siehe E4), damit dieselbe Verkleinerung wie zur Laufzeit benutzt wird — ein
anderer Filter verschiebt die Miniatur und damit den Abstand.

**Die Bilder selbst werden nicht eingecheckt.** Nur die abgeleiteten
Miniaturen (256 Byte, kein wiedererkennbares Bild). Deezers Bytes gehören nicht
ins Repository.

### E2 — Was nach einer Ablehnung passiert: der nächste Kandidat, nicht sofort Initialen

**Beschluss.** `parse_best_artist` liefert künftig die **rangsortierte Liste**
der exakten Namenstreffer statt eines einzelnen Treffers. `load_or_fetch_with`
lädt den besten Kandidaten; wird das Bild als Platzhalter abgelehnt, versucht es
den nächsten — höchstens `MAX_PORTRAIT_CANDIDATES = 3` Downloads pro Künstler.
Erst wenn alle Kandidaten abgelehnt sind, wird die Negativ-Marke geschrieben.

**Begründung.** Ohne diesen Rückfall hinge das Ergebnis daran, ob jemand die
Kennung schon nachgetragen hat: bei bekannter Kennung überspringt die Auswahl den
Eintrag schon *vor* dem Download und der Namensvetter mit echtem Foto gewinnt
(so verhält sich „Oceano" heute); bei unbekannter Kennung gäbe es Initialen. Zwei
verschiedene Ergebnisse für dieselbe Lage, abhängig von einem Listeneintrag. Mit
dem Rückfall sind beide Wege identisch — die Kennungsliste wird damit zur reinen
Ersparnis und trägt keine Semantik mehr.

**Kosten.** Höchstens zwei zusätzliche Bildabrufe, und nur für Künstler, deren
bester Kandidat tatsächlich die Silhouette trägt. Die Ratenbremse
(`MIN_REQUEST_INTERVAL`, 300 ms) gilt weiter.

**Bekannte Nebenwirkung, unverändert übernommen.** Trägt der populärste Eintrag
den Platzhalter und ein Namensvetter ein echtes Foto, zeigt die App das Gesicht
des Namensvetters (Rang 10 „Oceano"). Das folgt aus E2 des Vorgängerplans und
bleibt ausdrücklich so; dieser Plan ändert daran nichts, er macht das Verhalten
nur unabhängig vom Stand der Kennungsliste.

### E3 — Die Kennungsliste bleibt, als Abkürzung

**Beschluss.** `MISSING_IMAGE_IDENTIFIERS` bleibt erhalten und wirkt weiterhin
vor dem Download.

**Begründung.** Für die strukturell erzwungene Leerstring-MD5 ist sie exakt und
spart den Abruf vollständig — die einzige Form, bei der man ohne Bytes sicher
sein kann. Sie ist ab jetzt aber nur noch eine Optimierung: mit E2 führt sie zum
selben Ergebnis wie der Fingerabdruck. Neue Kennungen von Hand nachzutragen ist
damit **nicht mehr nötig** und soll unterbleiben; wächst die Liste weiter, ist
das ein Zeichen, dass der Fingerabdruck nicht greift.

### E4 — Beweisführung

**Unit-Tests (ohne Netz, deterministisch).** Aus einer Referenz-Miniatur wird ein
synthetisches Bild hochskaliert — es muss abgelehnt werden. Ein synthetisches
Bild mit deutlich anderem Muster muss angenommen werden. Ein Bild, das nicht
dekodierbar ist, darf den Pfad nicht ändern (fällt weiter durch die vorhandene
`validated_image_extension`-Prüfung). Ein Suchergebnis, dessen bester Kandidat
abgelehnt wird und dessen zweiter ein echtes Bild trägt, muss das zweite Bild
zwischenspeichern; sind alle drei Platzhalter, muss genau eine Negativ-Marke und
kein Bild entstehen. Jeder Test zuerst rot, Beleg als `red-*.txt`.

**Trennschärfe live (einmalig, mit Netz).** Ein Skript holt die zwei bekannten
Varianten und mindestens 20 echte Porträts der Ränge 1–20 und misst die Abstände
**mit dem Rust-Code**, nicht mit dem Python-Vorlauf. Kriterium: beide Varianten
≤ 0,06, alle echten Fotos ≥ 0,25. Ergebnis als Evidenz. Das ist der eigentliche
Beweis, dass die Schwelle sitzt — die Unit-Tests prüfen nur den Mechanismus.

**Sichtbare Abnahme.** Die Strecke aus #469
(`acceptance/deezer-placeholder-portraits/run-accept.sh`) läuft unverändert und
muss dasselbe Ergebnis liefern wie dort: Ränge 1–20 unverändert, außer den
beiden beabsichtigten Korrekturen. Sie ist hier eine Regressionsprüfung, kein
neuer Beweis. Sie braucht einen freien Sockelpfad (< 107 Byte) und den
`test-fixtures`-Build.

---

## Umsetzung in Wellen

### Welle 0 — Vorbereitung

Generator schreiben (`scripts/dev/portrait-placeholder-reference.rs` oder als
`#[ignore]`-Test in `reprise-core`), der eine Bildkennung entgegennimmt, das Bild
holt, auf 16×16 Grau verkleinert und das Rust-`const` ausgibt. Für beide
bekannten Kennungen ausführen, Ausgabe in den Code übernehmen.

### Welle 1 — zwei unabhängige Stränge, parallel

**Strang A — Erkennung.** Neues Modul
`crates/reprise-core/src/artist_portrait/placeholder.rs`: Referenzen, Schwelle,
`fn looks_like_placeholder(bytes: &[u8]) -> bool`. Tests dort.

**Strang B — Kandidatenliste.** `parse_best_artist` → `parse_ranked_artists`
(rangsortierte Liste, gleiche Ordnung wie heute: Bild vorhanden, dann `nb_fan`,
dann Antwortreihenfolge). Aufrufer in `mod.rs` auf die Liste umstellen, ohne die
Ablehnung — das Verhalten bleibt in diesem Strang identisch.

Die Stränge fassen verschiedene Dateien an; `mod.rs` gehört **B**.

### Welle 2 — Zusammenführung

`load_or_fetch_with` schleift über die Kandidaten, ruft nach jedem Download
`looks_like_placeholder` auf, `MAX_PORTRAIT_CANDIDATES` deckelt. Negativ-Marke
erst nach dem letzten Kandidaten. Tests für die Schleife.

### Welle 3 — Beweis und Landung

Trennschärfe-Skript live laufen lassen, Evidenz ablegen. Gates (`fmt`, `clippy`,
`cargo test -p reprise-core`, Core-Purity). Abnahmestrecke aus #469 als
Regression. Danach landen wie üblich (rebasen, pushen, sofort mergen).

---

## Abnahmekriterien

1. Beide bekannten Silhouetten-Varianten werden abgelehnt, gemessen mit dem
   Rust-Code, Abstand ≤ 0,06.
2. Mindestens 20 echte Porträts der Bibliothek werden angenommen, Abstand
   ≥ 0,25. Kein Fehlalarm, insbesondere nicht bei „Falling in Reverse" (256
   Farben, echtes Foto).
3. Ein Künstler, dessen bester Kandidat die Silhouette trägt und dessen
   Namensvetter ein echtes Foto hat, landet mit dem echten Foto im
   Zwischenspeicher — auch wenn die Kennung **nicht** in
   `MISSING_IMAGE_IDENTIFIERS` steht. Das ist der Kern des Plans.
4. Sind alle Kandidaten Platzhalter, entsteht genau eine Negativ-Marke und kein
   Bild.
5. Höchstens `MAX_PORTRAIT_CANDIDATES` Bildabrufe pro Künstler, nachgewiesen
   über die gezählten Download-Aufrufe im Test.
6. Die Abnahmestrecke aus #469 liefert dasselbe Ergebnis wie dort.
7. Jeder neue Test war vor seiner Implementierung rot, mit Beleg.

---

## Risiken

**Die Schwelle ist an 22 Bildern einer Bibliothek gemessen.** Die Trennung ist
mit Faktor 6 groß genug, dass ein einzelnes ungewöhnliches Foto sie nicht
schließt — aber sie ist nicht an ganz Deezer gemessen. Gegenmittel: Kriterium 2
misst live und breit; wird dort je ein Abstand unter 0,25 beobachtet, gehört die
Schwelle neu hergeleitet, nicht stillschweigend nachjustiert.

**Ein Fehlalarm ist teurer als ein Fehlschlag.** Ein fälschlich abgelehntes
echtes Foto ist unsichtbar (Initialen statt Bild) und niemand meldet es. Deshalb
die asymmetrische Schwelle: lieber ein Platzhalter durchgelassen als ein Foto
verworfen. 0,15 gegen 0,311 hält diesen Abstand ein.

**Deezer zeichnet die Silhouette neu.** Dann greift der Fingerabdruck nicht mehr
und es braucht eine weitere Referenz — eine Zeile, die dann aber alle Künstler
auf einmal abdeckt. Genau dieser Unterschied ist der Zweck des Plans.

**Der Umbau von `parse_best_artist` bleibt im Modul.** Geprüft am 14.08.: neun
Fundstellen, alle unter `crates/reprise-core/src/artist_portrait/`, davon acht in
Tests. Kein Aufrufer außerhalb.

---

## Ausdrücklich nicht Gegenstand

- Kein Erkennen *unbekannter* Platzhalter über Bildeigenschaften (flach, wenige
  Farben). Die Messung oben zeigt, warum: „Falling in Reverse" wäre ein
  Fehlalarm.
- Keine Paginierung der Suche, kein Anfassen der Auswahlordnung aus E2 des
  Vorgängerplans.
- Kein Löschen bestehender Zwischenspeicher-Einträge im Code. Der Ordner wurde
  am 14.08. einmalig von Hand geleert.
- Kein GNOME-Code. Die Änderung ist vollständig in `reprise-core`.
