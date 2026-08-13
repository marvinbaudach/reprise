---
slug: cua-explore-capture-and-stall-attribution
worktree: /home/marvin/Projects/reprise-cua-explore-night-run-fixes
branch: feature/cua-explore-night-run-fixes
phase: planned
created: 2026-08-11
---

# Die beiden offenen Punkte aus M4b

Nach M4b (11 von 12 Läufen sauber) bleiben zwei Wurzeln offen, dazu eine
Aufräumarbeit. Beide Befunde sind gemessen, nicht vermutet — die Zahlen unten
sind Fakten aus `~/.cache/reprise-explore-evidence/2026-08-11-m4b/`.

## Rahmen

- Worktree `/home/marvin/Projects/reprise-cua-explore-night-run-fixes`, Branch
  `feature/cua-explore-night-run-fixes` (PR #402 ist offen, `dev` ist eingemergt).
- **Kein `cargo`.** Reines Python unter `scripts/cua-explore/` und `scripts/tests/`.
- Fokussierte Commits, **Englisch im Code und in den Commit-Messages**, kein Push.
- **Nicht** versuchen, `cua-driver` gegen einen echten Bildschirm zu fahren.

## A — Eine Klartext-Fehlermeldung wird als „invalid JSON" gemeldet

`pointer-layout-reachability` seed 29 endete nach 22 s mit
`cua-driver get_window_state returned invalid JSON`. Der aufbewahrte Fault
(`driver-faults.jsonl`, Feld `stdout_head`) zeigt, was wirklich ankam:

```
Capture error: window screenshot failed for window 8388613:
all Linux window capture backends failed
- XShm: MIT-SHM capture failed after reconnect for DISPLAY=:2:
  X11Error { error_kind: Match, error_code: 8, … major_opcode: 130,
             extension_name: "MIT-SHM", request_name: "GetImage" }
- XGetImage: XGetImage failed after reconnect (…)
```

Also **kein kaputtes JSON**, sondern eine menschenlesbare Fehlermeldung auf stdout,
bei Exit 0. Dreimal hintereinander innerhalb von 0,75 s, dann Abbruch. Der Treiber
hat zuvor selbst neu verbunden („after reconnect"); im Treiberlog steht dazu nichts.
Die App lebte und war beschäftigt (Spektrogramm-Analyse lief in derselben Sekunde
zu Ende). Fenstergröße der Mission ist 1200×800 auf einem 1920×1200-Bildschirm,
passt also — die Wurzel ist **nicht** die Geometrie.

Was daran heute dreifach schiefgeht:

1. Die Meldung nennt die falsche Ursache („invalid JSON"), obwohl der Wortlaut des
   Treibers vorliegt. Das hat mich beim Lesen schon einmal fehlgeleitet.
2. Die Wiederholungsleiter für kaputte Antworten ist 0,25 s + 0,50 s — zu kurz für
   eine Störung, die der Treiber gerade selbst mit einem Reconnect beantwortet hat.
3. **Ein fehlender Screenshot beendet den ganzen Lauf**, obwohl Geometrie und
   AT-SPI-Baum davon gar nicht abhängen.

### Zu tun

- Eine **nicht-JSON-Antwort mit Exit 0** ist eine eigene Form. Sie gehört so
  benannt, und die **erste Zeile des Treibertexts** gehört in die Fehlermeldung —
  wer den Lauf später liest, muss die Ursache sehen, ohne die Fault-Datei zu öffnen.
- Wiederholen mit einer Leiter, die eine vorübergehende Capture-Störung überbrücken
  kann (Größenordnung wie bei der Bereitschaftsleiter, begründet im Code).
- Hält die Störung an: **den Lauf nicht beenden**, sondern den Snapshot **ohne
  Screenshot** aufnehmen und weiterarbeiten. Geometrie und Baum sind vollständig da.
- **Die entscheidende Auflage:** ein fehlender Screenshot darf für die visuellen
  Orakel niemals wie „nichts hat sich geändert" aussehen. Der Snapshot muss
  ausdrücklich als *ohne Bild* markiert sein, und jedes Orakel, das Bilder
  vergleicht, muss ihn überspringen statt ihn zu bewerten. Sonst erzeugt genau
  dieser Fix eine neue Klasse falscher Produktbefunde — dieselbe Krankheit, gegen
  die dieser Branch angeht.
- Das Ganze bleibt ein **Harness-Fault** (`driver-transport-fault`), nie ein
  Produktbefund.

## B — `main-loop-stall` trennt Maschinenlast nicht von der App

148 Vorkommen über zwölf Läufe. Ich habe die Stall-Zahl gegen die parallel
protokollierte Systemlast gestellt (`host-load.log`, Minutentakt, 8 Kerne):

| Lauf | Stalls | Last (Median) |
| --- | --- | --- |
| `large-library-stress` seed 11 | **47** | 7,74 |
| `large-library-stress` seed 29 | **28** | 6,10 |
| `offline-recovery` seed 29 | 13 | **13,44** |
| `offline-recovery` seed 11 | 5 | 5,08 |
| `hover-affordance-sweep` 11 / 29 | 13 / 11 | 9,72 / 8,31 |
| `section-search-isolation` 11 / 29 | 2 / 7 | 5,39 / 7,73 |

**Das Ergebnis ist zweideutig, und genau das ist der Befund.** `offline-recovery`
stützt die Lastthese (13 Stalls bei Last 13,4 gegen 5 bei 5,1 — gleiche Mission,
gleicher Code). `large-library-stress` widerlegt sie: **47 Stalls bei ganz
gewöhnlicher Last 7,7.** Dort steckt also sehr wahrscheinlich ein echtes
Produktproblem — nur kann das Orakel es heute nicht belegen, weil es ausschließlich
Wanduhrzeit misst.

### Zu tun

- Zu jeder beobachteten Antwortlücke zusätzlich messen, **was die App in dieser Zeit
  selbst getan hat**: die eigene CPU-Zeit des App-Prozesses (`utime + stime` aus
  `/proc/<pid>/stat`, Differenz über das Fenster) und die Systemlast im selben
  Moment. Beide Zahlen gehören in die Evidenz des Befunds.
- Daraus die Einordnung ableiten: hat die App in der Lücke selbst gerechnet, ist es
  ein **Produktbefund**; war sie untätig, während die Maschine belastet war, ist es
  ein **Umgebungshinweis** und ausdrücklich kein Produktbefund.
- Die Schwelle ist frei zu wählen, aber **zu benennen und zu begründen** — keine
  erfundene Genauigkeit. Im Zweifel lieber als Umgebungshinweis führen: ein
  verschwiegener echter Stall ist ärgerlich, ein erfundener kostet einen ganzen
  Untersuchungsdurchgang.
- Die Rohzahlen bleiben in jedem Fall erhalten, damit ein späterer Leser die
  Einordnung nachrechnen kann, statt ihr glauben zu müssen.

## C — Tote Setter aufräumen

`report.set_geometry_resolution` und `set_geometry_calibration` haben seit dem
Generationen-Sammler **keinen Aufrufer mehr**, und der `elif resolution:`-Zweig im
Markdown-Report ist für neue Läufe unerreichbar. Das ist eine Falle: wer sie wieder
aufruft, hat erneut einen einzelnen Snapshot als Laufkennzahl.

Zu tun: entfernen. **Nicht** anfassen darf man den Rückfallzweig in
`aggregate_report.py` — der liest Evidenz von der Platte, die vor dem Umbau
geschrieben wurde (die zwölf Läufe der Nacht vom 10.08. müssen weiter aggregierbar
bleiben). Es gibt dafür bereits einen Test; er muss grün bleiben.

## Verifikation — verbindlich

1. **A:** Ein Test muss **durch den produktiven Pfad** zeigen, dass ein Treiber, der
   dauerhaft eine Klartext-Capture-Fehlermeldung liefert, den Lauf **weiterlaufen**
   lässt, der Snapshot als *ohne Bild* markiert ist und **kein** bildvergleichendes
   Orakel daraus einen Produktbefund macht. Ein Test, der nur die Fehlermeldung
   prüft, deckt den Befund nicht ab.
2. **B:** Ein Test muss beide Fälle abbilden: Lücke mit eigener CPU-Zeit der App →
   Produktbefund; Lücke ohne eigene CPU-Zeit bei belasteter Maschine → kein
   Produktbefund. Beide Rohzahlen müssen in der Evidenz stehen.
3. **Mutationsprobe für jeden Punkt (A, B, C):** Fix testweise zurücknehmen, Suite
   muss **rot** werden. Kommando, Exit-Code und Zahl der Fehlerzeilen in den
   Bericht; danach Rücknahme rückgängig, Diff leer.
4. `bash scripts/tests/cua-explore.sh` läuft durch. Stand vorher: **436 Tests in 16
   Dateien**, Exit 0. Die **Dateizahl darf nicht sinken**. Beide Zahlen im Bericht.
