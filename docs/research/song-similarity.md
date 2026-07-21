# Song-Ähnlichkeit für Reprise

Stand: 2026-07-21

## Kurzantwort

Ja, es gibt brauchbare Verfahren. Für Reprise sollte „ähnlich“ aber nicht als
eine einzige objektive Wahrheit modelliert werden. Hörtests zeigen, dass
Menschen sich innerhalb eines Genres weniger einig sind als über deutliche
Genregrenzen hinweg und dass sogar die momentane Stimmung Urteile beeinflusst
([Flexer, Lallai und Rašl 2021](https://transactions.ismir.net/articles/10.5334/tismir.107)).
MIREX bewertet Audio-Ähnlichkeit deshalb primär mit menschlichen Urteilen und
trennt „query by example“ ausdrücklich von allgemeiner Empfehlung
([MIREX Audio Music Similarity and Retrieval](https://music-ir.org/mirex/wiki/2019%3AAudio_Music_Similarity_and_Retrieval)).

Die konservative Empfehlung lautet daher:

1. Ein lokales, hochdimensionales Audio-Embedding übernimmt die eigentliche
   Suche nach „klingt ähnlich“.
2. Das vorhandene Vier-Dimensionen-Profil bleibt für verständliche Erklärungen,
   Filter und Energieverläufe erhalten, aber nicht als Hauptabstand.
3. Genre/Tags, persönliche Hörhistorie und optionale Artist-/Track-Graphen
   liefern getrennte Rankings, die erst danach hybrid zusammengeführt werden.
4. Fehlende Analysen führen immer zu einem abgestuften Fallback statt zu einer
   leeren Vorschau.

Damit funktioniert ein Mix bereits mit einer einzigen Band in der Sammlung:
Die Audio-Suche vergleicht Songs direkt und benötigt weder fremde Artists noch
einen erfolgreichen Internetaufruf.

## Warum das aktuelle Profil nicht genügt

Der aktuelle Reprise-Planner vergleicht `intensity`, `brightness`,
`dynamicity` und `rhythmicity` per quadrierter euklidischer Distanz
([`mix_planner_plan.rs`](../../crates/reprise-core/src/mix_planner_plan.rs)).
Diese Werte werden im Wesentlichen aus RMS-Lautheit, Dynamikspanne,
Spektralzentrum/-Rolloff, Onset-Rate und Tempo-Konfidenz projiziert
([`audio_analysis_accumulator.rs`](../../crates/reprise-core/src/audio_analysis_accumulator.rs)).

Das ist nützlich für Aussagen wie „heller“, „dynamischer“ oder „rhythmischer“.
Vier skalare Mittelwerte verlieren aber Instrumentierung und Klangfarbe,
harmonische und melodische Muster, Gesangscharakter, Produktion, Textur und
zeitliche Struktur. Zwei musikalisch sehr verschiedene Songs können deshalb
fast dieselben vier Werte haben. Die Dimensionen sollten weiterhin sichtbar
und erklärbar sein, jedoch nur als Reranking-Signal und für die Energie-Kurve.

## Geeignete Repräsentationen

| Kandidat | Was er kann | Eignung für Reprise | Lizenz-/Betriebsrisiko |
| --- | --- | --- | --- |
| **OpenL3** | Erzeugt lokale Audio-Embeddings mit 512 oder 6144 Dimensionen und bietet ein `music`-Modell ([offizielle Doku](https://openl3.readthedocs.io/en/stable/tutorial.html), [offizielles Repository](https://github.com/marl/openl3)). | Guter distributierbarer Baseline-Kandidat; zuerst gegen menschliche Paarurteile prüfen, da das Training audiovisuell und nicht speziell auf Song-Ähnlichkeit optimiert wurde. | Code MIT, Gewichte CC BY 4.0; damit wesentlich unkomplizierter als NC-Modelle. Python/TensorFlow ist für die App zu schwer, daher nur nach erfolgreichem Qualitäts-Prototyp als ONNX-/native Inferenz integrieren. |
| **musicnn** | Musikspezifisches CNN für Auto-Tagging; die Beispiel-Tags enthalten Genre, Instrumente, Tempo- und Stimmungshinweise ([Paper](https://arxiv.org/abs/1909.06654), [offizielles Repository](https://github.com/jordipons/musicnn)). Verborgene Aktivierungen können als Embedding dienen. | Sinnvoller zweiter Baseline-Kandidat, besonders für semantische Ähnlichkeit. Es ist aber kein direkt auf menschliche Ähnlichkeitsurteile trainiertes Modell. | Repository unter ISC. Vor dem Bündeln müssen Lizenzumfang der konkreten Gewichte und Herkunft der Trainingsdaten separat dokumentiert werden. |
| **Discogs-EffNet** | Bietet Klassifikations- und kontrastiv trainierte Embeddings; Varianten ziehen Tracks desselben Artists, Labels, Releases oder Tracks zusammen, `multi` kombiniert Ziele ([Essentia-Modelldoku](https://essentia.upf.edu/models.html#discogs-effnet)). | Inhaltlich der passendste sofort verfügbare Prototyp für Musikähnlichkeit; gut als Qualitätsreferenz im Benchmark. | MTG-Modelle sind CC BY-NC-SA 4.0, Essentia selbst AGPLv3 oder proprietär ([Modelle](https://essentia.upf.edu/models.html), [Essentia](https://essentia.upf.edu/documentation.html)). Nicht ungeprüft in der Standarddistribution bündeln; nur Forschung, User-supplied Model oder proprietäre Lizenz. |
| **MERT** | Selbstüberwachtes Musikmodell mit 95M/330M Parametern; der Paper-Benchmark umfasst 14 Music-Understanding-Aufgaben ([Paper](https://arxiv.org/abs/2306.00107), [offizielle Implementierung](https://github.com/yizhilll/MERT)). | Gute Forschungsreferenz, aber schwerer als für die erste lokale Desktop-Version nötig und nicht direkt auf Similar-Song-Retrieval optimiert. | Code Apache-2.0, veröffentlichte MERT-v1-Gewichte CC BY-NC 4.0 ([Model Card](https://huggingface.co/m-a-p/MERT-v1-95M)); deshalb kein Default-Modell. |
| **MuLan / musiksprachliche Modelle** | Gemeinsamer Raum für Audio und freie Texte wie „melancholischer Post-Rock mit langsamem Aufbau“; MuLan demonstriert Zero-Shot-Tagging und Cross-Modal-Retrieval ([Paper](https://arxiv.org/abs/2208.12415)). | Sehr interessant für „Playlist nach Stimmung“ und MCP-Textanfragen, aber kein erster Baustein für robuste lokale Song-zu-Song-Suche. | Der geprüfte offizielle Primärbeleg liefert eine Methode, aber kein unmittelbar bündelbares Reprise-Artefakt mit passender Runtime-/Gewichtslizenz. |

**Empfehlung für einen Spike:** OpenL3-512 gegen musicnn und
Discogs-EffNet-`multi` auf denselben lokalen, rechtmäßig vorhandenen Tracks
vergleichen. Discogs-EffNet dient dabei nur als nicht auszuliefernde
Qualitätsreferenz. Wird OpenL3 deutlich geschlagen, sollte nicht vorschnell ein
NC-Modell eingebaut, sondern gezielt nach einem kommerziell nutzbaren
musik-spezifischen Gewicht oder einer Lizenz gesucht werden.

## Vorgeschlagener Algorithmus

### 1. Sparsame lokale Analyse

- Audio wird ausschließlich lokal dekodiert; es wird nichts hochgeladen.
- Zunächst drei deterministische Ausschnitte pro Song aus frühem, mittlerem und
  spätem Bereich analysieren. Als Startwert sind je 10–15 Sekunden sinnvoll;
  ein Benchmark muss anschließend zeigen, ob ein, drei oder fünf Fenster den
  besten Qualitäts-/Kostenpunkt liefern.
- Frame-Embeddings pro Ausschnitt mitteln, die Ausschnittvektoren mitteln und
  den Ergebnisvektor L2-normalisieren. Zusätzlich kann die Streuung der
  Ausschnitte gespeichert werden, um heterogene Songs nicht fälschlich als
  homogen darzustellen.
- Cache-Schlüssel enthalten Audio-Fingerprint, Modell-ID, Modellversion,
  Fensterstrategie und Aggregationsversion. Nur fehlende oder veraltete
  Einträge werden berechnet. Neue Imports landen in einer begrenzten
  Hintergrund-Queue; Wiedergabe und UI bleiben wichtiger als Analyse.

Diese Fensterstrategie ist eine bewusst zu messende Reprise-Entscheidung, keine
aus den Papern abgeleitete magische Konstante. Für einen 512-dimensionalen
`float32`-Vektor fallen nur 2 KiB pro Track an; selbst eine exakte Suche ist bei
Bibliotheken in der aktuellen Reprise-Größenordnung klein genug.

### 2. Audio-Retrieval

- Distanz: `1 - cosine_similarity(normalized_a, normalized_b)`.
- Ein Seed: Nachbarn direkt zum Seed-Vektor.
- Mehrere Seeds: Im Spike mindestens zwei Varianten gegen menschliche Urteile
  prüfen: Abstand zum normalisierten Seed-Zentroid und Mittel der zwei besten
  Seed-Ähnlichkeiten. Letzteres bewahrt mehrere unterschiedliche
  Geschmacksinseln besser; der Centroid bevorzugt einen geschlossenen Mix.
- Seed-Songs ausschließen, Duplikate entfernen und Artist-Abstände erst nach
  dem Retrieval anwenden. Eine Ein-Band-Bibliothek darf diese Regel lockern und
  muss trotzdem Ergebnisse liefern.

Für die heutige Bibliotheksgröße reicht ein exakter linearer Scan. Erst wenn
gemessene Latenz oder Bibliotheksgröße ihn rechtfertigt, sollte ein
Approximate-Nearest-Neighbor-Index dazukommen. HNSW ist dafür ein etablierter
graphbasierter Kandidat mit logarithmischem Skalierungsverhalten bei hoher
Trefferquote ([Malkov und Yashunin](https://arxiv.org/abs/1603.09320)); ein
Index davor wäre unnötige Komplexität.

### 3. Hybrides Ranking statt Scheingenauigkeit

Getrennte Ranglisten erzeugen:

- `audio`: Cosinus-Nachbarn des Embeddings;
- `profile`: vorhandene vier Dimensionen plus BPM für Energie und Erklärung;
- `metadata`: Genre/Tags, ohne „kein Genre“ hart zu bestrafen;
- `taste`: Playcount, Skips, Ratings und bewusstes Nutzerfeedback;
- `graph`: optionale Track-/Artist-Beziehungen aus Internetquellen.

Für den ersten Hybrid empfiehlt sich Reciprocal Rank Fusion statt willkürlich
addierter Rohscores. RRF kombiniert Ranglisten, ohne ihre inkompatiblen
Score-Skalen kalibrieren zu müssen
([Cormack, Clarke und Buettcher 2009](https://research.google/pubs/reciprocal-rank-fusion-outperforms-condorcet-and-individual-rank-learning-methods/)).
Die vom User gewählte Betriebsart steuert nur, welche Ranglisten teilnehmen:

- **Audio character:** Audio dominant; Profil für Energie/Erklärung.
- **Genre:** Metadaten dominant; Audio löst Gleichstände.
- **Related artists:** Graph dominant; Audio verhindert völlig unpassende
  lokale Treffer.
- **Balanced:** Audio, Metadaten, Taste und vorhandener Graph gemeinsam.

Das folgt auch dem bekannten Cold-Start-Befund: Kollaborative Verfahren sind
stark, sobald Interaktionshistorie existiert, Audio-Inhalt lässt sich dagegen
auf neue oder unbekannte Tracks übertragen. Eine gelernte Inhaltsmetrik aus
kollaborativen Signalen verbessert klassische Content-Verfahren
([McFee, Barrington und Lanckriet 2011](https://arxiv.org/abs/1105.2344)).

### 4. Definierte Fallback-Leiter

Eine Vorschau darf nicht davon abhängen, dass jeder Song bereits das neue
Embedding besitzt:

1. Embedding vorhanden: Audio-Ranking verwenden.
2. Nur aktuelles Vier-Dimensionen-Profil vorhanden: damit ranken und Treffer
   sichtbar als „eingeschränkte Audioanalyse“ erklären.
3. Nur Genre/Tags vorhanden: Metadaten-Ranking verwenden.
4. Nur Artist-Beziehung vorhanden: Graph-Ranking verwenden.
5. Keine Evidenz: reproduzierbares neutrales Fallback aus nicht ausgewählten
   Songs, mit ehrlicher Diagnose statt leerem Dialog.

Die Preview zeigt die Abdeckung pro Signal, etwa „Audio-Embeddings 820/1.654,
Basisprofil 1.600/1.654“. Hintergrundanalyse verbessert dieselbe Vorschau
inkrementell; sie ist keine Voraussetzung zum Öffnen.

## Artist- und Track-Graphen

ListenBrainz stellt im offiziellen Labs Dataset Hoster sowohl
`similar-artists` als auch `similar-recordings` bereit
([Dataset Hoster](https://labs.api.listenbrainz.org/)); die Similar-Artists-
Ansicht erlaubt mehrere session-basierte Algorithmen und MusicBrainz-IDs
([Endpoint](https://labs.api.listenbrainz.org/similar-artists)). Das ist ein
gutes optionales Discovery-Signal, aber ein Labs-Endpunkt sollte gecacht,
zeitlich begrenzt und nie zur Voraussetzung für den lokalen Mix werden.

Last.fm hat offizielle Endpunkte für
[`artist.getSimilar`](https://www.last.fm/api/show/artist.getSimilar) und
[`track.getSimilar`](https://www.last.fm/api/show/track.getSimilar); letzterer
beruht laut Dokumentation auf Hördaten. Beide brauchen einen API-Key, und
Last.fm verlangt für kommerzielle oder Forschung-Nutzung vorherige Kontaktaufnahme
([API-Startseite](https://www.last.fm/api)). Daher nur als separat aktivierter
Provider nach geklärten Nutzungsbedingungen, nicht als heimlicher Default.

Online-Provider senden ausschließlich MusicBrainz-IDs beziehungsweise
normalisierte Artist-/Track-Metadaten, niemals Audio oder lokale Dateipfade.
Alle Netzquellen bleiben opt-in, haben kurze Timeouts und liefern bei Fehlern
den lokalen Mix unverändert weiter.

## Wie Qualität entschieden wird

Ein Embedding darf nicht anhand von Tagging-Benchmarks allein ausgewählt
werden. Reprise braucht einen kleinen, reproduzierbaren Hörtest:

1. 50–100 Seeds über mehrere vorhandene Genres und Bekanntheitsgrade ziehen.
2. Pro Seed anonymisierte Kandidaten aus Vier-Dimensionen-Baseline, OpenL3,
   musicnn und der nicht auszuliefernden Discogs-EffNet-Referenz mischen.
3. Nutzer bewertet paarweise „welcher klingt ähnlicher?“ und zusätzlich
   „würdest du ihn in denselben Mix nehmen?“ – akustische Ähnlichkeit und
   persönliche Vorliebe sind nicht dasselbe. Eine Studie mit 117 Personen fand
   akustische Ähnlichkeit zwar stark mit Playlist-/Empfehlungswahl korreliert,
   aber nicht zwingend mit persönlicher Präferenz
   ([Cheng et al. 2020](https://program.ismir2020.net/poster_4-15.html)).
4. Modelle mit NDCG/Pairwise Accuracy, Laufzeit, RAM, Modellgröße und
   Analyseenergie vergleichen; zusätzlich Ergebnisse pro Genre prüfen.
5. Erst danach Modell, Fensterzahl und Multi-Seed-Aggregation festschreiben.

Später kann freiwilliges Feedback wie „mehr davon“/„passt nicht“ lokal die
Hybridgewichte pro Nutzer anpassen. Es sollte niemals automatisch als globale
Trainingsmusik hochgeladen werden.

## Entscheidungsvorschlag

1. **Jetzt:** Vier-Dimensionen-Fallback und Analyseabdeckung reparieren, damit
   kein Mix wegen fehlender Profile oder nur einer Band leer bleibt.
2. **Nächster technischer Spike:** modellneutrale `AudioEmbeddingBackend`-
   Schnittstelle, versionierter Cache und exakter Cosinus-Scan; OpenL3-512,
   musicnn und Discogs-EffNet-Referenz gegeneinander hören und messen.
3. **Danach:** distributierbares Modell nur bei eindeutig dokumentierter Code-,
   Gewichts- und Runtime-Lizenz wählen; lokale Offline-Inferenz als Default.
4. **Optional:** ListenBrainz-Graph in RRF einbeziehen; Last.fm erst nach API-
   und Nutzungsklärung.
5. **Später:** Text-Audio-Modell für Stimmungs-/MCP-Suche als getrennten
   Provider evaluieren. Musikgenerierung ist ein eigenes Produkt- und
   Lizenzthema und sollte nicht mit dem Similarity-Index gekoppelt werden.

Lizenzhinweise hier sind technische Risikohinweise, keine Rechtsberatung.
