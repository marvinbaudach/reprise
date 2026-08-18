# Übergabe — Feed-Tags-Nachlese und ein neuer Standort-Befund

**Stand:** 15.08.2026, 08:10 · **Löst ab:**
`docs/plans/updates-concerts-releases-rework.HANDOFF.md` (dessen drei offene
Entwurfsfragen sind jetzt entschieden).

## Kurzfassung

Diese Sitzung hat die drei Entwurfsfragen entschieden, die die
Updates/Concerts/Releases-Runde offen gelassen hatte, daraus einen fertigen,
gegrillten Plan gemacht — und dabei einen **neuen, unabhängigen Fehler**
aufgenommen, den der Eigentümer per Screenshot gemeldet hat.

Es ist **kein Code geschrieben** worden. Der Plan wartet auf `/code`.

## 1. Was entschieden ist

Alle drei offenen Fragen der Vorgänger-Übergabe, vom Eigentümer beantwortet:

| Frage | Beschluss |
|---|---|
| Welche Verfügbarkeitswerte bekommen im Popover einen Tag? | **`Off sale` und `Unknown` ja, `On sale` nein.** Die Tabelle ist Vergleichsfläche und füllt alle drei; der Feed markiert die Abweichung |
| Was passiert mit R4 (Kachel-Parität)? | **Datierter Nachtrag in §9 des Mutterplans, kein Umschreiben.** Der Anforderungstext bleibt wörtlich stehen — ein abgeschlossenes Protokoll wird nicht nachträglich begradigt. Keine neue ux-rules-Regel für R4 |
| Wie wird die neutrale Pille angeglichen? | **Eigener Ton fürs Popover**, die Töne spiegeln die drei Tabellenklassen 1:1. Der Upcoming-Chip der Releases bleibt unangetastet |

## 2. Der Plan

**`docs/plans/feed-tags-mark-the-exception.md`** · `phase: planned` ·
Branch-Name steht schon drin: `feature/feed-tags-mark-the-exception`.

Nächster Schritt, unverändert:

```
/code docs/plans/feed-tags-mark-the-exception.md
```

Fünf Aufgaben, ein Strang: (1) Töne, Tag-Auswahl und Regel NR-39 in *einem*
Commit — die Hausordnung von `docs/ux-rules.md` verlangt Regel und Beweis
gemeinsam; (2) zwei Paritätstests; (3) der R4-Nachtrag; (4) Sondenskript samt
Messung; (5) der Gate-Lauf.

### Was der Grill geändert hat

Sechs Entscheidungen, alle im Plan vermerkt. Drei davon haben den Entwurf
wirklich bewegt:

- **Vier Töne statt drei.** Der Entwurf hat vorgerechnet, dass „genau ein
  dritter Ton", „exakte 1:1-Spiegelung" und „Upcoming-Chip unverändert" zu
  dritt nicht erfüllbar sind: `.updates-tag-neutral` ist *keine* der drei
  Tabellenklassen. Spiegelung und Chip gewinnen, bezahlt mit einer vierten
  Enum-Variante.
- **Der Anzeigetest bekommt sein Kriterium vorab**, nicht als
  Rückfallposition: Geometrie plus drei Farbstichproben mit ±1, kein
  Byte-Vergleich. Der Entwurf hätte Codex sonst mitten im Lauf entscheiden
  lassen — genau das, was er an anderer Stelle vermeidet.
- **Das Sondenskript wird getrackt, die Bilder nicht.** Was letzte Runde
  verloren ging, war der Prüfstand, nicht die Aufnahmen. Die Zahlen (RGB mit
  Koordinaten, beide SHAs, Werkzeugversionen) tragen `manifest.txt`.

### Eine nachgemessene Falle, die im Plan steht

`contrast_1_text_classes_consume_roles_without_local_dimming` prüft per
Substring `!rules.contains("color: alpha(@window_fg_color")`. Der String
`background-color: alpha(@window_fg_color, 0.08)` **enthält** ihn — weil
„background-color" auf „color" endet. Wer den neuen gefüllten Ton in die Liste
dieses Tests aufnimmt, macht ihn rot, obwohl die Deklaration korrekt ist.
Nachgemessen, nicht vermutet; im Plan als Falle R-3.

## 3. Der neue Befund: der Standort-Chip ist zu lang

**Gemeldet am 15.08.2026 per Screenshot.** Der aktive Filter-Chip der
Concerts-Ansicht liest sich:

```
Zürich, Bezirk Zürich, Zürich, Schweiz/Suisse/Svizzera/Svizra · 500 km
```

**Gewünscht:** die Stadt allein, in der Sprache der Oberfläche.

**Das ist keine Entwurfsfrage, sondern eine verletzte Regel.** CONC-2
(`docs/ux-rules.md:5095-5107`) sagt bereits: *„With location, the active chip
reads `{city} · {radius} km`"*. Die Formatierung tut auch genau das — sie
bekommt nur keine Stadt gereicht.

Die Kette, belegt gegen `origin/dev` @ `b6be7cdc61`:

| Stelle | Datei |
|---|---|
| formatiert `{city} · {radius} km` | `crates/reprise-gnome/src/ui/strings_concerts.rs:101-105` (`concerts_location_radius`) |
| ruft es auf | `crates/reprise-gnome/src/ui/concerts/concerts_filter_bar.rs:89-97` (`chip_label`) |
| liefert den Namen | `AppLocation.name`, `crates/reprise-core/src/location.rs:38-43` |
| füllt ihn | `crates/reprise-core/src/concerts/geocode.rs:41-42,59` — nimmt Nominatims **`display_name`** roh |
| reicht ihn durch | `crates/reprise-gnome/src/ui/preferences/preference_location.rs:39,42` (`LocationDecision::Store`) |
| persistiert ihn | Settings-Schlüssel `location.name`; schreiben `location::store()` (`location.rs:75-86`), lesen `app_location_in()` (`location.rs:60-73`) |

Drei Dinge folgen daraus, und ein Fix braucht vermutlich alle drei:

1. **Die Stadt steht schon in der Antwort.** Die Geocoder-URL
   (`geocode.rs:18-22`) fragt bereits `addressdetails=1` an — das
   `address`-Objekt mit `city`/`town`/`village`/`municipality` kommt also mit
   und wird schlicht ignoriert. Der Fix ist eine Feldwahl mit Fallback-Kette,
   kein zusätzlicher Abruf.
2. **Die Sprache wird nie erbeten.** Weder in der URL noch im HTTP-Aufruf
   (`crates/reprise-core/src/concerts/http.rs:42-72`) steht ein
   `Accept-Language`. Deshalb kommt der viersprachige Landesname. Der
   Oberflächen-Locale muss mitgeschickt werden.
3. **Der Altbestand heilt nicht von selbst.** Der lange Name liegt in der
   Datenbank unter `location.name`, nicht nur in der Anzeige. Eine reine
   Anzeige-Kürzung repariert bestehende Installationen nicht — es braucht eine
   Migration oder ein erneutes Geocodieren beim Lesen.

**Noch offen und ungeprüft:** ob CONC-2 einen Test trägt und was der misst
(vermutlich mit einem bereits kurzen Stadtnamen, sonst wäre er rot); wie sich
das zum abgeschlossenen `docs/plans/location-is-not-a-concerts-setting.md`
verhält; und ob die Kürzung auch andere Flächen betrifft, die
`AppLocation.name` anzeigen.

Das gehört in einen eigenen Plan. Es fasst andere Dateien als die Feed-Tags
und wurde deshalb ausdrücklich **nicht** in diese Runde gezogen.

## 4. Der Stand ringsum

- **Der Blocker der letzten Übergabe ist weg.** `#502`
  („The delete-failure case is measured, not assumed", `b6be7cdc61`) hat den
  `worktree-gc`-Defekt behoben und liegt auf `dev`.
- **Die Promotion `dev` → `main` steht weiter aus.** `origin/main` liegt auf
  `4912275130`, `dev` ist **42 Commits** voraus, Fast-Forward wäre sauber. Die
  Sitzung mit dem Wake-Lock `showcase-relaunch` („worktree-gc root-Fix,
  **Promotion**, Pages") hat sie sich vorgenommen — hier wurde bewusst nicht
  hineingegriffen. Der Push gehört laut AGENTS.md ohnehin dem Eigentümer.
- **Falle bei der Promotion:** `delete_branch_on_merge` muss aus bleiben, sonst
  nimmt der Merge `origin/dev` mit. `git push origin dev:dev` holt ihn zurück.

## 5. Was noch aufzuräumen ist

Unverändert aus der Vorgänger-Übergabe, nichts davon angefasst:

- **`/home/marvin/Projects/reprise-ucr-acceptance`** — Abnahme-Worktree mit
  4 GB warmem Debug-Build. Ein Neubau kostet gut anderthalb Stunden, deshalb
  nicht ungefragt gelöscht.
- **`/home/marvin/Projects/reprise-ucr-release`** — nur für eine einzelne
  Release-Gate-Messung angelegt, kann weg.
- Der Hauptcheckout `/home/marvin/Projects/reprise` steht auf einem **fremden,
  detachten Stand** (`be5f014d3b`), nicht auf `dev`. Wer dort misst, misst den
  falschen Baum. Alle Befunde dieser Sitzung wurden deshalb per
  `git show origin/dev:<pfad>` gelesen, nicht aus dem Arbeitsbaum.
- **Wake-Lock `ucr-open-questions`** wird von dieser Sitzung gehalten und
  gehört freigegeben, wenn niemand mehr daran arbeitet
  (`wake-lock release ucr-open-questions`).

## 6. Belege

Alles Dauerhafte steht in den beiden Plandateien. Diese Sitzung hat keinen
Prüfstand gebaut und keine Aufnahme gemacht — sie hat entschieden, geplant und
gegrillt. Die Tatsachenbehauptungen des Plans sind gegen `origin/dev` @
`b6be7cdc61` geprüft; wo eine Zeilennummer verrutscht, gilt der im Plan
genannte Bezeichner.
