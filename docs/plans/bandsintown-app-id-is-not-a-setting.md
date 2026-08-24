---
slug: bandsintown-app-id-is-not-a-setting
worktree: /home/marvin/Projects/reprise-bandsintown-app-id-is-not-a-setting
branch: feature/bandsintown-app-id-is-not-a-setting
phase: shipped
codex_session:
created: 2026-08-24
---

# Aufgabe: Bandsintown-`app_id` ist keine Einstellung mehr

Der Nutzer sieht in Einstellungen → Plugins → Concerts eine leere Zeile
„Bandsintown app_id" und muss dort etwas eintragen, damit Concerts überhaupt
Bandsintown befragt. Das ist eine Sackgasse: die öffentliche Bandsintown-REST-API
akzeptiert für Read-only-Abfragen **einen beliebigen eindeutigen String** als
`app_id` — es gibt nichts zu beschaffen und nichts geheim zu halten. Erst
kommerzielle Nutzung / höhere Rate-Limits verlangen eine offizielle Registrierung
über das Partnership-Programm.

Ergebnis dieser Aufgabe: die App bringt ihren eigenen Identifier mit, die Zeile
verschwindet, und mit ihr die letzte Credential-Eingabe der Concerts-Oberfläche.

## Entscheidungen (bindend, nicht neu verhandeln)

1. **Kein Secret, kein `option_env!`.** Der Wert ist kein Geheimnis: er steht als
   Query-Parameter im Klartext in jeder Anfrage (`bandsintown.rs:49,57`) und darf
   beliebig sein. Also eine gewöhnliche Konstante im Code — anders als
   `BUNDLED_TICKETMASTER_API_KEY` (`config.rs:19`), das echte Zugangsdaten trägt.
2. **Der Wert ist `io.github.marvinbaudach.Reprise`** — identisch mit `APP_ID`
   (`crates/reprise-gnome/src/main.rs:22`), aber als eigene Konstante in
   `reprise-core` dupliziert, weil Core nicht von der GNOME-Kiste abhängen darf.
   Er identifiziert das Projekt eindeutig, genau das will Bandsintown.
3. **Reihenfolge der Auflösung bleibt dieselbe Form wie bei Ticketmaster:**
   gespeicherter DB-Wert → `REPRISE_BANDSINTOWN_APP_ID` → eingebaute Konstante.
   Der DB-Schlüssel `concerts.bandsintown_app_id` bleibt als Altwert lesbar
   (jemand hat schon eine offizielle Partner-ID eingetragen); nur das Schreiben
   aus der Oberfläche entfällt. `Credentials::bandsintown_app_id` ist damit nie
   mehr `None`.
4. **Die Concerts-Oberfläche hat danach keine Credential-Zeile mehr.** Damit
   entfällt die gesamte Prüf- und Rückmeldemechanik dahinter, in Core wie in GTK.
   Sie hat keinen zweiten Aufrufer — das ist eine Löschung, kein Auskommentieren.

## Aufgaben

### 1 — Core: Standardwert einbacken

`crates/reprise-core/src/concerts/config.rs`

- Neue Konstante neben den bestehenden:
  `pub const DEFAULT_BANDSINTOWN_APP_ID: &str = "io.github.marvinbaudach.Reprise";`
  mit einem Einzeiler-Kommentar, warum sie kein Secret ist (Punkt 1 oben).
- In `credentials_with_env` hängt die Bandsintown-Kette ein letztes
  `.or_else(|| non_empty(DEFAULT_BANDSINTOWN_APP_ID))` an — dieselbe Form, die
  Ticketmaster für seinen Build-Wert benutzt.
- `Credentials::is_empty()` bleibt unverändert stehen; es ist ab jetzt faktisch
  immer `false`. Die `NoCredentials`-Leerseite (`concerts_empty_state.rs`) wird
  **nicht** angefasst — sie hängt an `has_credentials` und ist außerhalb dieser
  Aufgabe.

Test in `crates/reprise-core/src/concerts/domain_tests.rs` (neben den
bestehenden Credential-Tests, Name trägt die Regel-ID aus Aufgabe 5):
gespeicherter Wert schlägt Umgebung schlägt Konstante, und ohne beides kommt die
Konstante heraus — nicht `None`.

### 2 — Core: Credential-Prüfung entfernen

Sie hat nach Aufgabe 3 keinen Aufrufer mehr.

- `crates/reprise-core/src/concerts/credential.rs` löschen (inkl. des
  `conc_8_…`-Tests darin).
- `mod credential;` und das `pub use credential::{verify_credential,
  CredentialVerification};` in `crates/reprise-core/src/concerts.rs:30` entfernen.
- `crates/reprise-core/tests/concert_credentials.rs` löschen — die Datei enthält
  ausschließlich den CONC-8-Fixture-Test.
- Prüfen, ob dadurch etwas in `http.rs` oder `provider.rs` unbenutzt wird
  (z. B. `ticketmaster::credential_url`). Was nur die Prüfung brauchte, geht mit;
  was der Abrufpfad benutzt, bleibt. `ProviderError::MissingCredentials` bleibt —
  Ticketmaster kann weiterhin ohne Key dastehen.

### 3 — GTK: die Zeile und ihre Mechanik entfernen

`crates/reprise-gnome/src/ui/preferences/preference_concerts.rs`

Ersatzlos streichen: `credential_preference_specs`, `CredentialPreferenceSpec`,
`CredentialPreferenceRow`, `password_row`, `credential_apply_decision`,
`CredentialApplyDecision`, `credential_feedback_message`,
`apply_credential_feedback`, das `credentials`-Feld in
`ConcertPreferenceRowsInner` und dessen Aufbau in `build`. Der Rest der
Concerts-Vorlieben (Location, Fenstertage, Similar) bleibt unberührt, inklusive
der Reihenfolge der übrigen Zeilen.

`crates/reprise-gnome/src/ui/strings_concerts.rs`: `CONCERTS_BANDSINTOWN_APP_ID`
und die drei `CONCERTS_CREDENTIAL_*`-Strings entfernen — sie haben danach keinen
Aufrufer mehr. Vorher mit `grep` bestätigen, dass wirklich keiner übrig ist.

`crates/reprise-gnome/src/ui/preferences/preference_concerts_tests.rs`: die
Tests `set_4_credential_apply_requires_successful_persistence`,
`conc_8_credential_feedback_projects_every_verification_outcome_inline` und
`set_4_concert_credentials_expose_apply_and_inline_status` entfallen;
`conc_9_ticketmaster_build_credential_is_not_user_editable` und
`concerts_preferences_expose_only_bandsintown_and_link_similar_sensitivity`
werden auf die neue Wahrheit umgeschrieben (siehe Aufgabe 5).
`stored_credentials_are_preferred_and_similar_count_clamps` bleibt und deckt
zusätzlich ab, dass ohne gespeicherten Wert die Konstante herauskommt.

**Achtung SET-4:** Wenn nach dem Löschen kein Test mehr `set_4_` im Namen trägt,
schlägt `scripts/check-ux-traceability.sh` für SET-4 fehl. Erst prüfen
(`grep -rn "set_4_" crates/`), und falls das der letzte war, den verbleibenden
Concerts-Einstellungszeilen (z. B. `window_days`, `similar_enabled`) einen
`set_4_…`-Test geben, der die Sofortwirkung ohne Apply belegt — kein
Attrappen-Test, der nichts misst.

### 4 — Übersetzungen

`po/reprise.pot` und alle sieben `.po`-Dateien verlieren die vier entfernten
`msgid`s. Regenerieren, nicht von Hand herausdiffen, wo ein Ziel dafür existiert
(`meson compile -C <build> reprise-pot` bzw. `reprise-update-po`, siehe
`po/meson.build`); sonst alle Kataloge **konsistent** bearbeiten. Die
Gettext-Prüfung bricht beim ersten Katalog ab — „fehlt in ar" heißt in Wahrheit,
dass alle sieben nicht passen. Am Ende muss jeder Katalog dieselbe Msgid-Menge
haben.

### 5 — UX-Regeln nachziehen

`docs/ux-rules.md` — das Verhalten verschwindet, also darf keine aktive Regel es
weiter behaupten (`scripts/check-ux-traceability.sh` erzwingt beides:
`[active]` braucht einen Test, `[replaced …]` darf **keinen** haben).

- **CONC-8** wird `[replaced by CONC-9a]`. Kein Test darf danach `conc_8_` im
  Namen tragen.
- **CONC-9** wird `[replaced by CONC-9a]` — ihr Wortlaut („Bandsintown remains
  available as an optional credential row") ist genau das, was hier stirbt.
- **CONC-9a** neu, `[active] [core] [gtk]`, sinngemäß: Concerts fragt den Nutzer
  nie nach Zugangsdaten. Ticketmaster kommt aus dem Build-Wert, Bandsintown aus
  einem eingebauten Identifier; ein gespeicherter Altwert und die Umgebung gehen
  in dieser Reihenfolge vor. Kein Credential-Wert erscheint in Oberfläche, Logs
  oder Fehlermeldungen.
- Genau ein Core-Test und ein GTK-Test tragen `conc_9a_` im Namen: Core = die
  Auflösungsreihenfolge aus Aufgabe 1, GTK = die Concerts-Vorlieben enthalten
  keine `PasswordEntryRow` mehr.
- `docs/research/p5-surface-scopes.md:416-417` nennt CONC-8/CONC-9 in einer
  Tabelle. Nur nachziehen, falls die Prüfung das verlangt; es ist ein
  Forschungsdokument, kein Regelwerk.

## Verifikation (in dieser Reihenfolge, Ausgabe in eine Logdatei)

1. `cargo build --workspace` und `cargo clippy --workspace --all-targets` — die
   Löschungen dürfen keine `dead_code`- oder `unused_import`-Warnung
   hinterlassen; eine solche Warnung heißt, dass etwas nur halb entfernt wurde.
2. `cargo test -p reprise-core concerts` und `cargo test -p reprise-gnome
   preference_concerts` — grün.
3. `scripts/check-ux-traceability.sh` — grün. Diese Prüfung ist der eigentliche
   Nachweis, dass Regeln und Tests zueinander passen.
4. `grep -rn "bandsintown_app_id\|CONCERTS_BANDSINTOWN_APP_ID\|verify_credential" crates/`
   — es darf nur noch der DB-Schlüssel plus seine Leser in `config.rs` und den
   Tests übrig sein.
5. Nicht behaupten, dass die Zeile weg ist, ohne sie gesehen zu haben: die
   Aussage stützt sich auf den GTK-Test aus Aufgabe 5, nicht auf einen
   Screenshot.

## Parallelität

Nicht zerlegbar. Aufgaben 1–5 hängen an derselben Kette: die Löschung in GTK
(3) macht den Core-Code aus (2) erst unbenutzt, und die Regeländerung (5)
bestimmt die Testnamen in (1) und (3). Die Prüfung
`scripts/check-ux-traceability.sh` liest Regeln und Tests gemeinsam und kann
strukturell erst grün werden, wenn alles zusammen im Baum liegt. Ein Strang.
