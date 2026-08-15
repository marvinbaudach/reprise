---
slug: first-run-welcome-screen
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-15
strands: 1,2
merge_order: 1,2
---
# Ein Welcome-Screen für den ersten Start

> **Entwurf.** Inventarisiert gegen `origin/dev` = `334c9adb30`. Jede Belegzeile
> unten stammt aus `git show origin/dev:<pfad>`, nicht aus dem lokalen Checkout
> (der hängt ~30 Commits zurück).

## Kontext

**Heute** öffnet `crates/reprise-gnome/src/ui/first_run.rs` bei
`FirstRunDecision::ShowWizard` einen `adw::Dialog` mit `content_width(560)` /
`content_height(430)` (`first_run.rs:184-188`). Inhalt: ein Datenschutz-Label
(`first_run.rs:151-155`), optional eine Rhythmbox-`PreferencesGroup`
(`first_run.rs:60-72`, angeboten nur wenn `rhythmbox_offer` `Some` liefert,
`first_run.rs:48-50`), und zwei Buttons (`first_run.rs:156-162`).

`ShowWizard` heißt: `onboarding_completed == false` **und** `library_root` leer
(`first_run.rs:97-105`). `ExistingLibrary` markiert `onboarding_completed` still
(`first_run.rs:123-127`).

Der Ordner-Picker läuft heute **erst beim Schließen**: `should_open_folder`
(`first_run.rs:44-46`) entscheidet, ob `scan_button.emit_clicked()` gefeuert wird
(`first_run.rs:221-225`); der Klick öffnet dann `gtk4::FileDialog`
(`scan_flow.rs:71-87`) und startet danach `spawn_scan` (`scan_flow.rs:106-114`).
Der Rhythmbox-Import wird über `arm_rhythmbox_import_after_library_setup`
(`first_run.rs:74-95`) an `scan_controls.add_on_complete` gehängt und über
`take_completed_library_import` genau einmal genommen (`first_run.rs:52-58`).

Online-Quellen kommen im Assistenten **gar nicht** vor. Sie werden stattdessen
später vom Banner erfragt (`online_discovery_banner.rs:13-21`, sichtbar solange
`online_discovery_banner_completed == false` **und**
`online_sources::is_enabled() == false`).

**Danach** stellt der Assistent alle drei Fragen in einem Moment: Musikordner,
Rhythmbox-Import, Online-Quellen. Beide Abschlusswege setzen
`online_discovery_banner_completed`, damit das Banner dieselbe Frage nicht ein
zweites Mal stellt. Bestandsinstallationen (`ExistingLibrary`) sehen den
Assistenten nie und behalten das Banner unverändert.

### Die `Files:`-Listen sind Startpunkt, kein Zaun

Jede `Files:`-Liste unten nennt die Dateien, in denen die Arbeit **beginnt**.
Angrenzende Dateien dürfen minimal geändert werden — nenne sie dann in der
Commit-Nachricht. Halte nur an, wenn der **Vertrag** falsch ist (eine Signatur
existiert nicht, eine Bedingung stimmt nicht), nicht weil eine Datei fehlt.

## Belegte Signaturen (Stand `origin/dev`)

| Was | Wo | Signatur / Wert |
|---|---|---|
| Gate lesen | `crates/reprise-core/src/online_sources.rs:40` | `pub fn is_enabled(db: &Db) -> Result<bool, rusqlite::Error>` |
| Gate schreiben | `online_sources.rs:45` | `pub fn set_enabled(db: &Db, value: bool) -> Result<(), rusqlite::Error>` |
| First-Enable-Defaults | `online_sources.rs:88-98` | `fn first_enable_source_defaults() -> [(&'static ModuleDescriptor, bool); 7]` — **privat** |
| Radio-Default | `online_sources.rs:94` | `(&modules::RADIO_MODULE, true)`; alle anderen sechs `false` |
| Modul schreiben | `crates/reprise-core/src/modules.rs:214-221` | `pub fn set_enabled(db: &Db, module: &ModuleDescriptor, value: bool) -> Result<(), rusqlite::Error>` |
| Modul-IDs | `modules.rs:98, 111, 119` | `"podcasts"`, `"youtube"`, `"radio"` — Konstanten `PODCASTS_MODULE`, `YOUTUBE_MODULE`, `RADIO_MODULE` |
| Onboarding-Flag | `crates/reprise-core/src/library/settings_api.rs:74` | `pub fn set_onboarding_completed(db: &Db, completed: bool) -> Result<(), rusqlite::Error>` |
| Banner-Flag | `settings_api.rs:84-90` | `pub fn set_online_discovery_banner_completed(db: &Db, completed: bool) -> Result<(), rusqlite::Error>` |
| Library-Root lesen | `settings_api.rs:49` | `pub fn get_library_root(db: &Db) -> Result<Option<String>, rusqlite::Error>` |
| Rhythmbox gefunden | `crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs:87` | `pub(in crate::ui) fn rhythmbox_import_available() -> bool` |
| Rhythmbox-Angebot | `first_run.rs:48` | `fn rhythmbox_offer(decision: FirstRunDecision, available: bool) -> Option<bool>` — lokal in `first_run.rs`, nicht in `preference_rhythmbox` |
| Banner bauen | `online_discovery_banner.rs:53-56` | `pub(in crate::ui) fn build(db: &Rc<Db>, on_review: impl Fn() + 'static) -> Option<OnlineDiscoveryBanner>` |
| Scan starten | `crates/reprise-gnome/src/ui/scan/scan_worker.rs:58` | `pub(in crate::ui) fn spawn_scan(folder: PathBuf, db_path: PathBuf, …)` — bereits aus `crate::ui` aufrufbar |
| Aufrufstelle des Assistenten | `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs:693-700` | `super::first_run::run(window, scan_button, scan_controls, conn, first_run_decision, &present_rhythmbox_import)` |

**Achtung zur Modulidentität:** `ModuleDescriptor` sind `pub const`, keine
`static`. Referenzen auf Konstanten dürfen dupliziert werden — vergleiche
Module deshalb immer über `module.id`, **nie** über Zeiger-Identität.

### Bestehende Strings (wiederverwenden, nicht neu anlegen)

| Konstante | Datei:Zeile | Text |
|---|---|---|
| `ONBOARDING_WELCOME` | `ui/strings.rs:125` | `"Welcome to Reprise"` |
| `ONBOARDING_PRIVACY` | `strings.rs:126` | `"Reprise keeps your library local. …"` |
| `ONBOARDING_IMPORT_FROM_RHYTHMBOX` | `strings.rs:127` | `"Import from Rhythmbox"` |
| `ONBOARDING_IMPORT_FROM_RHYTHMBOX_DESCRIPTION` | `strings.rs:128-129` | `"Rhythmbox was found. Choose what Reprise should import."` |
| `ONBOARDING_SKIP` | `strings.rs:140` | `"Skip for Now"` |
| `ONBOARDING_SET_UP` | `strings.rs:141` | `"Set Up Library"` |
| `LIBRARY_FOLDER` | `strings.rs:196` | `"Library Folder"` |
| `NO_LIBRARY_FOLDER` | `strings.rs:197` | `"No folder selected"` |
| `CHOOSE_FOLDER` | `strings.rs:198` | `"Choose Folder…"` |
| `PREFERENCES_ONLINE_SOURCES` | `ui/strings_online_sources.rs:27` | `"Online sources"` |
| `ONLINE_SOURCES_USE_RADIO` | `strings_online_sources.rs:47` | `"Use Radio"` |
| `ONLINE_SOURCES_USE_PODCASTS` | `strings_online_sources.rs:46` | `"Use Podcasts"` |
| `ONLINE_SOURCES_USE_YOUTUBE` | `strings_online_sources.rs:45` | `"Use YouTube"` |
| `ONLINE_SOURCES_RADIO_SUBTITLE` | `strings_online_sources.rs:39-40` | `"Stations and live streams · radio-browser.info directory"` |
| `ONLINE_SOURCES_PODCASTS_SUBTITLE` | `strings_online_sources.rs:37-38` | `"Shows as audio episodes · RSS feeds, search via Apple Podcasts"` |
| `ONLINE_SOURCES_YOUTUBE_SUBTITLE` | `strings_online_sources.rs:35-36` | `"Channels as audio episodes · channel feeds, audio via yt-dlp"` |

Alle drei `ONLINE_SOURCES_USE_*` sind heute **ungenutzt** (deshalb steht
`#![allow(dead_code)]` in `strings_online_sources.rs:1`); der Assistent ist ihr
erster Konsument.

Das Formatier-Muster für Platzhalter steht in `strings_online_sources.rs:20-25`:
eine Konstante mit `{name}` plus eine Funktion, die `formatted(KONST, &[("name",
wert)])` ruft. `formatted` ist `pub(super)` (`strings.rs:19`), der Ersetzer ist
`i18n.rs:113-119` (schlichtes `{name}` → Wert).

---

## Tasks

### Task 1 — Neue Strings anlegen

**Ziel:** Alle Texte, die der neue Aufbau braucht, existieren als `N_!`-Konstanten,
bevor die erste Widget-Zeile geschrieben wird.

**Files:** `crates/reprise-gnome/src/ui/strings.rs`,
`crates/reprise-gnome/src/ui/strings_online_sources.rs`

**Schritte:**

1. In `strings.rs` neben den bestehenden `ONBOARDING_*`-Konstanten (also
   zwischen Zeile 125 und 141):
   ```rust
   pub const ONBOARDING_GROUP_LIBRARY_FOLDER: &str = N_!("Library folder");
   pub const ONBOARDING_GROUP_IMPORT: &str = N_!("Import");
   pub const ONBOARDING_NOTHING_FOUND_IN: &str = N_!("Nothing found in {folder}");

   pub fn onboarding_nothing_found_in(folder: &str) -> String {
       formatted(ONBOARDING_NOTHING_FOUND_IN, &[("folder", folder)])
   }
   ```
   `formatted` steht bereits in derselben Datei (`strings.rs:19`) — kein Import
   nötig. Das Muster spiegelt `online_content_show_sources`
   (`strings_online_sources.rs:20-25`).
2. In `strings_online_sources.rs`:
   ```rust
   pub const ONBOARDING_ONLINE_SOURCES_BODY: &str = N_!(
       "Three sources may reach the network. Off makes this a local player: no requests, no downloads, and their sidebar entries stay hidden."
   );
   pub const ONBOARDING_ONLINE_SOURCES_FOOTER: &str =
       N_!("You can change this any time in Preferences · Plugins.");
   ```
   Die Datei trägt bereits `#![allow(dead_code)]` (Zeile 1) und ist über
   `strings.rs:120-123` re-exportiert — es genügt `strings::ONBOARDING_…`.

**Akzeptanzkriterium:**
```
cargo build -p reprise-gnome
```
kompiliert. Keine `#[allow(dead_code)]`-Attribute hinzufügen (die Allowlist in
`scripts/check-frontend-thinness.sh:199-246` ist byte-genau gepinnt).

---

### Task 2 — Die First-Enable-Defaults auslesbar machen (Core)

**Ziel:** Die Radio-Vorauswahl kommt aus derselben Tabelle, die der erste
Gate-Enable schreibt — nicht aus einer zweiten Wahrheit in der UI.

**Files:** `crates/reprise-core/src/online_sources.rs`

**Problem:** `first_enable_source_defaults()` ist **privat**
(`online_sources.rs:88`). Ohne Änderung kann die UI die Vorauswahl nicht lesen
und müsste `true` für Radio hardcoden — genau das verbietet die Spezifikation.

**Minimale Sichtbarkeitsänderung** (kleiner als die Funktion selbst öffentlich
zu machen, weil sie das Array nicht nach außen gibt):

```rust
/// The state a first enable would write for one source, so a surface can
/// *display* the rule instead of restating it. Unknown modules answer `false`:
/// a source nobody seeded is not one the app turns on by itself.
///
/// Compared by `id`: `ModuleDescriptor`s are `const`, so two references to the
/// same module need not be the same pointer.
pub fn first_enable_default_for(module: &ModuleDescriptor) -> bool {
    first_enable_source_defaults()
        .into_iter()
        .find(|(candidate, _)| candidate.id == module.id)
        .map(|(_, enabled)| enabled)
        .unwrap_or(false)
}
```

`first_enable_source_defaults()` bleibt privat.

**Unit-Tests** (im bestehenden `mod tests` derselben Datei, ab
`online_sources.rs:141`):

```rust
#[test]
fn first_enable_defaults_are_readable_without_restating_them() {
    assert!(first_enable_default_for(&modules::RADIO_MODULE));
    assert!(!first_enable_default_for(&modules::PODCASTS_MODULE));
    assert!(!first_enable_default_for(&modules::YOUTUBE_MODULE));
    // A module outside the table answers off, not "unknown".
    assert!(!first_enable_default_for(&modules::SONG_VISUALS_MODULE));
}
```

**Mutations-Probe (verpflichtend, sonst misst der Test nichts):** Ändere
probeweise `online_sources.rs:94` von `true` auf `false` und prüfe, dass der
neue Test **und** `first_enable_turns_every_online_source_off_except_radio`
(`online_sources.rs:214`) rot werden. Danach zurückändern.

**Akzeptanzkriterium:**
```
cargo test -p reprise-core online_sources
```
grün, und die Mutations-Probe war rot.

---

### Task 3 — Auswahl → Modulzustände als reine Funktionen (Core)

**Ziel:** Die Abbildung „drei Schalter → was in die Datenbank geht" ist eine
reine Funktion mit Tests, bevor irgendein Widget sie benutzt.

**Files:** `crates/reprise-core/src/online_sources.rs`

**Warum Core und nicht `ui/first_run.rs`:** Die Abbildung kennt
`ModuleDescriptor`s und die First-Enable-Regel — beides Core-Wissen. Zusätzlich
zählt `scripts/check-frontend-thinness.sh:131` **jede** Nennung von `Connection`
oder `rusqlite::` in `crates/reprise-gnome/src` gegen ein hart gleichgesetztes
Budget; eine Core-Funktion hält Rückgabetypen wie `Result<…, rusqlite::Error>`
aus dem Frontend heraus. (Reine Funktionen *dürften* formal in `first_run.rs`
stehen — das Skript verbietet nur DB-Muster, nicht Logik — aber sie kämen dort
nicht ohne Modul- und Fehlertypen aus.)

**Schritte:**

1. Auswahlstruktur und Schreibliste:
   ```rust
   /// The three sources the first-run wizard offers, in display order.
   pub const WIZARD_SOURCE_MODULES: [&ModuleDescriptor; 3] = [
       &modules::RADIO_MODULE,
       &modules::PODCASTS_MODULE,
       &modules::YOUTUBE_MODULE,
   ];

   /// What the wizard's three switches say. Not a settings snapshot — the
   /// user's answer, before anything is written.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub struct WizardSourceSelection {
       pub radio: bool,
       pub podcasts: bool,
       pub youtube: bool,
   }

   impl WizardSourceSelection {
       /// The state the wizard opens in: exactly what a first enable would
       /// write, read from the table rather than repeated here.
       pub fn from_first_enable_defaults() -> Self {
           Self {
               radio: first_enable_default_for(&modules::RADIO_MODULE),
               podcasts: first_enable_default_for(&modules::PODCASTS_MODULE),
               youtube: first_enable_default_for(&modules::YOUTUBE_MODULE),
           }
       }

       /// `NET-1a`: no source chosen means the gate stays shut. Turning the
       /// gate on "just in case" would make the app network-capable without
       /// anyone asking for it.
       pub fn opens_the_gate(self) -> bool {
           self.radio || self.podcasts || self.youtube
       }

       /// The module writes this selection implies, in write order.
       pub fn module_writes(self) -> [(&'static ModuleDescriptor, bool); 3] {
           [
               (&modules::RADIO_MODULE, self.radio),
               (&modules::PODCASTS_MODULE, self.podcasts),
               (&modules::YOUTUBE_MODULE, self.youtube),
           ]
       }
   }
   ```
2. Die Schreibfunktion, die die Reihenfolge festhält (siehe Abschnitt
   „Schreibreihenfolge"):
   ```rust
   /// Applies the wizard's answer. The gate goes first so its one-shot seeding
   /// runs before the explicit choices land on top of it; no source chosen
   /// leaves the gate — and every module — untouched.
   pub fn apply_wizard_selection(
       db: &Db,
       selection: WizardSourceSelection,
   ) -> Result<(), rusqlite::Error> {
       if !selection.opens_the_gate() {
           return Ok(());
       }
       set_enabled(db, true)?;
       for (module, enabled) in selection.module_writes() {
           modules::set_enabled(db, module, enabled)?;
       }
       Ok(())
   }
   ```

**Unit-Tests** (gleicher `mod tests`):

```rust
#[test]
fn the_wizard_opens_with_the_first_enable_defaults() {
    let selection = WizardSourceSelection::from_first_enable_defaults();
    assert!(selection.radio);
    assert!(!selection.podcasts);
    assert!(!selection.youtube);
}

#[test]
fn no_source_chosen_leaves_the_gate_shut() {
    let db = migrated_db();
    apply_wizard_selection(&db, WizardSourceSelection::default()).unwrap();
    assert!(!is_enabled(&db).unwrap());
    assert!(settings::get_setting_in(db.conn(), &modules::enabled_key(&modules::RADIO_MODULE))
        .unwrap()
        .is_none());
}

#[test]
fn the_wizard_selection_survives_the_first_enable_seeding() {
    // Radio off, Podcasts on — the inverse of the seed, so a seed that won
    // would be visible.
    let db = migrated_db();
    apply_wizard_selection(
        &db,
        WizardSourceSelection { radio: false, podcasts: true, youtube: false },
    )
    .unwrap();

    assert!(is_enabled(&db).unwrap());
    assert!(!modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
    assert!(modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
    assert!(!modules::is_enabled(&db, &modules::YOUTUBE_MODULE).unwrap());
    // The four sources the wizard never mentions keep their first-enable
    // defaults — the wizard adds a question, it does not answer theirs.
    for module in [
        &modules::NEW_RELEASES_MODULE,
        &modules::CONCERTS_MODULE,
        &modules::ARTWORK_MODULE,
        &modules::ONLINE_LYRICS_MODULE,
    ] {
        assert!(!modules::is_enabled(&db, module).unwrap(), "{}", module.id);
    }
}
```

`migrated_db()` existiert bereits (`online_sources.rs:145-147`), ebenso das
Muster `db.conn()` **innerhalb von Core** (dort erlaubt; im Frontend ist
`.conn(` ein harter Bann, `check-frontend-thinness.sh:132`).

**Akzeptanzkriterium:**
```
cargo test -p reprise-core online_sources
```
grün. Kein `cargo test --exact` verwenden — ein `--exact` mit einem Namen, den
es nicht gibt, beendet sich mit 0, nachdem es nichts gelaufen ist.

---

### Task 4 — Abschluss-Schreibpfad in `first_run.rs`, testbar herausgezogen

**Ziel:** Beide Abschlusswege schreiben ihre Flags über eine Funktion, die ohne
GTK aufrufbar und damit ohne Display testbar ist.

**Files:** `crates/reprise-gnome/src/ui/first_run.rs`

**Schritte:**

1. `CompletionOptions` (heute `first_run.rs:24-27`) erweitern:
   ```rust
   #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
   struct CompletionOptions {
       rhythmbox_import: bool,
       sources: WizardSourceSelection,
   }
   ```
   `Skip` benutzt weiter `CompletionOptions::default()` — also keine Quelle
   (`first_run.rs:233-239`).
2. Den Schreibteil aus der `complete`-Closure (`first_run.rs:199-228`) in eine
   freie Funktion heben:
   ```rust
   /// Everything the wizard persists, on both exits. `NET-4`: the wizard
   /// *replaces* the discovery banner's question for a fresh install, so it
   /// closes the banner too — otherwise the same question arrives twice.
   fn persist_completion(db: &Db, options: CompletionOptions) {
       if let Err(error) = settings::set_onboarding_completed(db, true) {
           tracing::warn!(%error, "could not persist onboarding completion");
       }
       if let Err(error) =
           online_sources::apply_wizard_selection(db, options.sources)
       {
           tracing::warn!(%error, "could not persist first-run source selection");
       }
       if let Err(error) =
           settings::set_online_discovery_banner_completed(db, true)
       {
           tracing::warn!(%error, "could not close the discovery banner");
       }
   }
   ```
   Fehler werden geloggt, nicht zurückgegeben: die Funktion darf keinen
   `rusqlite`-Typ in ihrer Signatur nennen, sonst hebt sie das
   `rusqlite`-Budget (`check-frontend-thinness.sh:131`) ohne Not.
3. Die Closure ruft nur noch `persist_completion(&conn, options)` und behält
   danach ihr bestehendes Verhalten (Dialog schließen, Rhythmbox armieren,
   Scan/Picker auslösen, `log_smoke_result`).
4. `initial_decision` (`first_run.rs:107-129`) **nicht** anfassen:
   `ExistingLibrary` setzt weiterhin nur `onboarding_completed`
   (`first_run.rs:124`) und schreibt das Banner-Flag ausdrücklich nicht.

**Tests** — ab hier in eine eigene Datei, nach dem Repo-Muster
(`preference_rhythmbox.rs:694-696`):

```rust
#[cfg(test)]
#[path = "first_run_tests.rs"]
mod tests;
```

Das ist nicht nur Kosmetik: `check-frontend-thinness.sh:84` schließt
`*_tests.rs` von allen Budgets aus, und `check-architecture.sh:20` deckelt jede
`.rs`-Datei bei 800 Zeilen — `first_run.rs` steht heute bei 373.

Die acht bestehenden Tests (`first_run.rs:296-372`) wandern unverändert mit.
Neu, alle **ohne** Display:

```rust
#[test]
fn both_exits_close_onboarding_and_the_discovery_banner() {
    for options in [
        CompletionOptions::default(),
        CompletionOptions {
            rhythmbox_import: true,
            sources: WizardSourceSelection::from_first_enable_defaults(),
        },
    ] {
        let db = Db::open_in_memory().unwrap();
        persist_completion(&db, options);
        assert!(settings::get_onboarding_completed(&db).unwrap());
        assert!(settings::get_online_discovery_banner_completed(&db).unwrap());
    }
}

#[test]
fn skipping_the_wizard_leaves_the_network_gate_shut() {
    let db = Db::open_in_memory().unwrap();
    persist_completion(&db, CompletionOptions::default());
    assert!(!reprise_core::online_sources::is_enabled(&db).unwrap());
}

#[test]
fn a_completed_wizard_leaves_no_banner_to_show() {
    // `build` returns before it touches a widget when the banner is done
    // (online_discovery_banner.rs:57-64), so this needs no display.
    let db = Rc::new(Db::open_in_memory().unwrap());
    persist_completion(&db, CompletionOptions::default());
    assert!(crate::ui::online_discovery_banner::build(&db, || {}).is_none());
}
```

**Zur Gegenprobe „nach `ExistingLibrary` liefert `build` weiter `Some`":** Die
`Some`-Seite baut `adw::Banner` (`online_discovery_banner.rs:69`) und braucht
deshalb ein Display — sie gehört als `#[ignore]`-Test in Task 6, nicht hierher.

**Akzeptanzkriterium:**
```
cargo test -p reprise-gnome --bin reprise first_run
```
grün. `reprise-gnome` hat kein `lib`-Target, nur `[[bin]] reprise`
(`crates/reprise-gnome/Cargo.toml:10-12`) — `--lib` gibt es dort nicht. Die
Bilanzzeile von `cargo test` ist als Beleg untauglich; prüfe stattdessen, dass
die neuen Testnamen in der Ausgabe stehen.

---

### Task 5 — Der Dialogaufbau

**Ziel:** Der Dialog zeigt Datenschutz, Ordner, Import und Online-Quellen in
dieser Reihenfolge; der Ordner-Picker öffnet **im** Dialog.

**Files:** `crates/reprise-gnome/src/ui/first_run.rs`,
`crates/reprise-gnome/src/ui/first_run_sources.rs` (neu),
`crates/reprise-gnome/src/ui/mod.rs`,
`crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`

**Schritte:**

1. **Online-Quellen-Gruppe** in die neue Datei `first_run_sources.rs`
   (hält `first_run.rs` unter 800 Zeilen):
   ```rust
   pub(super) struct SourceWidgets {
       pub(super) group: adw::PreferencesGroup,
       pub(super) footer: gtk4::Label,
       radio: adw::SwitchRow,
       podcasts: adw::SwitchRow,
       youtube: adw::SwitchRow,
   }

   impl SourceWidgets {
       pub(super) fn selection(&self) -> WizardSourceSelection { … }
   }

   pub(super) fn build_source_group(
       selection: WizardSourceSelection,
   ) -> SourceWidgets
   ```
   Die Gruppe: `adw::PreferencesGroup::builder().title(strings::text(
   strings::PREFERENCES_ONLINE_SOURCES)).description(strings::text(
   strings::ONBOARDING_ONLINE_SOURCES_BODY)).build()` — Muster wie
   `preference_plugins.rs:292-297`.
   Jede Zeile ist eine `adw::SwitchRow::builder().title(…).subtitle(…)
   .use_markup(false).active(…).build()`, exakt wie
   `build_rhythmbox_import_group` (`first_run.rs:60-72`) — `use_markup(false)`
   ist Pflicht, „kein Markup" steht so in der Spezifikation.
   Die Fußnote ist ein eigenes `gtk4::Label` (`adw::PreferencesGroup` hat keinen
   Footer-Slot) mit `wrap(true)`, `xalign(0.0)` und den CSS-Klassen
   `dim-label` + `caption` — Muster `browse_chooser.rs:89-92`. Beides erfüllt
   `CONTRAST-5`: kein roher Accent als Textfarbe, sondern Adwaita-Klassen.
2. **Ordnergruppe** in `first_run.rs`, nur wenn `library_root` leer ist — bei
   `ShowWizard` ist das per Definition der Fall (`first_run.rs:97-105`), die
   Gruppe wird also immer gebaut; die Bedingung bleibt trotzdem explizit, damit
   der Test sie messen kann.
   `adw::ActionRow` mit `title = NO_LIBRARY_FOLDER`, `subtitle =
   onboarding_nothing_found_in(<Anzeigename des XDG-Musikordners>)`, dazu ein
   `gtk4::Button::with_label(&strings::text(strings::CHOOSE_FOLDER))` als
   `add_suffix`, `valign(gtk4::Align::Center)`.
   Den XDG-Musikordner liefert `glib::user_special_dir(
   glib::UserDirectory::Music)` — im Repo bisher nur in
   `crates/reprise-platform-linux/src/diagnostics.rs:82` benutzt; im Frontend
   ist es neu. Für die Anzeige `~/Music` das Home-Präfix durch `~` ersetzen;
   einen Helfer dafür gibt es im Repo **nicht**, er wird hier angelegt (reine
   Funktion, Unit-Test in `first_run_tests.rs`). Liefert `user_special_dir`
   `None`, entfällt der Untertitel — nie einen Pfad raten.
3. **Picker im Dialog.** Muster wörtlich von `scan_flow.rs:71-105` übernehmen:
   `gtk4::FileDialog::builder().title(…).modal(true).build()`, dann
   `glib::spawn_future_local(async move { … dialog.select_folder_future(
   Some(&window)).await … })`, `DialogError::Dismissed` / `Cancelled` nur
   `debug!`, alles andere `error!`.
   Der gewählte Pfad wird **nur gemerkt** (`Rc<RefCell<Option<PathBuf>>>`) und
   in die Zeile geschrieben (Titel = Ordnername, Untertitel = Pfad,
   Button-Label → `"Change…"`). **Kein** `settings::set_library_root` hier: im
   Frontend schreibt den Root sonst nur `main.rs:232` (CLI-Argument), im
   Normalfall schreibt ihn der Scan-Pfad in Core. Zwei Schreiber wären zwei
   Wahrheiten.
4. **Abschluss.** `should_open_folder` (`first_run.rs:44-46`) bekommt das
   Vorwissen dazu:
   ```rust
   fn should_open_folder(response: CompletionResponse, folder_chosen: bool) -> bool {
       response == CompletionResponse::SetUp && !folder_chosen
   }
   ```
   Wurde im Dialog schon gewählt, feuert **nicht** `scan_button.emit_clicked()`
   (das öffnete den Picker ein zweites Mal), sondern ein neuer Callback
   `start_scan_of: Rc<dyn Fn(PathBuf)>`. Er wird in
   `window_runtime_wiring.rs` neben `present_rhythmbox_import`
   (`window_runtime_wiring.rs:685-692`) gebaut und ruft
   `scan_worker::spawn_scan` (`scan_worker.rs:58`, bereits `pub(in crate::ui)`).
   `arm_rhythmbox_import_after_library_setup` (`first_run.rs:74-95`) bleibt
   unverändert: der Import hängt weiter an `scan_controls.add_on_complete`, also
   an einer fertigen Bibliothek — in beiden Ordnerwegen.
5. **Dialoghülle.** `content_height(430)` → `content_height(620)`
   (`first_run.rs:187`), und der `content`-Box (`first_run.rs:163-172`) kommt in
   ein `gtk4::ScrolledWindow` mit `propagate_natural_height(true)` und
   `hscrollbar_policy(gtk4::PolicyType::Never)`, das per
   `toolbar.set_content(...)` gesetzt wird. Fehlt ein Block, schrumpft der
   Dialog dadurch von selbst.
6. **Accessibility.** Jede neue Zeile braucht ihre Semantik, und
   `gtk4::AccessibleRole` lässt sich **nur im Konstruktor** setzen — nachträglich
   nicht (`.builder().accessible_role(…)`, Muster
   `preference_plugins.rs:266`). Die Fußnote und der Datenschutzabsatz sind
   `AccessibleRole::Presentation` bzw. bleiben Labels; die `SwitchRow`s und die
   `ActionRow` tragen ihre Adwaita-Rollen und brauchen sprechende Titel — keine
   nackten Icon-Buttons.
7. Buttons bleiben wie heute: `ONBOARDING_SKIP` schlicht, `ONBOARDING_SET_UP`
   mit `add_css_class("suggested-action")` (`first_run.rs:158`), rechtsbündig
   über `buttons.set_halign(gtk4::Align::End)` (`first_run.rs:160`).
8. `ui/mod.rs` um das neue Modul ergänzen (neben `pub mod first_run;`,
   `ui/mod.rs:52`).

**Akzeptanzkriterium:**
```
cargo build -p reprise-gnome
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test -p reprise-gnome --bin reprise first_run
```
alle drei sauber.

---

### Task 6 — Regel `NET-4a` und der Display-Test

**Ziel:** Der Assistent ist als UX-Regel dokumentiert und hat einen Display-Test,
der von den Gates auch wirklich gefahren wird.

**Files:** `docs/ux-rules.md`,
`crates/reprise-gnome/src/ui/first_run_tests.rs`

**Warum eine neue Regel — die Gates lassen keine Wahl:**
`scripts/check-display-tests.sh --rule-named` filtert Testnamen mit
`^(${prefixes})_[0-9]+[a-z]?_` (`check-display-tests.sh:49`); die Präfixe kommen
aus `docs/ux-rules.md` selbst (`check-display-tests.sh:40-43`). Ein Test ohne
Regel-ID im Namen läuft nur im Default-Modus mit. Umgekehrt verlangt
`check-ux-traceability.sh:81-90`, dass **jede** `[active]`-Regel mindestens einen
regel-benannten Test hat — eine neue Regel ohne Test macht das Gate rot. Beide
Richtungen zusammen ergeben genau eine widerspruchsfreie Wahl: **neue
`[active]`-Regel plus regel-benannter Test.**

**Schritte:**

1. In `docs/ux-rules.md` direkt hinter `NET-4` (heute Zeile 2687-2694) einfügen:
   ```
   - **NET-4a** [active] [gtk] — On a fresh install the first-run wizard asks
     the online-sources question once, in the same dialog as the music folder
     and the Rhythmbox import: Radio preselected, Podcasts and YouTube off,
     with the sources' own subtitles and a footnote pointing at Preferences ·
     Plugins. Both exits — "Skip for Now" and "Set Up Library" — close the
     discovery banner of `NET-4`, so the question is never asked twice. No
     source chosen leaves the gate shut and writes no module. An existing
     library never sees the wizard and keeps the banner.
   ```
   Format wörtlich nach dem Muster von `NET-4` — der Parser liest
   `^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(active|planned|replaced)` und danach
   `\[(core|gtk|e2e|manual)\]` (`check-ux-traceability.sh:24-32`). `[gtk]` ist
   richtig: die Abdeckung liegt in `crates/reprise-gnome`.
2. `NET-4` selbst um einen Satz ergänzen, damit die beiden Regeln sich nicht
   widersprechen: das Banner ist der Weg für **Bestandsinstallationen**; auf
   einer Neuinstallation stellt `NET-4a` die Frage und das Banner erscheint nie.
3. Display-Test in `first_run_tests.rs`:
   ```rust
   #[test]
   #[ignore = "requires a display; run via xvfb-run"]
   fn net_4a_the_wizard_asks_folder_import_and_sources_in_one_dialog() { … }
   ```
   Der `#[ignore]`-Text muss **exakt** so lauten — nur diese Zeichenkette zählt
   in `check-ux-traceability.sh:99-101` als Abdeckung einer `[active]`-Regel;
   jede andere `#[ignore]`-Begründung macht das Gate rot
   (`check-ux-traceability.sh:103-104`). Der Name muss mit `net_4a_` beginnen,
   sonst greift `--rule-named` nicht.
   Prüfe darin, gegen den Widget-Baum und nicht gegen einen Screenshot:
   - die Gruppen in Reihenfolge (Ordner, Import, Online-Quellen),
   - drei `SwitchRow`s mit `ONLINE_SOURCES_USE_RADIO/PODCASTS/YOUTUBE` als
     Titel und den `*_SUBTITLE`-Strings als Untertitel,
   - Radio `is_active()`, Podcasts und YouTube nicht,
   - der Rhythmbox-Switch inaktiv,
   - die Ordnerzeile mit Button-Label `CHOOSE_FOLDER`,
   - Fußnote trägt `ONBOARDING_ONLINE_SOURCES_FOOTER`.
   Beginne mit `let _main_context = crate::ui::test_main_context::lock_main_context();`
   und `if gtk4::init().is_err() { return; }` — Muster
   `online_discovery_banner.rs:143-146`.
4. Zwei weitere `#[ignore]`-Tests, ohne Regel-ID im Namen (sie messen
   Strukturvarianten, keine eigene Regel):
   - Ordnerblock fehlt, wenn `library_root` gesetzt ist,
   - Rhythmbox-Block fehlt ohne Fund (`rhythmbox_offer(…, false) == None`,
     `first_run.rs:48-50`),
   - `build(&db, || {})` nach `ExistingLibrary` liefert weiter `Some` —
     der Widget-Zweig aus `online_discovery_banner.rs:69`.

**Akzeptanzkriterium:**
```
scripts/check-ux-traceability.sh
xvfb-run -a cargo test -p reprise-gnome --bin reprise \
  net_4a_the_wizard_asks_folder_import_and_sources_in_one_dialog -- --ignored
```
Erstes Kommando grün; zweites meldet `1 passed`. Verlasse dich nicht auf die
Bilanzzeile allein — `--exact` mit einem veralteten Namen beendet sich mit 0,
nachdem es nichts gelaufen ist (deshalb prüft `check-display-tests.sh:191-196`
zusätzlich auf `test result: ok. 1 passed;`).

---

### Task 7 — Thinness-Budgets nachziehen

**Ziel:** `scripts/check-frontend-thinness.sh` misst wieder exakt den Ist-Stand.
Diese Task ist **kein** Aufräumen am Ende, sondern der Grund, warum am
14./15.08.2026 drei PRs nacheinander mit rotem Quality-Gate auf `dev` landeten,
obwohl jeder lokal grün gemessen war — alle drei Fehlschläge lebten
ausschließlich in diesem Skript.

**Files:** `scripts/check-frontend-thinness.sh`

**Warum es zwingend ist:** Jedes Budget ist **harte Gleichheit**, nicht
Obergrenze — zu niedrig ist genauso rot wie zu hoch
(`check-frontend-thinness.sh:104-114`, mit der ausdrücklichen Meldung „lower the
budget … to $actual"). Bei #498 hob ein neues UI-Modul `rusqlite` von 109 auf
113.

**Kandidaten, die dieser Umbau bewegen kann:**

| Budget | Zeile | Stand | Wodurch er sich bewegt |
|---|---|---|---|
| `rusqlite` | `:52` | 113 | Pattern `rusqlite::\|use rusqlite\|params!\|\.prepare\(\|\.query_row\(\|Connection` (`:131`). Der Plan vermeidet solche Nennungen bewusst (Task 4, Punkt 2) — trotzdem messen. |
| `filesystem` | `:53` | 13 | Pattern `std::fs::\|use std::fs\|File::open\|…` (`:133`). Bleibt unberührt, **solange** keine Ordner-Inhaltsprüfung eingebaut wird (siehe Widerspruch 2). |
| `threads` / `workers` | `:54-55` | 15 / 7 | Unberührt: keine neuen Threads, keine neue `*worker*.rs`. |
| `view_floor` | `:39` | 2116 | Unberührt: nichts wandert nach `reprise-view`. |
| Dead-code-Allowlist | `:199-246` | byte-genau | Unberührt, solange **kein** neues `#[allow(dead_code)]` entsteht. Neue Strings brauchen keins: `strings_online_sources.rs` trägt bereits ein datei-weites, und die neuen `strings.rs`-Konstanten haben Konsumenten. |

Zähltiefe beachten: `*_tests.rs` und `#[cfg(test)]`-Blöcke auf Spaltenebene 0
sind ausgeschlossen (`:77-86`) — die Auslagerung nach `first_run_tests.rs` aus
Task 4 hält Testcode aus allen Budgets heraus.

**Schritte:** `scripts/check-frontend-thinness.sh` laufen lassen, die vom Skript
gemeldeten Ist-Zahlen **wörtlich** übernehmen (nicht schätzen), und die Änderung
in der Commit-Nachricht begründen — das Skript verlangt das selbst
(`:43-45`: „Never raise one without a reason recorded in the commit message").

**Akzeptanzkriterium:**
```
scripts/check-frontend-thinness.sh
```
endet mit `Frontend thinness lint passed`.

---

## Schreibreihenfolge in `CompletionOptions`

Die Spezifikation verlangt: Gate zuerst, Module danach — „sonst überschreibt die
Saat die Auswahl". Das Ergebnis ist richtig, die Begründung trifft den Code
nicht.

`set_enabled` sät ein Modul nur, wenn dafür **noch kein Wert gespeichert** ist:

```rust
// crates/reprise-core/src/online_sources.rs:63
if settings::get_setting_in(conn, &key)?.is_none() {
    settings::set_bool_in(conn, &key, enabled)?;
}
```

Ein zuvor geschriebener Modulwert wäre also nicht `is_none()` und würde von der
Saat übersprungen — beide Reihenfolgen führen zum selben Endzustand. Die
vorgeschriebene Reihenfolge bleibt trotzdem die richtige, aus zwei anderen
Gründen:

1. Sie ist die einzige, die **unabhängig von diesem Implementierungsdetail**
   stimmt. Ein späterer Umbau der Saat darf die Auswahl nicht kippen können.
2. Sie hält die vier Module, die der Assistent nicht erfragt (New Releases,
   Concerts, Artwork, Online Lyrics), auf ihren First-Enable-Defaults — genau
   das, was die Spezifikation unter „Was ausdrücklich bleibt" verlangt.

**Folge für die Tests:** Ein Test, der behauptet „Gate vor Modulen", würde bei
umgekehrter Reihenfolge trotzdem grün — er misst nichts. Prüfe deshalb das
**Ergebnis** (die Modulzustände nach `apply_wizard_selection`), nicht die
Reihenfolge. Der Testfall in Task 3 wählt bewusst „Radio aus, Podcasts an", also
die Inverse der Saat: nur so wäre eine gewinnende Saat überhaupt sichtbar.

## Was ausdrücklich unangetastet bleibt

- **Das Banner bleibt für Bestandsinstallationen.** `FirstRunDecision::
  ExistingLibrary` markiert `onboarding_completed` weiter still
  (`first_run.rs:123-127`) und darf `online_discovery_banner_completed`
  **nicht** setzen — diese Leute sehen den Assistenten nie.
- **`AlreadyCompleted`** unverändert (`first_run.rs:98-100`).
- **Die anderen Module des Gates** — New Releases, Concerts, Artwork, Online
  Lyrics — rührt der Assistent nicht an; sie behalten ihre First-Enable-Defaults
  aus `online_sources.rs:88-98`.
- **Nichts wird gelöscht:** keine Abos, keine Favoriten, keine bestehende
  Modulentscheidung. `set_enabled` respektiert bereits entschiedene Module
  (`online_sources.rs:63`).
- **Das Gate-Verhalten selbst** und die Preferences-Seite bleiben, wie sie sind.
- **Nicht in diesem Auftrag:** der neue Banner-Text für Bestandsinstallationen
  („can now" ist falsch — `strings_online_sources.rs:48-50`); das ist ein
  separater Auftrag.

## Offene Punkte / Widersprüche zur Spezifikation

1. **Die Begründung der Schreibreihenfolge trifft den Code nicht.** Siehe oben:
   `online_sources.rs:63` überspringt bereits entschiedene Module. Die
   vorgeschriebene Reihenfolge bleibt gültig, aber ein Test darf sie nicht als
   messbar behandeln.

2. **„Nothing found in ~/Music" behauptet eine Prüfung, die es nicht gibt.** Der
   Text liest sich, als hätte Reprise in den XDG-Musikordner geschaut. Tatsächlich
   ist `ShowWizard` nur „`library_root` ist leer" (`first_run.rs:97-105`) — über
   den Inhalt des Ordners weiß der Code nichts, und einen Ordner-Scan gibt es im
   Frontend heute nicht.
   Zwei Wege:
   - **(a), empfohlen:** kein Dateisystemzugriff. Der Untertitel benennt den
     Ort, an dem Reprise noch keine Bibliothek hat. Kostet nichts, hebt kein
     `filesystem`-Budget.
   - **(b):** echte Prüfung. Dann gehört sie nach `reprise-core` (im Frontend
     hebt jedes `std::fs::` das Budget von 13, `check-frontend-thinness.sh:133`),
     und die Zeile braucht einen zweiten Text für „Ordner enthält Musik".
   Der Entwurf plant (a). **Entscheidung im Grilling.**

3. **`ONBOARDING_GROUP_LIBRARY_FOLDER = "Library folder"` neben dem bestehenden
   `LIBRARY_FOLDER = "Library Folder"`** (`strings.rs:196`) — zwei Strings, die
   sich nur in der Groß-/Kleinschreibung unterscheiden, ergeben zwei
   Übersetzungseinträge für dieselbe Sache. Die Spezifikation nennt beide
   (einmal unter „neu", einmal unter „wiederverwenden"). Der Entwurf folgt der
   Spezifikation und legt den neuen an; die schlankere Alternative wäre,
   `LIBRARY_FOLDER` auch als Gruppentitel zu nehmen.

4. **Der Untertitel im Design-Screenshot ist kürzer als der Bestandsstring:**
   „Stations and live streams · radio-browser.info" vs.
   `ONLINE_SOURCES_RADIO_SUBTITLE` = „… radio-browser.info **directory**"
   (`strings_online_sources.rs:39-40`). Der Entwurf nimmt den Bestandsstring —
   „alles andere wiederverwenden" wiegt schwerer als die Verkürzung im Bild.

5. **`NET-4` sagt heute „never a modal"** (`docs/ux-rules.md:2692`). Der
   Assistent ist ein Dialog. Der Satz gilt dem Banner, nicht dem Assistenten —
   deshalb ergänzt Task 6 beide Regeln so, dass sie sich nicht widersprechen.
   Ohne diese Ergänzung steht eine `[active]`-Regel im Widerspruch zum Produkt.

6. **`first_run::run` bekommt ein Argument mehr** (`start_scan_of`), also ändert
   sich die Aufrufstelle in `window_runtime_wiring.rs:693-700`. Das ist die
   angekündigte Nachbardatei-Änderung, kein Vertragsbruch.

## Gate-Liste

Abgeleitet aus `scripts/check-merge-readiness.sh` (`origin/dev`), in dessen
Reihenfolge. `scripts/ci-quality.sh:31` ruft genau dieses Skript mit
`--no-fetch` auf — es gibt keine zweite Kette.

Während der Arbeit, nach jeder Task:
```
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test -p reprise-core online_sources
cargo test -p reprise-gnome --bin reprise first_run
```

Vor dem PR, in dieser Reihenfolge (die einschlägigen Stufen aus
`check-merge-readiness.sh:51-121`):
```
scripts/check-architecture.sh
scripts/check-accessibility-semantics.sh
scripts/check-frontend-thinness.sh
scripts/check-ux-traceability.sh
scripts/check-gnome-idioms.sh
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace --exclude reprise-platform-linux
scripts/check-display-tests.sh
```

Die vollständige Kette liegt in einem Kommando:
```
scripts/check-merge-readiness.sh
```
Sie verlangt einen **sauberen** Arbeitsbaum inklusive untracked Dateien
(`check-merge-readiness.sh:37-41`) und dass die Basis Vorfahre von `HEAD` ist
(`:43-46`). Sie läuft lange; erwarte nicht, dass sie in einem Rutsch durchläuft.

**Baseline zuerst.** Display-Tests sind im Rudel flaky, und einige sind auf
`dev` bereits rot — Rot ist nicht automatisch die eigene Schuld. Fahre deshalb
**vor der ersten Änderung** einmal:
```
scripts/check-display-tests.sh > "$SCRATCH/display-baseline.log" 2>&1
```
und halte die Bilanzzeile am Ende (`== display test summary ==`,
`check-display-tests.sh:246-249`) fest. Nach der Arbeit dieselbe Messung; nur
die **Differenz** zählt. Ganze Logs nie zurücklesen — `grep` auf
`^failed:` und die Namensliste darunter genügt.

---

## Parallelität

Der Schnitt wurde ernsthaft versucht. Ergebnis: **zwei Stränge, mit einer harten
Merge-Reihenfolge**, und der Gewinn ist klein.

### Strang 1 — `core-first-enable`

**Zweck:** Die First-Enable-Defaults auslesbar machen und die Auswahl→Modul-
Abbildung samt Tests bauen. Vorbedingung für Strang 2: dieser kompiliert nicht
ohne `first_enable_default_for`, `WizardSourceSelection` und
`apply_wizard_selection`.

**Dateibesitz (Globs):**
```
crates/reprise-core/src/online_sources.rs
```

**Tasks:** 2, 3.

**Eigene Gates:** `cargo test -p reprise-core online_sources`,
`cargo clippy --locked -p reprise-core --all-targets -- -D warnings`.

### Strang 2 — `gnome-welcome-dialog`

**Zweck:** Strings, Dialogaufbau, Schreibpfad, Tests, Regel, Budgets.

**Dateibesitz (Globs):**
```
crates/reprise-gnome/src/ui/strings.rs
crates/reprise-gnome/src/ui/strings_online_sources.rs
crates/reprise-gnome/src/ui/first_run*.rs
crates/reprise-gnome/src/ui/mod.rs
crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs
docs/ux-rules.md
scripts/check-frontend-thinness.sh
```

**Tasks:** 1, 4, 5, 6, 7.

**Eigene Gates:** `cargo test -p reprise-gnome --bin reprise first_run`,
`scripts/check-ux-traceability.sh`, `scripts/check-frontend-thinness.sh`.

### Merge-Reihenfolge

**1 vor 2**, ohne Ausnahme. Strang 2 referenziert drei Core-Symbole, die es vor
Strang 1 nicht gibt.

### Warum der Schnitt wenig bringt

Strang 1 ist ein einziger Commit in einer einzigen Datei; Strang 2 trägt fünf
der sieben Tasks, und die Tasks 4, 5, 6 fassen alle `first_run.rs` bzw. seine
Testdatei an — dort gibt es **keine disjunkte Dateigruppe**. Wer den Overhead
zweier Worktrees sparen will, fährt beide Stränge nacheinander in einem: das ist
ein gültiges Ergebnis und kostet nur die Sequenz, die ohnehin erzwungen ist.

Ein dritter Strang „nur Strings" (Task 1) wurde verworfen: Task 1 ist ein
Zehnzeiler, aber `strings.rs` ist eine viel angefasste Datei — ein eigener
Strang darauf produziert mehr Konfliktfläche mit anderen laufenden Arbeiten als
er Zeit spart.

### Post-Merge-Cross-Checks

Jede dieser Prüfungen liest Dateien, die **kein** Strang allein besitzt. Sie
gehören nach dem Merge beider Stränge ausgeführt, nicht in einen Strang:

1. `scripts/check-frontend-thinness.sh` — zählt über **alle** `.rs` unter
   `crates/reprise-gnome/src` und vergleicht auf Gleichheit
   (`:104-114`). Nur nach dem Merge steht die Ist-Zahl fest; erst dann darf
   Task 7 die Budgets festschreiben. Läuft Task 7 innerhalb von Strang 2, muss
   sie nach dem Merge **wiederholt** werden.
2. `scripts/check-ux-traceability.sh` — liest `docs/ux-rules.md` **und** alle
   Tests unter `crates/` (`:43-49`, `:81-90`). Die Regel wohnt in Strang 2, ihre
   Abdeckung teils in Testnamen — die Gegenrichtung ist nur global messbar.
3. `scripts/check-architecture.sh` — deckelt jede `.rs`-Datei bei 800 Zeilen
   (`:18-24`), also auch die Summe aus beiden Strängen.
4. `scripts/check-display-tests.sh` — fährt **alle** ignorierten Tests des
   Crates, nicht nur die neuen. Gegen die vorab genommene Baseline halten.
5. `cargo test --locked --workspace --exclude reprise-platform-linux` — die
   einzige Messung, die Core und Frontend zusammen sieht.
6. `scripts/check-merge-readiness.sh` als Abschluss.
