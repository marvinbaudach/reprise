---
slug: updates-concerts-releases-rework-3
worktree: /home/marvin/Projects/reprise-updates-concerts-releases-rework-3
branch: feature/updates-concerts-releases-rework-3
phase: shipped
codex_session:
created: 2026-08-14
---
# Strang 3 — `update-notifications`

> **Lies zuerst den Mutterplan:**
> `docs/plans/updates-concerts-releases-rework.md`. Er trägt die Ausgangslage
> (§0), alle 18 Beschlüsse (§1), die englischen Quellstrings (§3), den
> vollständigen UX-Regeltext (§4), die Abnahme (§5), die Abgrenzung (§6), die
> Parallelität (§7) und die Nachträge der Schlussprüfung (§8). Diese Datei sagt, **was**
> zu tun ist; der Mutterplan sagt, **warum**. Wo beide sich zu widersprechen
> scheinen, gewinnt der Mutterplan.

> Zeilennummern gegen `origin/dev` @ `5721ade95e`. Von den Dateien dieses
> Strangs hat sich seither keine geändert; `docs/ux-rules.md` liegt ab `:1062`
> um **+1** verschoben (Abschnitt H also `:1320`). Der Hauptcheckout ist
> geteilt — nicht umschalten, per `git show origin/dev:<pfad>` lesen.

## Zweck

Dieser Strang meldet auf dem Desktop, wenn ein Release seinen
Veröffentlichungstag erreicht — **genau einmal**, und **nie beim ersten
Fetch** — und gibt dem Nutzer die dreistufige Einstellung, mit der er das
steuert. Er enthält Paket E vollständig.

**Er hat keine Schema-Arbeit.** Das ist die Änderung aus dem Grilling: die
Spalte `new_releases.notified_released_at` legt **Strang 1** an, zusammen mit
seiner eigenen, damit `crates/reprise-core/src/db.rs` genau einen Besitzer
hat. Dieser Strang liest und schreibt die Spalte nur.

## Dateibesitz

```
crates/reprise-core/src/artist_news_notify.rs          (neu)
crates/reprise-core/src/artist_news_query.rs           (nur: notified_released_at
                                                        lesen/schreiben)
crates/reprise-gnome/src/ui/notifications.rs
crates/reprise-gnome/src/ui/notifications_updates.rs   (neu)
crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs
crates/reprise-gnome/src/ui/strings_notifications.rs   (neu)
crates/reprise-gnome/src/ui/strings.rs                 (nur: mod-Deklaration + pub use)
po/POTFILES.in                                         (nur: eine Zeile)
docs/ux-rules.md                                       (NUR Abschnitt H: OS-6, OS-7)
```

**Ausdrücklich NICHT — und das ist die Grill-Änderung gegenüber dem Entwurf:**

```
crates/reprise-core/src/db.rs                     → Strang 1 (ALLEINBESITZ)
crates/reprise-core/src/db_new_releases_notify.rs → Strang 1 (trägt migrate_v74)
```

Dieser Strang legt **keine** Migration an, fasst `SUPPORTED_SCHEMA_VERSION`
**nicht** an und schreibt keine `CREATE`- oder `ALTER TABLE`-Anweisung. Findet
sich in seinem Diff eine, ist der Schnitt verletzt.

Weiter nicht:

```
crates/reprise-gnome/src/ui/updates/**            → Strang 2 (auch feed_snapshot.rs!)
crates/reprise-gnome/src/ui/releases/**           → Strang 2
crates/reprise-gnome/src/ui/strings_news.rs       → Strang 2
crates/reprise-gnome/src/ui/strings_releases.rs   → Strang 2
crates/reprise-gnome/src/ui/concerts/**           → Strang 1
crates/reprise-gnome/src/ui/strings_concerts.rs   → Strang 1
crates/reprise-gnome/src/ui/feed_footer.rs        → Strang 1
crates/reprise-core/src/concerts/**               → Strang 1
```

In `docs/ux-rules.md` wird **ausschließlich** Abschnitt H beschrieben. Der
Abschnitt ist bewusst gewählt: dort steht heute **keine** `[active]`-Regel
(OS-1…OS-5 sind alle `[planned]`, `:1321`ff), eine Desktop-Benachrichtigung
ist per Definition OS-Integration — und damit muss dieser Strang **nicht** in
Abschnitt R schreiben, wo Strang 2 arbeitet.

## Vorbedingungen

Dieser Strang braucht **eine** Sache aus Strang 1: die Spalte
`new_releases.notified_released_at` (`migrate_v74`) und
`SUPPORTED_SCHEMA_VERSION == 74`.

**Prüfung vor dem Start:** Findet sich auf der Basis kein `migrate_v74` oder
steht `SUPPORTED_SCHEMA_VERSION` unter `74`, ist dieser Strang zu früh dran —
**erst auf den gemergten Strang 1 rebasen, dann beginnen.** Vor dem Rebase
`origin/dev` frisch fetchen.

Das ist ein harter, sofort sichtbarer Stopp: ohne die Spalte kompiliert die
Query in Aufgabe 1 nicht. Es gibt hier keinen stillen Fehlerpfad — und genau
deshalb ist die Reihenfolge sicher.

Von Strang 2 braucht dieser Strang **nichts**; beide dürfen parallel laufen.
Ihre Dateimengen sind disjunkt — die einzige Berührung ist, dass beide
Strings schreiben, aber in verschiedene Dateien (Beschluss 17).

---

## Aufgaben

### 1. Der Kern: wer heute gemeldet wird

Neue Datei `crates/reprise-core/src/artist_news_notify.rs` mit
`released_today_candidates(db, run_started_at, today)`.

Ein `StoredRelease` (`artist_news_query.rs:30-44`) wird gemeldet, wenn
**alle drei** Bedingungen gelten (Beschluss 13):

1. `release_kind(...) == NewsKind::New` **und** `first_release_date == heute`
   — das Datum ist *gerade* erreicht, nicht irgendwann in den letzten 90
   Tagen. Der Statuswechsel Upcoming → Released ist ohnehin kein
   Fetch-Ereignis: `announcement_kind()` (`artist_news_parsing.rs:294`)
   entscheidet ihn allein aus dem Datum gegen heute. Ein Release wird
   released, weil ein Tag vergeht, nicht weil eine Antwort eintrifft.
2. `fetched_at < run_started_at` — die Zeile stand **vorher schon** in der
   Liste. Beim allerersten Fetch ist jede Zeile in diesem Lauf entstanden,
   also feuert nichts. **Das ist die Ausnahme des ersten Fetches, und sie
   braucht keine Sonderbehandlung** — kein Flag, kein „ist das der erste
   Lauf?"-Zweig.
3. `notified_released_at IS NULL` — sonst meldete die stündliche
   Fälligkeitsprüfung dasselbe Release bis zu 24-mal am selben Tag.

`seen_at` beantwortet „gesehen", nicht „war vorher upcoming", und wird hier
bewusst **nicht** benutzt.

Dazu `mark_release_notified(db, mbid, now)`, das die Spalte stempelt —
**unmittelbar nach** dem erfolgreichen `send_notification`, nicht davor.

Beide Datenbankzugriffe laufen über `artist_news_query.rs`; Vorbilder für
Form und Sichtbarkeit sind dort `delta_candidates` (`:318`) und
`mark_releases_seen` (`:434`).

**Ziel:** Eine reine, testbare Auswahlfunktion, die ohne Uhr und ohne UI
auskommt.
**Nachweis:** `os_6_the_first_fetch_announces_nothing` grün (siehe
UX-Regeln), plus ein zweiter Test, der beweist, dass eine bereits gemeldete
Zeile **nicht erneut** meldet.

### 2. Kernreinheit

`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` muss
**leer** bleiben. Die Auswahl gehört in den Kern, `gio::Notification` **nicht**
— die Grenze verläuft zwischen „wer wird gemeldet" (Kern) und „wie sieht die
Meldung aus" (GNOME).

**Ziel:** Die Meldelogik bleibt portabel.
**Nachweis:** der Befehl gibt nichts aus — nach **jeder** `reprise-core`-
Änderung erneut laufen lassen, nicht nur am Ende.

### 3. Die Einstellung `updates.notifications`

- **Key:** `updates.notifications` in der SQLite-Tabelle `settings`;
  Zugriffsschicht `crates/reprise-core/src/library/settings.rs` und
  `settings_api.rs` (die Zeilennummern des Entwurfs ließen sich nicht
  bestätigen — am Funktionsnamen orientieren, Nachtrag 3 des
  Mutterplans).
- **Werte:** `off` | `releases` | `all`, **Vorgabe `releases`**.
- **Kein GSettings.** GP-6 bleibt `[planned]`; der Key lebt in der
  `settings`-Tabelle wie alle anderen.
- Ist das Concerts-Modul aus, verhält sich `all` wie `releases`.

**Ziel:** Ein gelesener und geschriebener Key mit einer Vorgabe, die auch
ohne Eintrag gilt.
**Nachweis:** ein Kern-Test liest den Key aus einer frischen Datenbank und
bekommt `releases`, nicht `off` und keinen Fehler.

### 4. Die Meldungen selbst

Neue Datei `crates/reprise-gnome/src/ui/notifications_updates.rs`, nach der
Hausform von `ui/notifications.rs` (72 Z., `notify_now_playing` `:18`,
`send_notification` `:31`/`:53`, Generationsschutz `generation_is_current`
`:13`).

| Fall | `send_notification`-ID | Titel | Body |
|---|---|---|---|
| 1–3 Releases | `updates-release-{release_group_mbid}` | der **Releasetitel** (Daten, nicht übersetzt) | `{artist} · {type} · out today` |
| ≥4 Releases | `updates-releases` (eine einzige) | `{count} releases are out` (Plural) | die ersten drei Künstlernamen, mit `·` verbunden |
| Concerts (`all`) | `updates-concerts` (eine einzige je Lauf) | `{count} new concerts` (Plural) | `{artist} · {city} · {date}` des ersten Eintrags |

Die stabile, releasebezogene ID sorgt dafür, dass ein wiederholter Versand
die alte Meldung **ersetzt statt stapelt**. Der Deckel bei 4 verhindert, dass
ein Freitag mit acht Veröffentlichungen den Benachrichtigungsschirm flutet.

**Kein Kicker.** Der Entwurf zeigt „Jetzt erschienen" als eigene Zeile;
`gio::Notification` hat dafür **keinen Platz** (nur Titel, Body, Icon) — der
Kicker-Slot ist bei GNOME der App-Name in der Kopfzeile. Der Sinn („warum
kommt das jetzt?") wandert in den Body: `… · out today`. Bewusste Abweichung
vom Entwurf, keine Auslassung; **Titel = Releasetitel** ist ausdrücklich
gegen die Alternative „Titel = Just released" bestätigt.

**Cover:** wie `notify_now_playing()` (`notifications.rs:33-54`) asynchron
nachgeladen und mit demselben Generationszähler gegen veraltete Treffer
gesichert. Ohne Cover geht die Meldung **ohne** Icon raus — nicht verzögert,
nicht unterdrückt.

**Ziel:** Ein Release, dessen Datum heute erreicht wird und das gestern schon
in der Liste stand, erzeugt beim nächsten Lauf **genau eine** Meldung mit
Cover; ein zweiter Lauf am selben Tag erzeugt nichts.
**Nachweis:** Screenshot 8 der Abnahme (die Benachrichtigung mit Cover), plus
die Messung „Der erste Fetch meldet nichts" aus §5.2 des Mutterplans:
frische Datenbank, Fetch mit einem heute erscheinenden Release → keine
Meldung; danach ein zweiter Lauf → genau eine.

### 5. Der Klick auf die Meldung

`notification.set_default_action_and_target_value("app.open-updates-link", &url.to_variant())`,
dazu auf der `gtk4::Application`:

- `gio::SimpleAction::new("open-updates-link", Some(glib::VariantTy::STRING))`
- `gio::SimpleAction::new("open-updates-view", Some(glib::VariantTy::STRING))`
  mit Ziel `"releases"` bzw. `"concerts"` für die gesammelten Meldungen.

Hausform für Aktionen ist `gio::SimpleAction` (Vorbild
`compact/compact_player_menu.rs:58ff`); `ui/notifications.rs` setzt heute
**keine** `default`-Aktion, das ist also neu.

Der URL-Handler schickt die URL **erneut** durch
`external_link::launch()` (`ui/external_link.rs:23-44`). Der zweite
`is_launchable_url`-Test dort ist kein Doppelmoppel: der Wert stammt aus
Anbieter-JSON und kommt über den D-Bus-Umweg zurück in den Prozess.

Die URL ist **dieselbe**, die die Popover-Zeile öffnet (NR-11-Priorität),
damit Meldung und Zeile nie auseinanderlaufen.

**Ziel:** Ein Klick auf die Meldung öffnet exakt das, was ein Klick auf die
Zeile öffnen würde.
**Nachweis:** ein Test vergleicht die beiden **Ergebniswerte** (nicht die
Implementierungen). Der vollständige Vergleich gegen Strang 2s echte Zeile
ist **post-merge** (siehe unten).

### 6. Der Concerts-Anteil von `All updates`

`All updates` fügt genau den Concerts-Delta hinzu, den das Popover ohnehin
berechnet — eine gesammelte Meldung je Lauf. Damit ist die dritte Stufe nicht
leer und braucht **keine** neuen Daten.

**Der Weg dorthin ist entschieden (Nachtrag 2, §8 des Mutterplans): die Zahl
kommt aus dem Kern, der Popover-Deckel wird nicht benutzt.**

- Zahl: `reprise_core::concerts::count_unseen()` — `pub` in
  `crates/reprise-core/src/concerts/query.rs:137`, re-exportiert aus
  `concerts.rs:38`.
- Beispielzeile für den Body: die erste Zeile aus
  `reprise_core::concerts::query_unseen()` (`query.rs:106`, ebenso `pub`).

`CONCERTS_DELTA_CAP = 3` (`ui/updates/feed_snapshot.rs:10`) ist `pub(super)`
und damit ohnehin unerreichbar — aber der eigentliche Grund, ihn nicht zu
benutzen, ist inhaltlich: er deckelt, wie viele Zeilen ein 470 px breites
Popover **zeigt**, nicht wie viel neu **ist**. Eine Meldung „3 new concerts"
bei zwölf neuen Terminen wäre falsch. Der Zähl-Chip im Abschnittskopf nennt
nach NR-23 ebenfalls die volle Stapelgröße; die Meldung stimmt damit mit dem
Chip überein.

Damit wird `feed_snapshot.rs` (Strang 2) **nicht** angefasst, und es entsteht
**keine** zweite Deckelkonstante. Verworfen: Strang 2 öffnet die Sichtbarkeit
(schafft eine zweite geteilte Datei); ebenso verworfen: eine eigene Konstante
desselben Werts im Kern (dieselbe Entscheidung an zwei Orten, und obendrein
die falsche Zahl).

**Ziel:** `All updates` meldet neu gefundene Konzerte von
Bibliotheks-Künstlern, `Releases only` nicht.
**Nachweis:** `os_7_all_updates_adds_the_concerts_delta` grün.

### 7. Die Einstellungszeile in den Preferences

Zweite `adw::ComboRow` **neben** dem bestehenden `scope_row()`
(`ui/preferences/preference_new_releases.rs:50`) in der Plugin-Zeile
**New Releases** unter `Preferences › Plugins › Online`.

**SET-10** (`docs/ux-rules.md:1155`) bleibt damit unangetastet: „Plugins is
the only settings surface for optional capabilities … There are no ‚Online
sources', ‚New Releases', or ‚Concerts' Preferences main pages." Eine eigene
Seite hätte eine Ausnahme von SET-10 gebraucht — die gibt es nicht.

Strings: Titel `Notify about updates`; Werte `Off` / `Releases only` /
`All updates`; Untertitel
`All updates also announces newly found concerts for your artists.`

**Ziel:** Die Einstellung sitzt dort, wo alle optionalen Fähigkeiten sitzen,
und nirgends sonst.
**Nachweis:** `os_7_all_updates_adds_the_concerts_delta` grün; SET-10
unverändert.

### 8. Strings, Modul-Verdrahtung, POTFILES

- Neue Datei `crates/reprise-gnome/src/ui/strings_notifications.rs` mit den
  Strings aus Aufgabe 4 und 7 (Volltabelle in §3 des Mutterplans):
  `{artist} · {type} · out today`, `{count} releases are out` /
  `{count} new concerts` (beide Plural), `Notify about updates`,
  `Off` / `Releases only` / `All updates`,
  `All updates also announces newly found concerts for your artists.`
- In `ui/strings.rs` **eine** Deklaration nach der Hausform (Vorbilder bei
  `:35-49`):
  `#[path = "strings_notifications.rs"] mod notifications; pub use notifications::*;`
- In `po/POTFILES.in` **eine** Zeile für die neue Datei, an der Stelle, an der
  die anderen `strings_*.rs` stehen (`:1`ff).
- **`po/`-Dateien werden nicht von Hand angefasst.** Deutsch entsteht über den
  normalen Extraktionslauf; nur `POTFILES.in` bekommt die eine Zeile.

**Ziel:** Jeder neue sichtbare Text ist extrahierbar.
**Nachweis:** `cargo test --workspace` grün; die neue Datei taucht im
Extraktionslauf auf.

### 9. Die UX-Regeln in Abschnitt H

Prozessvertrag: eine Regel wechselt `[planned]` → `[active]` **in demselben
Commit**, der das Verhalten baut und den regelbenannten Test hinzufügt. Ein
Test trägt **genau eine** primäre Regel-ID im Namen.
`scripts/check-ux-traceability.sh` ist Merge-Gate.

**Neu zu schreiben** (Volltext in §4.2 des Mutterplans — wörtlich übernehmen,
nicht neu formulieren):

| ID | Level | Test |
|---|---|---|
| `OS-6` | `[active] [core] [gtk]` | `os_6_the_first_fetch_announces_nothing` (`crates/reprise-core/src/artist_news_notify.rs`, `#[cfg(test)]`) |
| `OS-7` | `[active] [gtk]` | `os_7_all_updates_adds_the_concerts_delta` (`ui/preferences/preference_new_releases.rs`, `#[cfg(test)]`) |

**Keine** Statusmarker: dieser Strang ersetzt keine bestehende Regel.
OS-1…OS-5 (`:1321`ff) bleiben unverändert `[planned]` und werden **nicht**
angefasst. Nichts wird in Abschnitt R oder AE geschrieben.

**Ziel:** Zwei neue `[active]`-Regeln, jede mit ihrem Test, in einem
Abschnitt, den kein anderer Strang berührt.
**Nachweis:** die Traceability über den **eigenen** Abschnitt ist grün; der
vollständige Lauf ist post-merge.

---

## Was dieser Strang NICHT verifiziert

Die folgenden Prüfungen lesen Dateien, die dieser Strang **nicht besitzt**.
Sie können vor dem Merge prinzipiell nicht grün werden. **Nicht auf sie
warten und nicht versuchen, sie vorzuziehen** — ein Strang, der darauf wartet,
bleibt mit fertiger, korrekter Arbeit stehen. Sie stehen vollständig in
**§7, „Post-Merge-Querprüfungen", des Mutterplans**:

1. `scripts/check-ux-traceability.sh` über die **ganze** `docs/ux-rules.md` —
   dieser Strang sieht nur Abschnitt H, nie die Regeln der Stränge 1 und 2.
2. „Ein Wort, zwei Flächen" — betrifft diesen Strang nicht.
3. „Ein Zeitstempel, drei Fußzeilen" — betrifft diesen Strang nicht.
4. **„Dieselbe URL":** die Benachrichtigung (hier) öffnet für ein gegebenes
   Release exakt die URL, die dessen Popover-Zeile (**Strang 2**) öffnet. Das
   ist die zentrale Querprüfung dieses Strangs — und sie ist erst möglich,
   wenn Strang 2 gemergt ist. Vorher lässt sich nur die eigene Hälfte prüfen
   (Aufgabe 5).
5. Geometrie-Parität — betrifft diesen Strang nicht.
6. **Migrationskette am Stück:** eine v72-Datenbank durch beide Migrationen
   fahren, danach `PRAGMA user_version == 74` und beide Spalten prüfen.
   Beide Migrationen stammen aus **Strang 1**; der vollständige Beweis
   braucht aber die **Schreibseite** von `notified_released_at`, und die
   gehört diesem Strang. Deshalb bleibt die Prüfung post-merge: Schema von
   Strang 1, Schreibpfad von hier.

Was dieser Strang **sehr wohl** vor dem Merge liefert: `cargo fmt --check`,
`cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`, die Kernreinheit aus Aufgabe 2, jede
angefasste Datei unter 800 Zeilen, Screenshot 8 der Abnahme (§5.1 des
Mutterplans) und die Messung „Der erste Fetch meldet nichts" aus §5.2.

Das Display-Gate ist im Rudel bekanntermaßen flaky und auf `dev` teils schon
rot: **zuerst gegen `origin/dev` messen, was ohne diese Änderung rot ist**,
sonst wird fremdes Rot als eigene Schuld verbucht.
