---
slug: cua-explore-input-delivery-and-readiness
worktree: /home/marvin/Projects/reprise-cua-explore-night-run-fixes
branch: feature/cua-explore-night-run-fixes
phase: planned
created: 2026-08-11
---

# Der Nachtlauf bricht ab, wo der Treiber seinen eigenen Ausweg nennt

M4 ist am 11.08. gelaufen (70 Minuten, `~/.cache/reprise-explore-evidence/2026-08-11-m4/`).
**Sieben von zwölf Läufen endeten mit `exploratory run failed:`**, und zwar an genau
zwei Wurzeln — beide im Harness, keine davon ein Produktbefund:

| Wurzel | Läufe |
| --- | --- |
| `background_unavailable` bei `type_text`, `press_key`, `hotkey` | 6 |
| `get_window_state` antwortet **leer** (rc 0, kein Byte auf stdout) | 1 |

Das ist kein Rückschritt: beide Formen liefen unter dem alten Harness **als Erfolg**
durch. Der Vertragsfix macht sie sichtbar — jetzt fehlt die Behandlung.

## Rahmen

- Worktree `/home/marvin/Projects/reprise-cua-explore-night-run-fixes`, Branch
  `feature/cua-explore-night-run-fixes`. Nichts außerhalb anfassen.
- **Kein `cargo`.** Reines Python unter `scripts/cua-explore/` und `scripts/tests/`.
- Fokussierte Commits, **Englisch im Code und in den Commit-Messages**, kein Push.
- **Nicht versuchen, `cua-driver` gegen einen echten Bildschirm zu fahren.** Die
  nötigen Messungen stehen unten und sind bereits gefahren.

## Befund 1 — `background_unavailable` beendet den Lauf, statt zu eskalieren

Der Treiber antwortet wörtlich:

```json
{"code": "background_unavailable",
 "detail": "the requested target has no focus-free input backend; the remaining
            XTest/X11 route can only deliver to the globally focused widget",
 "escalation": {"reason": "background input is unavailable on this surface;
                           retry this action with delivery_mode:\"foreground\""}}
```

Das ist eine der Fehlerhüllen **ohne** `status` und **ohne** `refusal`; seit dem
Vertragsfix wird sie zu `DriverError` und beendet den Lauf. Der Treiber nennt dabei
seinen eigenen Ausweg — und das Harness liest ihn nicht.

### Gemessen (eigener Xvfb, `cua-driver 0.19.3`, `gnome-text-editor`)

Effekt unabhängig geprüft, indem der getippte Text aus dem AT-SPI-Baum
zurückgelesen wurde — nicht aus der Treiberantwort.

| Aufruf | `delivery_mode: background` | `delivery_mode: foreground` |
| --- | --- | --- |
| `press_key` | `background_unavailable` | **akzeptiert**, `route: synthetic_events` |
| `hotkey` | `background_unavailable` | **akzeptiert**, `route: synthetic_events` |
| `type_text` | angekommen (Marker im AT-SPI-Baum) | angekommen |

`delivery_mode` ist ein dokumentierter Parameter (`describe type_text`), Enum
`background` (Standard) | `foreground`. `foreground` aktiviert das Zielfenster,
tippt und stellt das vorher aktive Fenster wieder her. Im privaten Xvfb mit eigenem
openbox ist das folgenlos — es fasst keinen echten Desktop an.

**Wichtige Einschränkung, nicht wegdiskutieren:** `type_text` hat gegen
`gnome-text-editor` **nicht** verweigert, in der Mission aber schon. Die Ablehnung
hängt also am Ziel, nicht am Werkzeug. Der Fix muss deshalb **auf die Ablehnung
reagieren**, nicht auf eine Werkzeugliste.

### Zu tun

- Auf `code == "background_unavailable"` denselben Aufruf **einmal** mit
  `delivery_mode: "foreground"` wiederholen, statt den Lauf zu beenden.
- **Kein Blankoscheck.** Nur dieser eine benannte Code, und nur für Aufrufe, deren
  Schema `delivery_mode` kennt. Jede andere Fehlerhülle beendet den Lauf weiter wie
  heute — die Positivliste `SUCCESS_CONTRACT` bleibt, wie sie ist. Ein pauschales
  „bei Fehler nochmal" macht genau die Blindheit wieder auf, die der Vertragsfix
  geschlossen hat.
- **Die Eskalation muss sichtbar bleiben.** Dass der Hintergrundweg auf dieser
  Oberfläche nicht existiert, ist eine echte Eigenschaft der Messumgebung: als
  Fault zählen (`transport_faults`) und in der Schritt-Evidenz festhalten, dass die
  Aktion nur im Vordergrund zugestellt wurde. Nicht stillschweigend heilen.
- Schlägt auch der Vordergrund-Versuch fehl, endet der Lauf laut wie heute.

## Befund 2 — eine leere Antwort erschöpft die Wiederholungen in 0,75 s

`pointer-layout-reachability` seed 29 endete nach 21 s mit
`cua-driver get_window_state returned invalid JSON`. Die aufbewahrten Faults aus
`driver-faults.jsonl` dieses Laufs zeigen die Wahrheit:

```
0 get_window_state attempt 1 rc 0 stdout=''
1 get_window_state attempt 2 rc 0 stdout=''
2 get_window_state attempt 3 rc 0 stdout=''
```

Kein kaputtes JSON, sondern **gar keine Ausgabe** bei Exit 0. `get_window_state`
steht bereits in `RETRYABLE_TOOLS` (`driver_transport.py:21-24`), aber die Leiter
ist `RETRY_DELAYS_SECONDS = (0.25, 0.50)` — drei Versuche innerhalb von 0,75 s.

Das ist die falsche Größenordnung: für diese Umgebung ist gemessen, dass das Fenster
nach 1929 ms steht, ein **benutzbarer AT-SPI-Baum aber erst nach 6770 ms** (Befund F7
aus demselben Harness). Eine Bereitschaftslücke lässt sich in 0,75 s nicht
überbrücken; der Abbruch nach 21 s Laufzeit passt genau ins Bild.

### Zu tun

- Die leere Antwort ist eine **eigene** Form und gehört als solche benannt — heute
  läuft sie unter „invalid JSON", was in die Irre führt.
- Die Wiederholungsleiter muss mehrere Sekunden überbrücken können, statt in
  Millisekunden auszulaufen. Die Zahlen sind frei zu wählen, aber **an der
  gemessenen Bereitschaft zu begründen** (Kommentar im Code, nicht nur in der
  Commit-Message), und die Gesamtwartezeit darf ein Missionsbudget nicht auffressen.
- Jeder Versuch bleibt als Fault erhalten, wie heute. Bleibt die Antwort leer, endet
  der Lauf laut.

## Verifikation — verbindlich

1. Ein Test muss **durch den produktiven Pfad** zeigen: ein Treiber, der
   `background_unavailable` antwortet und beim Vordergrund-Versuch liefert, lässt
   den Lauf **weiterlaufen**, der Fault ist gezählt und die Eskalation steht in der
   Evidenz.
2. Ein Test muss zeigen, dass eine **andere**, unbekannte Fehlerhülle den Lauf
   weiterhin beendet. Ohne diesen Test ist nicht bewiesen, dass aus dem gezielten
   Ausweg kein Blankoscheck wurde.
3. Ein Test muss zeigen, dass eine dauerhaft leere Antwort nach erschöpfter Leiter
   laut endet — und dass die Zwischenversuche als Faults erhalten sind.
4. **Mutationsprobe für jeden der drei Punkte:** Fix testweise zurücknehmen, Suite
   muss **rot** werden. Bleibt sie grün, ist der Test keiner. Kommando, Exit-Code
   und Zahl der Fehlerzeilen in den Bericht; danach Rücknahme rückgängig, Diff leer.
5. `bash scripts/tests/cua-explore.sh` läuft durch. Stand vorher: **430 Tests in 16
   Dateien**, Exit 0. Die **Dateizahl darf nicht sinken**. Beide Zahlen im Bericht.
