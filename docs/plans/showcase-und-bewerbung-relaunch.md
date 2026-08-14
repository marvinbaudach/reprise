---
slug: showcase-und-bewerbung-relaunch
created: 2026-08-13
kind: spec
status: abgestimmt — nicht freigegeben, nichts gebaut
baseline: f38f8556c78021dc23edc81c5352c9fde32e3b4b
---

# Spezifikation — Showroom- und Bewerbungs-Relaunch

Nachfolger von `showcase-und-bewerbung-relaunch.HANDOFF.md`. Die Handover bleibt
gültig für Repo-Pfade, Werkzeuge und Umgebungsfallen; **ihre Zahlen sind
veraltet** (gemessen gegen `5995f70e77` vom 11.08.). Maßstab ist ab jetzt
`origin/dev` = **`f38f8556c7`** (13.08.2026, 14:19).

## 1 Auftrag und Leser

Showcase-Repo und Bewerbungsunterlagen neu bauen. Verwendungszweck: Bewerbungen.

**Der Leser**, für den jede Entscheidung getroffen wird: jemand mit
IT-Verständnis, der in wenigen Sekunden entscheidet, ob er weiterliest.

Ihn stoppt: eine Zahl, die eine Haltung ist. Eine Behauptung, die seiner
Erwartung widerspricht. Ein Link, den er nachprüfen kann.

Ihn verliert: große LOC-Zahlen (er liest sie als Wildwuchs), Adjektive ohne
Beleg, „aims to" / „strives for", Feature-Aufzählungen, „About me"-Prosa.

## 2 Fünf Aussagen, die belegt werden

1. **Ein Rust-Kern, vier Frontends** — und der Preis jedes weiteren ist gemessen.
2. **So entsteht die Arbeit** — Spec, Grilling, TDD, Gewaltenteilung zwischen
   Modellen, widerlegte Reviews, Gates. Plus Kontextmanagement als Untergrund.
3. **Agenten schreiben es, Agenten greifen es an** — fünf Verifikationsstufen,
   die oberen zwei agentengetrieben.
4. **Gemessen, nicht behauptet** — Performance mit Vorher/Nachher.
5. **So sieht es aus** — Design als Vertrag, Spectral Seek, Atmosphäre,
   Visualisierung.

## 3 Positionierung — entschieden

**Haltung zuerst.** Nicht die Agenten, nicht das Produkt.

Grund: Der Agenten-Aufmacher öffnet beim Bewerbungsleser sofort die
unausgesprochene Frage *„und was konnte er selbst?"*. Solange sie unbeantwortet
im Raum steht, arbeitet sie gegen die Bewerbung.

### Aufmacher-Copy

```
                    R E P R I S E

     One Rust core. Two native apps.
     Nothing merges because an agent said it was done.

  [Rust 2021] [GTK4 · libadwaita] [Kotlin · Media3]
  [45% tests] [<N> Rust tests] [<N> merge gates]

  ┌──────────────────────────────────────────────┐
  │   ▶  Live-Spektrum, Endlosschleife, stumm    │
  └──────────────────────────────────────────────┘

  I didn't write most of the lines.
  I wrote the rules that decide which lines survive.
```

Deutsch:

> **Ein Rust-Kern. Zwei native Apps. Nichts wird gemergt, weil ein Agent sagt,
> es sei fertig.**
>
> Die meisten Zeilen habe ich nicht geschrieben. Ich habe die Regeln
> geschrieben, die entscheiden, welche Zeilen überleben.

Der letzte Satz ist der wichtigste des Dokuments. Er kippt den Verdacht in die
Pointe: nicht „hat delegiert", sondern „hat das System gebaut, dem man beim
Delegieren vertrauen kann".

**Bewegtbild gehört über die Falz** — der Spektrum-Clip direkt unter die
Badge-Zeile, nicht in eine Galerie weiter unten. Bewegung im ersten Bildschirm
ist der stärkste Bleib-hier-Reiz, den die Seite hat.

## 4 Zahlenpolitik

**LOC nur als Verhältnis, nie als Volumen.** Die beiden LOC-Badges
(`217.8k product` / `89.0k test`) werden **ersatzlos gestrichen**. Auch die
korrigierten Zahlen gehören nicht in die Badge-Zeile — 174k liest sich
genauso nach Wildwuchs wie 218k. Ihr Platz ist unter dem Balkendiagramm, als
Verhältnis:

> Das Android-Frontend kostete 25'932 Zeilen, weil es 114'292 Zeilen Kern nicht
> noch einmal schreiben musste.

**Die 45 % brauchen ihren Nachsatz**, direkt daneben, nicht drei Absätze später
— sonst fragt der Leser, ob 142k Zeilen Test nicht schlicht Redundanz sind:

> Tests are named after the UX rule they defend. The rulebook and the suite are
> the same index — a rule ID takes you to the test, the test to the commit, the
> commit to the decision record.

**Methodikwechsel mit Fußnote.** Die Umstellung auf die strenge
`#[cfg(test)]`-Trennung ändert die Zahlen ohne Codeänderung. Das muss
dastehen, sonst sieht es nach Schönung aus.

## 5 Belegprinzip

Jede Behauptung bekommt einen **Permalink auf `origin/dev`**. Das ist der
Unterschied zwischen diesem Showroom und einer hübschen Portfolioseite — und es
ist erst möglich, seit klar ist, dass `marvinbaudach/reprise` öffentlich ist.

| Behauptung | Beleg |
|---|---|
| Architektur wird maschinell erzwungen | `scripts/check-architecture.sh`, `scripts/tests/architecture-size-limits.sh` |
| Kern hat keine Oberfläche | `crates/reprise-core/Cargo.toml` |
| Zustand lebt in Dateien | `docs/plans/`, `docs/superpowers/` |
| Design ist ein Vertrag | `docs/ux-rules.md` |
| Agentenoberfläche | `crates/reprise-mcp/src/` |
| Visualisierung im geteilten Kern | `crates/reprise-core/src/visuals/` |

## 6 Kapitel und Überschriften

Wer überfliegt, liest nur Überschriften. Sie müssen allein gelesen die
Geschichte erzählen:

1. *One core, two frontends — and the bill for the second one.*
2. *How the work actually happens.*
3. *Agents write it. Agents also try to break it.*
4. *Measured, not claimed.*
5. *This is what it looks like.*

Fünf Kapitel, nicht sechs. Der Absatz unter jeder Überschrift ist
Bildunterschrift, kein Fließtext.

### Kapitel 2 ist das Herzstück

Es enthält die drei Aussagen, die das heutige README **gar nicht** macht:

**Gewaltenteilung zwischen Modellen.**

> The model that writes the code never reviews it. The model that reviews it
> never writes it. Not a convention — the review agent is structurally
> forbidden from applying its own findings. They go back to the implementer.

**Der Review wird selbst überprüft.**

> Reviews are not trusted either. Every finding is handed to a skeptic agent
> whose job is to refute it. Only findings that survive the refutation are
> allowed to change code.

**Der Mensch steht an der richtigen Stelle.**

> One human checkpoint, deliberately placed: the plan gets interrogated before a
> line is written. Everything after that is machinery.

**Wichtig:** Das Kapitel behauptet **nicht** „ich habe die Pipeline gebaut". Es
behauptet: *so läuft dieses Projekt, und das Repo belegt jede Station*. Damit
ist die Autorenfrage nicht im Raum und die Belege sind härter als jede
Selbstbeschreibung.

**Keine Skill-Behauptungen.** Der Nutzer nutzt Skills, schreibt sie nicht. Keine
Zahl, kein Link, keine Erwähnung.

### Kontextmanagement — zweiter Satz von Kapitel 2

> **334,000 lines don't fit in a context window. Neither does the reason last
> Tuesday's fix failed.**
>
> Every task is anchored to a plan file whose frontmatter carries its worktree,
> branch and phase — so it survives a closed terminal, a cleared session,
> another machine. Wide reading happens in throwaway agents that die with their
> context; only the verdict comes back. When a session ends mid-task, it writes
> a handover.

Belegt durch: **168 Dateien unter `docs/`**, davon **66 unter `docs/plans/`**
(3 Handover) und **67 unter `docs/superpowers/`**; `docs/ux-rules.md` mit
**5'957 Zeilen**. Alles gemessen gegen `f38f8556c7`.

## 7 Sechs SVG-Abbildungen

Bindend bleibt `docs/showcase.md`: SVG statt Mermaid, 1440×900, dunkler Grund,
Produktfarbe für Produkt / Mint für belegte Ergebnisse / Amber nur für Kosten,
SVG-Titel und Alt-Text bei jeder Abbildung.

### A · Kern und Kanten — *überarbeitet, Kernaussage neu*

Vier **existierende** Frontends, nicht zwei. `reprise-mcp` und `reprise-cli`
sind Geschwister von `reprise-gnome`, keine Werkzeuge am Rand.

```
 GNOME/GTK4   Android/Kotlin   Agent · MCP   CLI      ┌ nächstes ┐
  157'005        25'932          11'392     4'079     └ ─ ─ ─ ─ ┘
      └──────────────┴────────┬──────┴────────┘            │
                              ▼   Abhängigkeit nach innen ─┘
                   reprise-core + reprise-view
                   114'292 Zeilen · 0 UI-Abhängigkeiten
```

Der gestrichelte Steckplatz nennt **einen Preis, kein Produkt**. Damit wird
„beliebig ausbaubar" von einer Prognose zu einer Erfolgsbilanz: viermal
gemacht, jedes Mal mit gemessenen Kosten.

**Kein benanntes Zukunftsprodukt in der Abbildung** — kein Tauri, kein iOS, kein
Web. Sobald ein Name dasteht, ist es ein Versprechen und der Leser wartet auf
Einlösung. Ein Preisschild dagegen ist eine Aussage über *heute*: so viel hat
das letzte Frontend gekostet, so viel kostet das nächste.

Beleg in einer Datei: `reprise-core` hat **19 Abhängigkeiten** und keine davon
ist eine Oberfläche — kein GTK, kein glib, kein libadwaita, kein JNI.

> `reprise-core` has nineteen dependencies and not one of them is a user
> interface. Not by convention — by a check that fails the build.
>
> That is why the second frontend cost 25,932 lines instead of a rewrite. And it
> is why the third one already has a price tag.

### B · Codeverhältnis — *neu*

Balken über die volle Breite, segmentiert nach Kern / GNOME / Android / Agent /
CLI / Plattform. Darunter derselbe Balken nach Produkt vs. Test.

### F · Wie die Arbeit entsteht — *neu, wichtigste Abbildung*

```
   ZIEL
    │
    ▼
 ┌────────────────┐   grilling   ┌──────────────┐
 │ PLAN           │◄────────────►│ MENSCH       │ ← der einzige Halt
 │ Opus · max     │  Ast für Ast │ Urteil       │
 └───────┬────────┘              └──────────────┘
         │  Planfile = Spec + Statusblock
         ▼
 ┌────────────────┐
 │ CODE           │  Codex · headless · sandboxed worktree
 │ rot/grün       │  keine Approval-Umgehung
 └───────┬────────┘
         ▼
 ┌────────────────┐  je Befund  ┌──────────────┐
 │ CHECK          │────────────►│ SKEPTIKER    │ widerlegt
 │ je Sprache     │◄────────────┤ nur Über-    │
 │                │             │ lebende      │
 └───────┬────────┘             └──────────────┘
         ▼
 ┌────────────────┐
 │ REFACTOR       │  zurück an Codex — nie an den Reviewer
 └───────┬────────┘
         ▼
     <N> GATES ─────► MERGE

 ══════════════════════════════════════════════════════
   SUBSTRAT — jede Station liest daraus
   Planfiles · Regelwerk · Handover · Entscheidungsprotokolle
```

Das Substrat-Band ist Kontextmanagement. Kein eigenes Diagramm: der Kontext ist
kein Schritt, er ist der Untergrund.

### C · Verifikationsstufen

Fünf Stufen als Treppe, je mit „kann beweisen / kann nicht beweisen". Die
oberen zwei agentengetrieben.

### D · Explorations-Bot — *neu*

Kreislauf: AT-SPI-Baum lesen → selbst klicken → Hauptthread-Stalls messen →
Anomalie melden → Befundbericht → Triage → Task mit Regel-ID → gleichnamiger
Test → Gate. Mit **echten** Befunden beschriftet.

### E · Performance

Messen / ändern / vergleichen. Zwei Fälle: Index-Optimierung und
Leerlauf-Frametakt.

## 8 Vier Videoclips

Maßstab: **zeigt Bewegung etwas, das ein Standbild nicht kann?** Wenn nein,
kein Clip.

| Clip | Warum er trägt |
|---|---|
| **Live-Spektrum, Desktop** | Erst Bewegung zeigt, dass es auf echte Musik reagiert und dass die Suchleiste *das Spektrum selbst ist*. Die Signaturinteraktion. |
| **Desktop + Handy nebeneinander, derselbe Track** | Macht „ein Kern, zwei native Apps" in vier Sekunden fühlbar. Bisher existiert die Aussage nur als Zahl. |
| **Virtualisiertes Scrollen** | Nur mit **sichtbarem Maßstab** im Bild. „Eine Tabelle scrollt" ist langweilig; „200'000 Zeilen rasen vorbei" ist ein Beweis. |
| **Explorations-Bot bedient die App** | Zeiger bewegt sich ohne Hand, Fokusringe springen, Befundbericht erscheint. Als Standbild nicht darstellbar. |

**Gestrichen:** Geräte-Sync mit Fortschritt (sich füllende Balken sind die
uninteressanteste Bewegung, die Software hat), Android-Tabwechsel (reine
Navigation).

**Format:** stumm, 6–12 s, Endlosschleife, ≤ 2 MB, headless aufgenommen. MP4 für
Pages, WebP-Standbild fürs README. Kein Git-LFS nötig.

**Harte Randbedingung:** Android darf **nicht** für Flüssigkeit herhalten.
Emulator-Aufnahmen lösen 60 Hz nicht auf, Debug-Builds ruckeln zu 97,7 % —
beides gemessen. Der Handy-Anteil zeigt Oberfläche und Gleichheit des Kerns,
nie Performance. Die kommt vom Desktop.

## 9 Kapitel 5 — Design

Vier Anker, vom Nutzer gewählt:

**Design als Vertrag.** Das UX-Regelwerk: bindende Regeln mit IDs, jede von
einem gleichnamigen Test bewacht. Design als überprüfbare Zusage statt als
Geschmack. Der stärkste Anker für einen technischen Leser.

**Spectral Seek.** Suchleiste und Spektrum sind ein Objekt. Auf Desktop und
Handy gleich.

**Cover-getragene Atmosphäre.** Nebel und Cover-Bloom nehmen ihre Farbe aus dem
Artwork, akzentbewusst, respektiert `prefers-reduced-motion`.

**Die Visualisierung als geteiltes Artefakt.** Strukturell der wertvollste Teil
des Showrooms, weil er **beide** Kernaussagen gleichzeitig beweist:

```
reprise-core/src/visuals/{engine,scene,modes,color}.rs
reprise-core/src/playback/cava/{bands,smoothing}.rs
reprise-core/src/playback/spectral.rs
reprise-android-ffi/src/visualizer.rs   ← nur die Brücke
```

Physik, Bänder, Glättung, Farbgebung liegen im geteilten Kern. Android rendert
es, schreibt es nicht neu. Damit ist der Split-Screen-Clip kein Beiwerk, sondern
der Beweis.

Format je Anker: **Entscheidung → Umsetzung → Ergebnis**. Screenshots zeigen ein
Ergebnis, keine Entscheidung — deshalb reicht eine Galerie nicht.

## 10 Die Agentenoberfläche

`reprise-mcp` exponiert **24 Tools** (gezählt gegen `f38f8556c7`):

```
music_search_tracks · music_search_artists · music_search_albums · music_search_sources
music_play · music_playback_control · music_set_playback · music_get_playback_state · music_queue
music_create_playlist · music_get_playlist · music_update_playlist
music_scan_tags · music_review_tags · music_apply_tags
music_manage_podcasts · music_manage_episodes · music_manage_radio
music_manage_online_sources · music_get_channel_detail
music_device_sync · music_get_device_sync_state · music_create_instrumental · music_get_job_status
```

Copy:

> **Anything the window does, an agent can do too.** 24 MCP tools: search,
> playback, queue, playlists, tag repair, podcasts, radio, device sync. Not an
> API bolted onto a GUI — the agent surface sits on the same core as the window,
> as a peer.

**Nicht** „alles was das GUI kann" behaupten, solange es nicht gemessen ist:
Statistiken, Konzerte, Lyrics und Spektrogramm tauchen in der Tool-Liste nicht
auf. Die Domänenliste ist stark genug und hält einer Prüfung stand.

## 11 Was raus muss

- **„The production source is private to preserve a commercial option."** —
  nachweislich falsch, `marvinbaudach/reprise` ist öffentlich. In zehn Sekunden
  widerlegbar.
- **Tauri — ERLEDIGT am 13.08., lokal, nicht gepusht.** Es gibt kein Tauri in
  Reprise und es ist keins geplant. Das README behauptete es korrekt als
  Roadmap-Ziel — also kein falscher Satz, sondern ein **Versprechen**. Und ein
  Versprechen ist der schwächste Inhalt einer Bewerbungsseite: das Einzige, was
  der Leser nicht nachprüfen kann, und es kann nur schlecht altern.

  Entfernt aus: `README.md` und `README.de.md` (je Roadmap-Absatz, Alt-Text,
  Aufzählungspunkt, Schlusssatz; „drei Richtungen" → „zwei"), beiden
  Architektur-SVGs (Beschreibung, Unterzeile, Frontend-Pille; die zwei
  verbliebenen Pillen auf je 600 px neu ausbalanciert), sowie **6 Assertions** —
  `scripts/check-showcase.sh` (4) und `scripts/tests/readme-evidence.sh` (2).

  Verifiziert: 0 Rest-Treffer für „tauri" im Repo, `xmllint` grün für beide
  SVGs, `check-showcase.sh` und `readme-evidence.sh` beide Exit 0, beide
  Abbildungen gerendert und gesichtet.

  Die Unterzeile des SVG trug das Versprechen ohne das Wort weiter („an explicit
  sequence for the next UI codebase") und wurde ersetzt durch die Aussage über
  heute: *„…and a boundary a check refuses to let them cross."*

  Ersatz beim Neubau von Abbildung A bleibt der gestrichelte Steckplatz:
  **ein Preis, kein Produktname.**
- **Die beiden LOC-Badges.**
- **`Impact-Site-Verification`** — bereits entfernt aus `README.md`,
  `README.de.md` und der Assertion in `scripts/tests/readme-evidence.sh`
  (lokal, nicht gepusht).

## 12 Zwei Bühnen

**README** — scanbare Kurzfassung: Wordmark, Aufmacher, sechs Kennzahlen,
WebP-Standbild, drei SVGs, Screenshot-Galerie, Link zur Seite. Zweisprachig.

**GitHub Pages** — dunkles Studio-Layout, editorial statt Karten-Raster, große
Zahlen als Ankerpunkte, Kapitelnavigation, Diagramme bauen sich beim Scrollen
auf. Quelle `main`, Ordner `/ (root)`, HTTPS erzwungen. URL:
`https://marvinbaudach.github.io/reprise-showcase/`.

Der Dreh: **die Seite hält sich an dieselben Regeln wie das Projekt** —
sichtbare Fokusringe, `prefers-reduced-motion` schaltet Bewegung ab, geprüfte
Kontraste. Steht als Fußzeile drunter und ist selbst ein Beleg.

Neue Dateien: `index.html`, `de.html`, leere `.nojekyll`, `assets/` für Videos
und SVGs.

**Empfehlung:** GitHub Pages im `reprise`-Repo wieder abschalten. Kein Leck,
aber es öffnet eine zweite URL für dieselbe Sache und verwässert die Adresse,
die in Bewerbungen steht.

## 13 Bewerbungsunterlagen

- **CV-Karte auf Seite 1** neu bauen. Farbe bleibt **Indigo/Violett**.
- **Neues einseitiges PDF „Projektsteckbrief Reprise"** als Beilage.

Verbunden mit dem Showroom (App-Teal) über ein gemeinsames Bildsystem: gleiche
Diagrammsprache, Typografie, Zahlendarstellung.

**Offen:** Eine Seite ist knapp für Diagramme plus Screenshots. Entweder zwei
Seiten oder nur zwei Diagramme — Entscheidung bei der Umsetzung, mit Vorlage.

## 14 Offene Messungen — vor Veröffentlichung zu erledigen

Nichts davon darf aus dem Gedächtnis oder aus der Handover übernommen werden.

| Was | Warum offen |
|---|---|
| **Alle LOC-Zahlen** | Handover misst gegen `5995f70e77` (11.08.). Neu gegen `f38f8556c7`. |
| **Test-Zahlen Rust und Android** | dito |
| **Aktive UX-Regeln** | 519 Regel-IDs, 158 Zeilen mit Ersetzt-/Withdrawn-Marker — teils in Erklärtexten. Exakt zählen. |
| **Gate-Anzahl** | README nennt 18. Gegen `dev` nachzählen. |
| **Produkt/Test je Crate** | Für Abbildung B nötig. Analyzer liefert nur Baumsummen; Gruppierung nach Pfadpräfix ergänzen. |
| **GUI↔MCP-Parität** | Nur messen, falls die starke Fassung gewünscht ist. Sonst Domänenliste. |

Tauri ist **geklärt** (13.08.): existiert nicht, ist nicht geplant, fliegt raus —
siehe §11.

Reproduktion:

```bash
cd /home/marvin/Projects/bewerbung/.skill-staging/update-reprise-code-stats
./scripts/reprise-stats.sh /home/marvin/Projects/reprise f38f8556c7
```

## 15 Reihenfolge

1. Zahlen neu vermessen gegen `f38f8556c7`, offene Messungen abschließen
2. Sechs SVGs
3. Vier Clips aufnehmen
4. README neu
5. Pages-Seite
6. CV-Karte und Projektsteckbrief
7. Gegenlesen — jede Zahl gegen `f38f8556c7` verifiziert, jeder Beleglink geprüft

## 16 Fallen in dieser Umgebung

- `origin/dev` ist der Maßstab, nicht der lokale Checkout (`48489dde5e`), nicht `main`.
- Pages deployt aus `main` — Arbeit auf einem Branch wird erst nach dem Merge live.
- Der Load-Governor-Hook blockt Bash-Kommandos, die nach schweren Einstiegspunkten
  aussehen. Umformulieren oder `HEAVY_RUN_DISABLE=1` bei reiner Textsuche.
- Bash-Ausgabe kappen — lange Läufe in eine Datei, Frage per `grep`/`wc`.
- **Nicht ohne Freigabe pushen.** Weder Showcase noch Bewerbung.
