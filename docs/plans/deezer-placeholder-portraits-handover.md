# Handover — Deezer-Platzhalter-Porträts

Stand: 2026-08-14, 07:30 CEST. Plan: `docs/plans/deezer-placeholder-portraits.md`
(`phase: shipped`). Dies ist die Übergabe für alles, was **noch offen** ist.

## Wo die Arbeit steht

- **Branch** `feature/deezer-placeholder-portraits`, 16 Commits.
- **Worktree** `/home/marvin/Projects/reprise-deezer-placeholder-portraits`, sauber.
- **Nicht gepusht, kein PR.** Push-Zugang ist geprüft und funktioniert
  (ein `fetch` scheiterte einmal transient mit `Connection closed by 140.82.121.3`
  — das war die Gegenseite, nicht die Konfiguration).
- **Basis** war `07f02b8fcc`; `origin/dev` steht inzwischen auf `dd67122fc7`
  (#468, Cover-Cache-Warnungstest). **Rebase nötig.** Geprüft: kein Überlapp mit
  unseren geänderten Dateien.

Produktionscode: 297 Zeilen in `crates/reprise-core/src/artist_portrait/`
(`deezer.rs`, `mod.rs`, `test_fixtures.rs` — letztere `#[cfg(test)]`). Kein
GNOME-Code, kein `Cargo.toml`, keine neue Abhängigkeit.

Belege liegen unter `docs/evidence/deezer-placeholder-portraits/`; die
Abnahmestrecke unter `acceptance/deezer-placeholder-portraits/run-accept.sh`.
Alle acht Orakel der sichtbaren Abnahme sind bestanden, die Regressionsprüfung
deckt Rang 1–20.

---

## Offene Punkte

### 1. Landen (nichts hindert es)

```
cd /home/marvin/Projects/reprise-deezer-placeholder-portraits
git fetch origin dev
git rebase origin/dev
git push -u origin feature/deezer-placeholder-portraits
gh pr create --base dev --fill
scripts/land.sh <pr-nummer> /home/marvin/Projects/reprise-deezer-placeholder-portraits
```

**Nicht auf CI warten.** Rebasen, pushen, sofort mergen — ein Lauf dauert ~45 min,
`dev` bewegt sich schneller, und GitHub verweigert den Merge danach aus einem
veralteten Mergeability-Cache. Die Absicherung ist die *vorher* gelaufene Evidenz,
nicht der grüne Haken. Nach dem Merge den dev-Lauf anschauen und notfalls
nachbessern; erwarte, dass er von der nächsten Landung abgeräumt wird
(`cancelled` ≠ rot).

Nach dem Merge: Worktree und Branch selbst entfernen — ein Squash-Merge lässt
beide stehen. `scripts/close-worktree.sh` scheitert gelegentlich an
`gradlew.bat` (CRLF-Blob); dann von Hand.

### 2. Der zurückgestellte `wait`-Defekt in der geteilten Infrastruktur

`scripts/cua-common/session.sh`, `cua_common_stop_daemon` (um Zeile 101-102):
`kill -TERM "$PID"` gefolgt von `wait "$PID"` ohne Timeout und ohne
`kill -KILL`-Eskalation. Empirisch nachgestellt: mit einem `trap '' TERM`-Kind
kehrt `wait` **nie** zurück, erst ein externer SIGKILL löst es (`status=137`).
Blockiert das, laufen alle nachfolgenden Aufräumschritte nicht mehr.

Bewusst **nicht** in diesem Branch behoben: geteilte Datei, andere Branches
arbeiten darin, und für die Abnahmestrecke reichte die Reparatur in ihrem eigenen
`private_run_cleanup` (dort gibt es jetzt eine 2-Sekunden-Schranke mit
Eskalation, `run-accept.sh` — als Vorlage brauchbar).

Indiz, dass es real beißt: am 14.08. lagen neun `cua-driver`-Prozesse auf der
Maschine, der älteste seit rund 20 Stunden.

Gehört in einen eigenen kleinen Branch, nicht in diesen PR.

### 3. Einmalige Cache-Löschung (E3) — braucht ausdrückliche Freigabe

**Erst nach der Landung** und nur mit ausdrücklichem Ja:

```
rm -rf ~/.cache/reprise/artist-portraits
```

Danach werden Porträts beim nächsten Öffnen von *My Stats* neu geholt — **faul
und nur für angezeigte Künstler**, und nur wenn das Artwork-Modul
(`module.artwork.enabled`, Voreinstellung **aus**) und das Online-Quellen-Gate
das erlauben. Ohne eingeschaltetes Modul passiert gar nichts und es sieht
trotzdem unauffällig aus.

Zustand am 14.08.: 162 Einträge, davon neun Platzhalter — 7× `5dfbb779cfb03bdf.jpg`,
2× `5e811de090722326.jpg`. Die positive TTL läuft am 17./18.08. ohnehin ab; die
Löschung kauft also ~3 Tage und macht die Reparatur sofort sichtbar. Sie ist
nicht zwingend: mit dem gelandeten Fix holt der Code beim Ablauf von selbst das
Richtige.

### 4. Entscheidung nötig: E1s Sentinel-Prämisse ist gebrochen

Der Live-Lauf fand die graue Silhouette unter
`415714b66a5de709809dd3d05f58afe4` — einer **gewöhnlichen künstlerspezifischen**
Bildkennung, nicht der strukturell erzwungenen Leerstring-MD5. Unabhängig
nachgemessen: 213 eindeutige Farben bei 1000×1000, RMSE 0,058 gegen beide
bekannten Varianten, also dasselbe Motiv neu kodiert. Zum Vergleich: eine echte
Fotografie liegt bei 49 000–105 000 Farben und RMSE 0,35–0,87.

Pikant: dieselbe Kennung war am **13.08.** noch ein echtes Bild. Deezer hat sie
innerhalb eines Tages auf die Silhouette umgestellt.

Damit trägt E1s Begründung nur noch für die eine, strukturelle Kennung. Die
Liste `MISSING_IMAGE_IDENTIFIERS` in `crates/reprise-core/src/artist_portrait/deezer.rs`
wächst künftig **pro betroffenem Künstler**, nicht einmalig. Die Annahme des
Plans, eine dritte Kennung sei ein seltenes Ereignis, war nach einem Tag
widerlegt.

**Nicht gebaut**, weil E1 es ausdrücklich verbietet: ein automatischer Detektor
für unbekannte Platzhalter (jede plausible Heuristik wäre inhaltsbasiert).
Die Entscheidung, ob das so bleibt, steht aus. Der Plan gehört um diesen Befund
ergänzt — siehe auch die Notiz `reprise-deezer-portrait-placeholders` im
Gedächtnis.

### 5. Sichtbare Nebenwirkung, die niemand gesehen hat

Rang 10 „Oceano" zeigt künftig das Gesicht eines **Namensvetters**: der
populärere Eintrag (16 388 Fans) hat nur die Silhouette, der mit 2 347 Fans ein
echtes Foto, und die Regel nimmt das Foto. Im Grill so entschieden
(„der Fehlerfall bleibt kosmetisch und auf Gleichnamige begrenzt"), aber bis zur
Abnahme nie sichtbar gewesen. Falls das stört, ist es eine Änderung an E2, kein
Fehler.

---

## Fallen, die den Lauf gekostet haben

Alles reproduziert, alles behoben — hier, damit es nicht ein zweites Mal kostet.

- **Ein Skript, das nie lief, sieht beim Lesen plausibel aus.** Vier der sieben
  Funde (F4–F7) waren durch Lesen nicht auffindbar. Drei Reviews hatten das
  Skript vorher für in Ordnung befunden.
- **Rang 6–20 existieren erst nach dem Klick.** *My Stats* baut beim Öffnen eine
  Leader-Karte plus `RUNNER_UP_COUNT = 4`, also Rang 1–5. Porträtanfragen hängen
  an der **gebauten Zeile**, nicht am abgefragten Künstler. Ohne Klick auf
  „Show more top artists" wird für Rang 10 nie etwas geholt. Die Aufklappung am
  umgeschlagenen Label „Hide more top artists" bestätigen.
- **AF_UNIX-Pfadlimit.** Ein Socket unterhalb des Evidenzverzeichnisses wird
  151 Byte lang; erlaubt sind ~107. Der Fehler kommt als `Cua Driver daemon is
  not running` an, nicht als Pfadfehler. `run-accept.sh` prüft die Länge jetzt
  vorab.
- **„Bridge fehlt" heißt nicht, dass AT-SPI fehlt.** Unbegrenzte
  Accessibility-Snapshots liefen ins Timeout, `cua-driver` fiel danach auf
  X11-Metadaten zurück und meldete das irreführend. Snapshots begrenzen.
- **Fixe `sleep`s als Bereitschaftsersatz.** `HTTP_TIMEOUT` ist 15 s — ein
  `sleep 12` ist kürzer als ein einziger langsamer Abruf. Und ein Screenshot,
  der nur auf zwei benannte Künstler wartet, feuert mitten im Nachladen der
  übrigen: Rang 11/13/14 zeigten Initialen, obwohl die Bytes identisch waren und
  der Abruf für „Bury Tomorrow" nachweislich **50 ms nach** dem Bild startete.
- **Ein Bericht ist kein Beleg.** Die Abnahme meldete „Ränge 1–2 und 4–9
  unverändert" als erfülltes Kriterium 8 — das *alle* übrigen Ränge verlangt.
  Das sieht wie ein grünes Orakel aus und ist schlimmer als ein rotes.
- **Platzhalter erkennt man an der Flachheit, nicht an der Kennung.** ~200
  eindeutige Farben bei 1000×1000 und RMSE < 0,1 gegen eine bekannte Variante.
  Als Diagnose durch einen Menschen zulässig; als ausgelieferter Mechanismus
  verboten.
