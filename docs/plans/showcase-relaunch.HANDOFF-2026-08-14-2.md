# Handover — Showroom-Relaunch, ab hier weiter

> **Überholt von `showcase-relaunch.HANDOFF-2026-08-14-3.md` (18:16).** §2 und
> §3.1 sind erledigt; die Farbklammer in §3.1 war **falsch** und ist dort
> richtiggestellt. Dieses Dokument nur noch als Protokoll lesen.

**Stand:** 14.08.2026, 17:47 CEST. §2 ist erledigt — **der nächste Schritt
steht in §3**. Nachfolger von
`showcase-relaunch.HANDOFF-2026-08-14.md` — jenes Dokument bleibt gültig für
Hintergrund, Zahlen (§2) und Fallen (§5); dieses hier sagt, was als Nächstes
zu tun ist. Der Auftrag des Nutzers dafür liegt vor („jo", 17:39).

---

## 0 Woran man sich halten muss

- **Nicht pushen.** Weder Showcase noch Bewerbung. Der Nutzer hat dem Bau
  zugestimmt, nicht der Veröffentlichung.
- **Maßstab bleibt `dev@604677322e`.** Erst ganz zum Schluss, direkt vor der
  Freigabe, alle Zahlen einmal gegen den dann aktuellen dev nachziehen.
- **Zahlen nie aus dem Gedächtnis.** Immer aus §2 des Vorgänger-Handovers oder
  aus `$OLD/measurements/` (Pfad unten).
- **Wake-Lock `showcase-relaunch` läuft.** `wake-lock release showcase-relaunch`,
  wenn die Arbeit endet.
- **Nichts einem Agentenbericht glauben** — jede Abbildung und jede Seite per
  Bild kontrollieren.

---

## 1 Was steht

| Repo | Branch | Stand |
|---|---|---|
| `reprise-showcase` | `feat/showcase-relaunch` | `45a0b87` — sauber, **nicht gepusht** |
| `bewerbung` | `fix/reprise-zahlen-604677322e` | `d683af1` — sauber, **nicht gepusht** |

Die Bewerbungsmappe hat eigene Verträge, alle grün (17:58):

```
bash tests/html_cv_test.sh            # HTML document contract: OK
bash tests/anschreiben_cortec_test.sh # Cortec cover letter: OK
bash tests/steckbrief_test.sh         # Projektsteckbrief: OK
bash tests/mutation-run.sh            # Mutationslauf: jeder Fall hat sich verhalten
```

Alle drei Gates grün (17:46):

```
bash scripts/check-showcase.sh          # Bilingual showcase contract passed
bash scripts/tests/readme-evidence.sh   # Evidence contract: OK — both READMEs and both pages
python3 scripts/build-pages.py --check  # pages are current
```

Dazu der Gegenprobe-Lauf, von Hand zu starten und **nicht** Teil der drei Gates:

```
bash scripts/tests/mutation-run.sh      # Mutation run: every case behaved
```

**Die vier Clips sind fertig, eingebaut und committet** (`d8a1da3`) —
provisorisch, Montag werden sie getauscht. Details, Messwerte und alle Fallen
der Aufnahme stehen in §3b des Vorgänger-Handovers. Aufnahmekette und
vorgewärmte Profile liegen dauerhaft unter
`~/.cache/reprise-showcase-recording/` mit eigenem README; Montag ist das ein
erneutes Ausführen, kein Neubau.

Altes Arbeits-Scratchpad, weiterhin gebraucht (`$OLD`):
`/tmp/claude-1000/-home-marvin-Projects-reprise/d807356c-692f-41c2-8605-e21ac20e5317/scratchpad`
— darin `ARBEITSSTAND.md`, `design-system.md`, `measurements/M1…M5`.
**Liegt auf tmpfs.** Wenn es weg ist: die Zahlen stehen auch in §2 des
Vorgänger-Handovers, die Belege lassen sich aus dem Repo neu zählen.

---

## 2 ~~Der nächste Schritt: das Gate deckt die Pages-Seite nicht ab~~ — erledigt (`45a0b87`)

Die Seite läuft nicht mehr ungeprüft mit. Was jetzt steht:

- **`check-showcase.sh`** rendert beide Seiten neu und verweigert eine, die
  nicht mehr zu `pages/template.html` und den Katalogen passt — eine
  Handkorrektur an einem Build-Produkt überlebt das nicht mehr. Die
  Sperrklauseln (iOS, Tauri, „source is private", die alte Übersichtsgrafik)
  und die Regeln zu Schweizer Orthografie und UTF-8 gelten jetzt für vier
  Flächen statt zwei.
- **`readme-evidence.sh`** pinnt dieselben Zahlen auf allen vier
  veröffentlichten Flächen. Drei Helfer sagen, wohin eine Aussage gehört:
  `require()` alle vier, `readme()` nur Markdown-Formen (Badge-URLs,
  Tabellenzellen), `page()` die eigene Formulierung der Seite für dieselbe
  Messung. Meldezeile heisst jetzt `Evidence contract: OK — both READMEs and
  both pages`.
- **`scripts/tests/mutation-run.sh`** ist neu: acht Mutationen, jede nennt die
  Gates, die rot werden müssen, jede wird zurückgenommen und nachgeprüft. Der
  Lauf verweigert den Start auf einem schmutzigen Baum. Damit ist die
  Gegenprobe wiederholbar statt eine Behauptung im Handover.

**Zwei Befunde aus dem Lauf, beide behoben:**

1. **Eine Mutation, die nur das erste Vorkommen ersetzt, beweist nichts.**
   `347,842` steht dreimal im englischen Katalog; ein überlebendes Vorkommen
   hielt den Pin grün und liess ein funktionierendes Gate blind aussehen. Die
   Mutation ersetzt jetzt *jedes* Vorkommen.
2. **Die Seite inlined die Abbildungen** — ein blosser Zahlen-Pin lässt sich
   deshalb von der Abbildung erfüllen, während die Prosa daneben etwas anderes
   sagt. Wo die Seite eine Messung zweimal nennt (Prosa und Abbildung), ist
   jetzt zusätzlich der Satz gepinnt, sonst könnten die zwei Hälften einer
   Seite sich widersprechen und trotzdem durchgehen.

Was bewusst nicht geändert wurde: `check-showcase.sh` bricht beim ersten Fehler
ab (`fail()` → `exit 1`), sammelt also *nicht*, anders als es in einer früheren
Fassung dieses Handovers stand. Nur `readme-evidence.sh` sammelt. Der Umbau
wäre ein Eingriff in ein durchgetestetes Skript ohne Nutzen für diese Aufgabe.

---

## 3 Der nächste Schritt und der Rest

1. ~~**Schritt 6 zu Ende:** einseitiges PDF „Projektsteckbrief Reprise"~~ —
   **erledigt** (`394f68b` im `bewerbung`-Repo, weiterhin nicht gepusht).
   `src/steckbrief-reprise.html`, `build.sh`-Eintrag mit Vertrag „eine Seite,
   mindestens sechs echte Hyperlinks" (das PDF trägt acht),
   `tests/steckbrief_test.sh` und `tests/mutation-run.sh`.

   **Die Farbklammer aus dem alten Handover war falsch — nicht wiederholen.**
   Nachgerechnet aus `crates/reprise-view/src/spectral_colour.rs@604677322e`:
   die Achse läuft **Coral `#FF6F5E` → Magenta → Violett → Blau → Teal
   `#4FDBD4`**, also genau andersherum als dort behauptet. Bei 58 % steht
   `#99adff`; der dem CV-Akzent `#9184d9` nächste Punkt der ganzen Rampe ist
   `#a2a8ff` bei t ≈ 0,55, sRGB-Abstand 55 — eine sichtbar andere Farbe.
   **Die echte Klammer**, die jetzt auf dem Blatt steht: `CORAL` und `TEAL` sind
   die beiden Konstanten aus `spectral_colour.rs`, und der Showroom nimmt sein
   Farbschema aus genau diesen zwei Werten.

   **Nebenbefund, mit repariert:** die drei Testverträge der Bewerbung waren
   seit der Zahlenkorrektur (`8d3a4ba`) rot — sie pinnten weiter Tauri,
   `217'800`/`89'000`, „18 Quality-Gates" und „18 Prüf-Gates und 5.541 Tests".
   Die Dokumente stimmten, ihre Wächter nicht; niemand hat sie laufen lassen.
   Jetzt pinnen sie die aktuellen Zahlen, verbieten die überholten in *allen*
   Dokumenten der Mappe und prüfen den gemeinsamen Maßstab-Commit, der nur noch
   einmal in `src/shared/profile.js` steht.

2. **Schritt 7 — Gegenlesen:** jede Zahl gegen die Belege, jeder Beleglink
   aufgerufen, beide Sprachfassungen gegeneinander. **Der Steckbrief gehört
   dazu** — seine acht Links sind noch nicht einzeln aufgerufen worden.
3. **Zum Schluss:** Maßstab auf den dann aktuellen dev ziehen — Zahlen,
   Abbildungen, beide READMEs, beide Sprachfassungen der Seite, CV, und alle
   `blob/604677322e/`-Permalinks.
4. **Erst dann Freigabe einholen und pushen.**

---

## 4 Zwei offene Kleinigkeiten

- **Die Medienlautstärke des Handys steht auf 0.** Für die Aufnahme
  stummgeschaltet; `adb shell cmd media_session volume --stream 3 --set 7`
  greift ohne aktive Audio-Route nicht. Der Nutzer stellt sie mit der Wippe
  wieder hoch — nicht erneut per adb daran herumprobieren.
- **`.clip .pending` im `pages/template.html` ist jetzt totes CSS.** Absichtlich
  stehen gelassen, falls ein Clip-Slot wieder leer laufen muss. Beim Gegenlesen
  entscheiden, ob es raus soll.

---

## 5 Umgebung

- **`git archive` liefert `docs/` unvollständig** (`export-ignore` auf
  `docs/plans`). Wer dort etwas nicht findet: `git show 604677322e:<pfad>`.
- **Der Load-Governor-Hook** blockt Bash-Kommandos, die nach schweren
  Einstiegspunkten aussehen — auch reine Textsuchen mit einem Skriptnamen im
  Muster. Dann `HEAVY_RUN_DISABLE=1` voranstellen.
- **Bash-Ausgabe kappen**, sonst frisst der Kontext den Rest der Sitzung.
- **Seite immer per Bild kontrollieren:**
  `chromium --headless=new --disable-gpu --hide-scrollbars --window-size=1440,24000
  --virtual-time-budget=20000 --screenshot=… file://…/index.html`, dann mit
  `magick` in Streifen schneiden und montieren. Der Sprung per `#anker` taugt
  nicht — die Seite scrollt weich, das Ergebnis war ein schwarzes Bild.
- **Pages deployt aus `main`.** Arbeit auf `feat/showcase-relaunch` wird erst
  nach dem Merge live.
