---
slug: first-run-welcome-screen
worktree: /home/marvin/Projects/reprise-first-run-welcome-screen
branch: feature/first-run-welcome-screen
phase: shipped
codex_session:
created: 2026-08-15
---
# Ein Welcome-Screen für den ersten Start

> Inventarisiert gegen `origin/dev` = `334c9adb30`. Jede Belegzeile stammt aus
> `git show origin/dev:<pfad>`, nicht aus dem lokalen Checkout (der hängt
> ~30 Commits zurück). Wo die Zeilennummern vom Vorentwurf abweichen, gilt die
> Zahl in diesem Dokument — sie wurde nachgezählt.

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
| Gate schreiben | `online_sources.rs:45` | `pub fn set_enabled(db: &crate::db::Db, value: bool) -> Result<(), rusqlite::Error>` |
| First-Enable-Saat | `online_sources.rs:63` | seedet ein Modul nur, wenn `settings::get_setting_in(conn, &key)?.is_none()` |
| First-Enable-Defaults | `online_sources.rs:88-98` | `fn first_enable_source_defaults() -> [(&'static ModuleDescriptor, bool); 7]` — **privat** |
| Radio-Default | `online_sources.rs:94` | `(&modules::RADIO_MODULE, true)`; alle anderen sechs `false` |
| Modul lesen | `crates/reprise-core/src/modules.rs:202` | `pub fn is_enabled(db: &crate::db::Db, module: &ModuleDescriptor) -> Result<bool, rusqlite::Error>` |
| Modul schreiben | `modules.rs:214-221` | `pub fn set_enabled(db: &Db, module: &ModuleDescriptor, value: bool) -> Result<(), rusqlite::Error>` |
| Modul-IDs | `modules.rs:98, 111, 119` | `"podcasts"`, `"youtube"`, `"radio"` — Konstanten `PODCASTS_MODULE`, `YOUTUBE_MODULE`, `RADIO_MODULE` |
| Onboarding-Flag | `crates/reprise-core/src/library/settings_api.rs:69/74` | `get_onboarding_completed(db) -> Result<bool, _>` / `set_onboarding_completed(db, completed) -> Result<(), _>` |
| Banner-Flag | `settings_api.rs:79/84-90` | `get_online_discovery_banner_completed(db)` / `set_online_discovery_banner_completed(db, completed)` |
| Library-Root | `settings_api.rs:49/54` | `get_library_root(db) -> Result<Option<String>, _>` / `set_library_root(db, root)` |
| Rhythmbox gefunden | `crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs:87` | `pub(in crate::ui) fn rhythmbox_import_available() -> bool` |
| Rhythmbox-Angebot | `first_run.rs:48` | `fn rhythmbox_offer(decision: FirstRunDecision, available: bool) -> Option<bool>` — lokal in `first_run.rs` |
| Banner bauen | `online_discovery_banner.rs:53-56` | `pub(in crate::ui) fn build(db: &Rc<Db>, on_review: impl Fn() + 'static) -> Option<OnlineDiscoveryBanner>` |
| Banner-Frühausstieg | `online_discovery_banner.rs:57-65` | `Ok(false)`/`Err` → `return None`, **vor** dem ersten Widget |
| Scan starten | `crates/reprise-gnome/src/ui/scan/scan_worker.rs:58` | `pub(in crate::ui) fn spawn_scan(folder: PathBuf, db_path: PathBuf, …)` — bereits aus `crate::ui` aufrufbar |
| Aufrufstelle des Assistenten | `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs:693-700` | `super::first_run::run(window, scan_button, scan_controls, conn, first_run_decision, &present_rhythmbox_import)` |
| Dialogschließung | `first_run.rs:189-190` | `TransientFocusGuard::capture(window)` + `bind_closable_dialog(&dialog, &setup)` — bindet nur Fokus und Ctrl+W, ruft `complete` **nicht** |
| Ordnerzeile in Preferences | `preferences/preference_library.rs:127-131` | `ActionRow` mit `title = LIBRARY_FOLDER`, `subtitle = library_root_text(self)` (voller Pfad), Suffix-Button `CHOOSE_FOLDER` mit `valign(Center)` |

**Achtung zur Modulidentität:** `ModuleDescriptor` sind `pub const`, keine
`static`. Referenzen auf Konstanten dürfen dupliziert werden — vergleiche
Module deshalb immer über `module.id`, **nie** über Zeiger-Identität.

**Achtung zur Sichtbarkeit:** `modules::enabled_key` ist `pub(crate)`
(`modules.rs:198`) — nur aus `reprise-core` heraus aufrufbar, also ausschließlich
in den Core-Tests aus Task 3. `Db::open_in_memory` liegt in `db_handle.rs:47` und
liefert `Result<Self, DbError>`; `.unwrap()` trägt in beiden Crates.

### Bestehende Strings (wiederverwenden, nicht neu anlegen)

| Konstante | Datei:Zeile | Text |
|---|---|---|
| `ONBOARDING_WELCOME` | `ui/strings.rs:131` | `"Welcome to Reprise"` |
| `ONBOARDING_PRIVACY` | `strings.rs:132` | `"Reprise keeps your library local. …"` |
| `ONBOARDING_IMPORT_FROM_RHYTHMBOX` | `strings.rs:133` | `"Import from Rhythmbox"` |
| `ONBOARDING_IMPORT_FROM_RHYTHMBOX_DESCRIPTION` | `strings.rs:134-135` | `"Rhythmbox was found. Choose what Reprise should import."` |
| `ONBOARDING_SKIP` | `strings.rs:146` | `"Skip for Now"` |
| `ONBOARDING_SET_UP` | `strings.rs:147` | `"Set Up Library"` |
| `LIBRARY_FOLDER` | `strings.rs:202` | `"Library Folder"` |
| `NO_LIBRARY_FOLDER` | `strings.rs:203` | `"No folder selected"` |
| `CHOOSE_FOLDER` | `strings.rs:204` | `"Choose Folder…"` |
| `PREFERENCES_ONLINE_SOURCES` | `ui/strings_online_sources.rs:27` | `"Online sources"` |
| `ONLINE_SOURCES_USE_YOUTUBE` | `strings_online_sources.rs:45` | `"Use YouTube"` |
| `ONLINE_SOURCES_USE_PODCASTS` | `strings_online_sources.rs:46` | `"Use Podcasts"` |
| `ONLINE_SOURCES_USE_RADIO` | `strings_online_sources.rs:47` | `"Use Radio"` |
| `ONLINE_SOURCES_YOUTUBE_SUBTITLE` | `strings_online_sources.rs:35-36` | `"Channels as audio episodes · channel feeds, audio via yt-dlp"` |
| `ONLINE_SOURCES_PODCASTS_SUBTITLE` | `strings_online_sources.rs:37-38` | `"Shows as audio episodes · RSS feeds, search via Apple Podcasts"` |
| `ONLINE_SOURCES_RADIO_SUBTITLE` | `strings_online_sources.rs:39-40` | `"Stations and live streams · radio-browser.info directory"` |

Alle drei `ONLINE_SOURCES_USE_*` sind heute **ungenutzt** (deshalb steht
`#![allow(dead_code)]` in `strings_online_sources.rs:1`); der Assistent ist ihr
erster Konsument.

Das Formatier-Muster für Platzhalter steht in `strings_online_sources.rs:20-25`:
eine Konstante mit `{name}` plus eine Funktion, die `formatted(KONST, &[("name",
wert)])` ruft. `formatted` ist `pub(super)` (`strings.rs:18`), der Ersetzer ist
`i18n.rs:113-119` (schlichtes `{name}` → Wert).

---

## Entscheidungen aus dem Grilling

Diese sechs Punkte **überschreiben** den Vorentwurf und, wo vermerkt, auch den
Wortlaut des Auftrags. Codex soll erkennen können, wo der Plan bewusst abweicht.

### E1 — Die Ordnerzeile behauptet keine Dateisystemprüfung

*(weicht vom Auftragstext ab)*

„Nothing found in ~/Music" liest sich, als hätte Reprise in den Ordner
geschaut. Hat es nicht: `ShowWizard` heißt nur „`library_root` ist leer"
(`first_run.rs:97-105`). Also **kein** `std::fs`, kein Ordner-Scan, kein
`filesystem`-Budget. Der Untertitel im ungewählten Zustand benennt nur den Ort,
an dem Reprise noch keine Bibliothek hat:

```
Reprise has no library yet · ~/Music
```

Der String heißt entsprechend `ONBOARDING_NO_LIBRARY_YET_IN`, **nicht**
`ONBOARDING_NOTHING_FOUND_IN`. Liefert
`glib::user_special_dir(glib::UserDirectory::Music)` `None`, **entfällt der
Untertitel** — nie einen Pfad raten. Damit ist Widerspruch 2 des Entwurfs eine
Feststellung, keine offene Frage.

### E2 — Regelwerk: `NET-4a` neu, `NET-4` präzisiert

`NET-4` sagt heute „exactly one dismissible banner appears in the Library on the
**first launch after the update**" … „is never a modal or a toast"
(`docs/ux-rules.md:2708-2715`). Der Assistent ist ein Dialog. Der ergänzende Satz
in `NET-4` muss die Regel deshalb ausdrücklich auf **Bestandsinstallationen**
begrenzen, sonst liest sich „never a modal" als Verbot des Assistenten. `NET-4a`
trägt `[active] [gtk]`, der Display-Test heißt `net_4a_…` und trägt exakt
`#[ignore = "requires a display; run via xvfb-run"]`.

### E3 — „Skip for Now" behält einen im Dialog gewählten Ordner und startet den Scan

*(weicht vom Auftragstext ab — dort hieß es „kein Ordner")*

Skip überspringt, was ungefragt bleibt — Quellen und Import —, nicht das, was
der Nutzer im Dialog sichtbar selbst eingetragen hat. Ein Ordner, der in der
Zeile steht und beim Klick auf Skip verschwindet, liest sich als Fehler.

Konkret: der Scan-Callback läuft auf **beiden** Abschlusswegen, sobald ein Ordner
gemerkt ist. Alles andere an Skip bleibt unverändert: keine Quelle
(`WizardSourceSelection::default()`), kein Import, beide Flags gesetzt. Ein Test
misst diesen Weg (Skip mit gemerktem Ordner → Scan-Callback gerufen, Gate zu).

### E4 — Gruppentitel neu, gewählte Zeile im Preferences-Muster

*(weicht vom Auftragstext ab und schließt eine Lücke darin)*

Der Gruppentitel bleibt der **neue** String
`ONBOARDING_GROUP_LIBRARY_FOLDER = "Library folder"` in Satzschreibung — alle
Gruppentitel im Repo sind so („Online sources", „Playback", „Appearance"). Er
lebt neben `LIBRARY_FOLDER = "Library Folder"`; das ist beabsichtigt, nicht
versehentlich.

Im **gewählten** Zustand nimmt die Zeile das Muster aus
`preference_library.rs:127-131`: Titel = `strings::LIBRARY_FOLDER`
(„Library Folder"), Untertitel = **der volle Pfad**. Nicht der Ordnername — das
weicht vom Auftrag ab, damit derselbe Zustand in Assistent und Preferences gleich
aussieht. Widerspruch 3 des Entwurfs ist damit erledigt.

Der Button wechselt im gewählten Zustand auf einen **neuen** String
`ONBOARDING_CHANGE_FOLDER = N_!("Change…")`. Im Repo gibt es keinen passenden:
`TAG_CHANGE_COVER` = „Change cover…" (`strings_tag_edit.rs:234`) und
`LOCATION_CHANGE_IN_LOCATION` = „Change in Location →" (`strings_location.rs:33`)
sind beide gebunden. Der Auftrag hatte diesen String nicht aufgeführt — der Plan
schließt die Lücke.

### E5 — Die Schalter zeigen den Ist-Zustand, wenn das Gate schon an ist

*(weicht vom Auftragstext ab; inhaltlich die wichtigste Änderung)*

Es gibt einen **erreichbaren** Pfad, auf dem der Assistent eine bewusste
Nutzerentscheidung überschreibt:

> Escape schließt den Dialog heute, ohne irgendetwas zu schreiben
> (`first_run.rs:189-190` bindet nur Fokus und Ctrl+W; `complete` wird nicht
> gerufen) → das Discovery-Banner erscheint (`banner_completed` false, Gate aus)
> → der Nutzer klickt „Review in Preferences", schaltet das Gate an und wählt
> Podcasts an, Radio ab → Neustart → `library_root` immer noch leer und
> `onboarding_completed` immer noch false, also **`ShowWizard` erneut** → der
> Assistent zeigt die First-Enable-Defaults und schreibt sie über die getroffene
> Wahl.

Das verletzt „Nichts wird gelöscht: keine bestehende Modulentscheidung."

**Die Regel, die dieser Plan festschreibt:** der Assistent zeigt immer, was
gerade gilt, und schreibt nur, was der Nutzer im Dialog sieht.

- Ist das Gate an (`online_sources::is_enabled`), kommen die drei
  Schalterstellungen aus den **gespeicherten Modulzuständen**
  (`modules::is_enabled`).
- Ist es aus, aus `first_enable_default_for(...)`.

Die Radio-Vorauswahl bleibt damit aus dem Code gelesen und nicht hardcodiert —
die Anforderung des Auftrags bleibt erfüllt. Die Core-Funktion
(`WizardSourceSelection::current_or_first_enable_defaults`) deckt beide Fälle
intern ab, damit das Frontend **keinen** `rusqlite`-Typ in einer eigenen Signatur
nennen muss (`check-frontend-thinness.sh:131`, hartes Gleichheitsbudget). Der
Aufrufer behandelt den Fehler durch Loggen und Rückfall auf
`WizardSourceSelection::from_first_enable_defaults()` — siehe Task 5, Schritt 1.

### E6 — Ein Strang

Der Schnitt wurde versucht und **verworfen**. Begründung steht belegt im
Abschnitt `## Parallelität` am Ende. Die Post-Merge-Cross-Checks des Entwurfs
werden dadurch zur normalen Gate-Liste vor dem PR; keine davon entfällt.

---

## Tasks

### Task 1 — Neue Strings anlegen

**Ziel:** Alle Texte, die der neue Aufbau braucht, existieren als `N_!`-Konstanten,
bevor die erste Widget-Zeile geschrieben wird.

**Files:** `crates/reprise-gnome/src/ui/strings.rs`,
`crates/reprise-gnome/src/ui/strings_online_sources.rs`

**Schritte:**

1. In `strings.rs` neben den bestehenden `ONBOARDING_*`-Konstanten (also im
   Block `131-147`):
   ```rust
   pub const ONBOARDING_GROUP_LIBRARY_FOLDER: &str = N_!("Library folder");
   pub const ONBOARDING_GROUP_IMPORT: &str = N_!("Import");
   pub const ONBOARDING_CHANGE_FOLDER: &str = N_!("Change\u{2026}");
   pub const ONBOARDING_NO_LIBRARY_YET_IN: &str =
       N_!("Reprise has no library yet · {folder}");

   pub fn onboarding_no_library_yet_in(folder: &str) -> String {
       formatted(ONBOARDING_NO_LIBRARY_YET_IN, &[("folder", folder)])
   }
   ```
   `formatted` steht bereits in derselben Datei (`strings.rs:18`) — kein Import
   nötig. Das Muster spiegelt `online_content_show_sources`
   (`strings_online_sources.rs:20-25`). Für das Auslassungszeichen `\u{2026}`
   verwenden, wie `TAG_CHANGE_COVER` (`strings_tag_edit.rs:234`).
2. In `strings_online_sources.rs`:
   ```rust
   pub const ONBOARDING_ONLINE_SOURCES_BODY: &str = N_!(
       "Three sources may reach the network. Off makes this a local player: no requests, no downloads, and their sidebar entries stay hidden."
   );
   pub const ONBOARDING_ONLINE_SOURCES_FOOTER: &str =
       N_!("You can change this any time in Preferences · Plugins.");
   ```
   Die Datei trägt bereits `#![allow(dead_code)]` (Zeile 1) und ist über
   `strings.rs:126-129` re-exportiert — es genügt `strings::ONBOARDING_…`.

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
(`online_sources.rs:88`). Ohne Änderung müsste die UI `true` für Radio hardcoden
— genau das verbietet der Auftrag.

**Minimale Sichtbarkeitsänderung** (kleiner, als die Funktion selbst öffentlich
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
(`online_sources.rs:214`) rot werden. Danach zurückändern. Die Mutation muss
Produktionscode treffen, nicht den Test.

**Akzeptanzkriterium:**
```
cargo test -p reprise-core online_sources
```
grün, und die Mutations-Probe war rot.

---

### Task 3 — Auswahl, Ist-Zustand und Schreibfunktion als Core-API

**Ziel:** „Womit öffnet der Assistent" und „was geht in die Datenbank" sind
Core-Funktionen mit Tests, bevor irgendein Widget sie benutzt.

**Files:** `crates/reprise-core/src/online_sources.rs`

**Warum Core und nicht `ui/first_run.rs`:** Die Abbildung kennt
`ModuleDescriptor`s und die First-Enable-Regel — beides Core-Wissen. Zusätzlich
zählt `scripts/check-frontend-thinness.sh:131` **jede** Nennung von `Connection`
oder `rusqlite::` in `crates/reprise-gnome/src` gegen ein hart gleichgesetztes
Budget; eine Core-Funktion hält Rückgabetypen wie `Result<…, rusqlite::Error>`
aus dem Frontend heraus.

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
       /// The state a fresh install opens in: exactly what a first enable
       /// would write, read from the table rather than repeated here.
       pub fn from_first_enable_defaults() -> Self {
           Self {
               radio: first_enable_default_for(&modules::RADIO_MODULE),
               podcasts: first_enable_default_for(&modules::PODCASTS_MODULE),
               youtube: first_enable_default_for(&modules::YOUTUBE_MODULE),
           }
       }

       /// What the wizard must show. The gate being on means somebody already
       /// answered this question — Preferences, or an earlier session — and the
       /// wizard has to display that answer instead of overwriting it with the
       /// first-enable defaults. Reachable: closing the dialog with Escape
       /// writes nothing, so the wizard returns after the banner has been used.
       pub fn current_or_first_enable_defaults(db: &Db) -> Result<Self, rusqlite::Error> {
           if !is_enabled(db)? {
               return Ok(Self::from_first_enable_defaults());
           }
           Ok(Self {
               radio: modules::is_enabled(db, &modules::RADIO_MODULE)?,
               podcasts: modules::is_enabled(db, &modules::PODCASTS_MODULE)?,
               youtube: modules::is_enabled(db, &modules::YOUTUBE_MODULE)?,
           })
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

**Unit-Tests** (gleicher `mod tests`; `migrated_db()` existiert bereits,
`online_sources.rs:145-147`, ebenso das Muster `db.conn()` **innerhalb von
Core** — im Frontend ist `.conn(` ein harter Bann,
`check-frontend-thinness.sh:132`):

```rust
#[test]
fn a_fresh_install_opens_the_wizard_with_the_first_enable_defaults() {
    let db = migrated_db();
    let selection = WizardSourceSelection::current_or_first_enable_defaults(&db).unwrap();
    assert!(selection.radio);
    assert!(!selection.podcasts);
    assert!(!selection.youtube);
}

#[test]
fn an_open_gate_makes_the_wizard_show_what_is_stored() {
    // The reachable path: Escape wrote nothing, the banner sent the user to
    // Preferences, and there they chose Podcasts on / Radio off — the inverse
    // of the first-enable defaults, so a wizard that ignored them is visible.
    let db = migrated_db();
    set_enabled(&db, true).unwrap();
    modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();
    modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();

    let selection = WizardSourceSelection::current_or_first_enable_defaults(&db).unwrap();
    assert!(!selection.radio);
    assert!(selection.podcasts);
    assert!(!selection.youtube);
}

#[test]
fn completing_the_wizard_unchanged_keeps_the_stored_choice() {
    let db = migrated_db();
    set_enabled(&db, true).unwrap();
    modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();
    modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();

    let selection = WizardSourceSelection::current_or_first_enable_defaults(&db).unwrap();
    apply_wizard_selection(&db, selection).unwrap();

    assert!(is_enabled(&db).unwrap());
    assert!(!modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
    assert!(modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
    assert!(!modules::is_enabled(&db, &modules::YOUTUBE_MODULE).unwrap());
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

**Mutations-Probe (verpflichtend):** Ändere
`current_or_first_enable_defaults` probeweise so, dass sie den Gate-Zweig
überspringt und immer `Self::from_first_enable_defaults()` liefert. Danach
müssen `an_open_gate_makes_the_wizard_show_what_is_stored` **und**
`completing_the_wizard_unchanged_keeps_the_stored_choice` rot sein. Danach
zurückändern. Läuft nur einer rot, misst der andere nichts — dann erst den Test
reparieren, nicht die Probe.

**Akzeptanzkriterium:**
```
cargo test -p reprise-core online_sources
```
grün, beide Mutations-Proben waren rot. Kein `cargo test --exact` mit unsicheren
Namen verwenden — ein `--exact` mit einem Namen, den es nicht gibt, beendet sich
mit 0, nachdem es nichts gelaufen ist. Prüfe die Testnamen in der Ausgabe, nicht
die Bilanzzeile.

---

### Task 4 — Abschluss-Schreibpfad in `first_run.rs`, testbar herausgezogen

**Ziel:** Beide Abschlusswege schreiben ihre Flags über eine Funktion, die ohne
GTK aufrufbar und damit ohne Display testbar ist.

**Files:** `crates/reprise-gnome/src/ui/first_run.rs`,
`crates/reprise-gnome/src/ui/first_run_tests.rs` (neu)

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
   (`first_run.rs:233-239`). `WizardSourceSelection` leitet `Default` ab (alles
   `false`), damit das trägt.
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
   Ordnerweg auslösen, `log_smoke_result`).
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

Die acht bestehenden Tests (`first_run.rs:296-372`) wandern **unverändert** mit.
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
    // (online_discovery_banner.rs:57-65), so this needs no display.
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
`crates/reprise-gnome/src/ui/first_run_tests.rs`,
`crates/reprise-gnome/src/ui/mod.rs`,
`crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`

**Schritte:**

1. **Startzustand der Schalter lesen** (E5). In `run(...)`, vor dem Bau der
   Gruppe:
   ```rust
   let selection = online_sources::WizardSourceSelection::current_or_first_enable_defaults(&conn)
       .unwrap_or_else(|error| {
           tracing::warn!(%error, "could not read online source state; showing first-enable defaults");
           online_sources::WizardSourceSelection::from_first_enable_defaults()
       });
   ```
   Der Aufrufer nennt damit **keinen** `rusqlite`-Typ: der Fehler wird nur als
   Closure-Parameter gebunden. Der Rückfall ist der Erstinstallations-Zustand —
   nie „alles aus", das wäre eine stille dritte Wahrheit.
2. **Online-Quellen-Gruppe** in die neue Datei `first_run_sources.rs`
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
   ist Pflicht, „kein Markup" steht so im Auftrag. Titel und Untertitel kommen
   aus den Bestandsstrings; für Radio gilt `ONLINE_SOURCES_RADIO_SUBTITLE`
   („… radio-browser.info **directory**"), nicht die kürzere Fassung aus dem
   Design-Screenshot — „alles andere wiederverwenden" wiegt schwerer als die
   Verkürzung im Bild.
   Die Fußnote ist ein eigenes `gtk4::Label` (`adw::PreferencesGroup` hat keinen
   Footer-Slot) mit `wrap(true)`, `xalign(0.0)` und den CSS-Klassen
   `dim-label` + `caption` — Muster `browse_chooser.rs:89-92`. Beides erfüllt
   `CONTRAST-5`: kein roher Accent als Textfarbe, sondern Adwaita-Klassen.
3. **Ordnergruppe** in `first_run.rs`, Gruppentitel
   `ONBOARDING_GROUP_LIBRARY_FOLDER`. Sie wird nur gebaut, wenn `library_root`
   leer ist — bei `ShowWizard` ist das per Definition der Fall
   (`first_run.rs:97-105`); die Bedingung bleibt trotzdem explizit, damit der
   Test sie messen kann.
   Ungewählter Zustand: `adw::ActionRow` mit
   `title = strings::NO_LIBRARY_FOLDER`,
   `subtitle = strings::onboarding_no_library_yet_in(&display)` — wobei
   `display` der Anzeigename des XDG-Musikordners ist. Suffix ist ein
   `gtk4::Button::with_label(&strings::text(strings::CHOOSE_FOLDER))` mit
   `valign(gtk4::Align::Center)`, Muster `preference_library.rs:134-138`.
   Den XDG-Musikordner liefert `glib::user_special_dir(
   glib::UserDirectory::Music)` — im Repo bisher nur in
   `crates/reprise-platform-linux/src/diagnostics.rs:82` benutzt; im Frontend
   ist es neu. **Liefert es `None`, entfällt der Untertitel** (`set_subtitle("")`
   bzw. Zeile ohne Untertitel bauen) — nie einen Pfad raten (E1). Es gibt
   **keinen** Dateisystemzugriff: kein `std::fs`, keine Inhaltsprüfung.
   Für die Anzeige `~/Music` das Home-Präfix durch `~` ersetzen. Einen Helfer
   dafür gibt es im Repo **nicht**; lege ihn als reine Funktion an und teste ihn
   in `first_run_tests.rs`:
   ```rust
   /// `~/Music` reads as a place; `/home/someone/Music` reads as a machine.
   /// Only an exact prefix match is folded — a sibling like `/home/someone2`
   /// must not become `~2`.
   fn tilde_path(path: &Path, home: &Path) -> String
   ```
   Tests: exakter Treffer faltet, Geschwister-Pfad faltet nicht, `home` gleich
   `path` ergibt `~`, ein Pfad außerhalb bleibt unverändert.
4. **Gewählter Zustand** (E4). Nach der Auswahl im Dialog nimmt dieselbe Zeile
   das Preferences-Muster (`preference_library.rs:127-131`):
   `title = strings::text(strings::LIBRARY_FOLDER)` („Library Folder"),
   `subtitle` = der **volle Pfad** (nicht der Ordnername, nicht die
   Tilde-Form — Preferences zeigt dort auch den vollen Pfad), Button-Label
   wechselt auf `strings::text(strings::ONBOARDING_CHANGE_FOLDER)` („Change…").
5. **Picker im Dialog.** Muster wörtlich von `scan_flow.rs:71-105` übernehmen:
   `gtk4::FileDialog::builder().title(…).modal(true).build()`, dann
   `glib::spawn_future_local(async move { … dialog.select_folder_future(
   Some(&window)).await … })`, `DialogError::Dismissed` / `Cancelled` nur
   `debug!`, alles andere `error!`.
   Der gewählte Pfad wird **nur gemerkt** (`Rc<RefCell<Option<PathBuf>>>`) und in
   die Zeile geschrieben. **Kein** `settings::set_library_root` hier: im Frontend
   schreibt den Root sonst nur `main.rs:232` (CLI-Argument), im Normalfall
   schreibt ihn der Scan-Pfad in Core. Zwei Schreiber wären zwei Wahrheiten.
6. **Abschluss und Ordnerweg** (E3). `should_open_folder` (`first_run.rs:44-46`)
   bekommt das Vorwissen dazu, und der gemerkte Ordner wird auf **beiden**
   Wegen gescannt:
   ```rust
   /// Which folder path the wizard takes on the way out.
   ///
   /// `Skip` keeps a folder the user typed into the dialog: skipping means
   /// skipping what was never asked for — sources and import — not the one
   /// thing the user filled in themselves. A folder that shows in the row and
   /// vanishes on click reads as a bug.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   enum FolderOutcome {
       /// Nothing chosen, and the user asked to set up: open the picker.
       OpenPicker,
       /// A folder is remembered: scan it, on either exit.
       ScanChosen,
       /// Skipped without choosing anything.
       Nothing,
   }

   fn folder_outcome(response: CompletionResponse, folder_chosen: bool) -> FolderOutcome {
       match (response, folder_chosen) {
           (_, true) => FolderOutcome::ScanChosen,
           (CompletionResponse::SetUp, false) => FolderOutcome::OpenPicker,
           (CompletionResponse::Skip, false) => FolderOutcome::Nothing,
       }
   }
   ```
   `OpenPicker` feuert weiterhin `scan_button.emit_clicked()`
   (`first_run.rs:221-225`). `ScanChosen` feuert **nicht** den Button — das
   öffnete den Picker ein zweites Mal —, sondern den neuen Callback
   `start_scan_of: Rc<dyn Fn(PathBuf)>`.
   `arm_rhythmbox_import_after_library_setup` (`first_run.rs:74-95`) bleibt
   unverändert: der Import hängt weiter an `scan_controls.add_on_complete`, also
   an einer fertigen Bibliothek — in beiden Ordnerwegen.
7. **Neues Argument und Aufrufstelle.** `first_run::run` bekommt
   `start_scan_of: Rc<dyn Fn(PathBuf)>` zusätzlich. Er wird in
   `window_runtime_wiring.rs` neben `present_rhythmbox_import`
   (`window_runtime_wiring.rs:685-692`) gebaut und ruft
   `scan_worker::spawn_scan` (`scan_worker.rs:58`, bereits `pub(in crate::ui)`);
   die Aufrufstelle `window_runtime_wiring.rs:693-700` ändert sich mit. Das ist
   die angekündigte Nachbardatei-Änderung, kein Vertragsbruch.
8. **Dialoghülle.** `content_height(430)` → `content_height(620)`
   (`first_run.rs:187`), und der `content`-Box (`first_run.rs:163-172`) kommt in
   ein `gtk4::ScrolledWindow` mit `propagate_natural_height(true)` und
   `hscrollbar_policy(gtk4::PolicyType::Never)`, das per
   `toolbar.set_content(...)` gesetzt wird (`first_run.rs:183`). Fehlt ein
   Block, schrumpft der Dialog dadurch von selbst.
9. **Accessibility.** Jede neue Zeile braucht ihre Semantik, und
   `gtk4::AccessibleRole` lässt sich **nur im Konstruktor** setzen — nachträglich
   nicht (`.builder().accessible_role(…)`, Muster
   `preference_plugins.rs:266`). Die Fußnote und der Datenschutzabsatz sind
   `AccessibleRole::Presentation` bzw. bleiben Labels; die `SwitchRow`s und die
   `ActionRow` tragen ihre Adwaita-Rollen und brauchen sprechende Titel — keine
   nackten Icon-Buttons.
10. Buttons bleiben wie heute: `ONBOARDING_SKIP` schlicht, `ONBOARDING_SET_UP`
    mit `add_css_class("suggested-action")` (`first_run.rs:158`), rechtsbündig
    über `buttons.set_halign(gtk4::Align::End)` (`first_run.rs:160`).
11. `ui/mod.rs` um das neue Modul ergänzen (neben `pub mod first_run;`,
    `ui/mod.rs:52`).

**Tests in `first_run_tests.rs`, ohne Display:**

```rust
#[test]
fn a_chosen_folder_is_scanned_on_both_exits() {
    assert_eq!(folder_outcome(CompletionResponse::SetUp, true), FolderOutcome::ScanChosen);
    assert_eq!(folder_outcome(CompletionResponse::Skip, true), FolderOutcome::ScanChosen);
    assert_eq!(folder_outcome(CompletionResponse::SetUp, false), FolderOutcome::OpenPicker);
    assert_eq!(folder_outcome(CompletionResponse::Skip, false), FolderOutcome::Nothing);
}
```

Dazu der Verhaltenstest zu E3, der den Callback wirklich misst — Skip mit
gemerktem Ordner ruft `start_scan_of` **und** lässt das Gate zu:

```rust
#[test]
fn skipping_with_a_chosen_folder_scans_it_and_keeps_the_gate_shut() {
    let db = Db::open_in_memory().unwrap();
    let scanned: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    // Same shape the dialog uses; the callback is the unit under test.
    let start_scan_of: Rc<dyn Fn(PathBuf)> = {
        let scanned = scanned.clone();
        Rc::new(move |folder| scanned.borrow_mut().push(folder))
    };

    persist_completion(&db, CompletionOptions::default());
    if folder_outcome(CompletionResponse::Skip, true) == FolderOutcome::ScanChosen {
        start_scan_of(PathBuf::from("/music"));
    }

    assert_eq!(scanned.borrow().as_slice(), [PathBuf::from("/music")]);
    assert!(!reprise_core::online_sources::is_enabled(&db).unwrap());
}
```

**Akzeptanzkriterium:**
```
cargo build -p reprise-gnome
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test -p reprise-gnome --bin reprise first_run
```
alle drei sauber; die neuen Testnamen stehen in der Ausgabe.

---

### Task 6 — Regel `NET-4a`, `NET-4` präzisiert, und der Display-Test

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

1. In `docs/ux-rules.md` direkt hinter `NET-4` (heute `2708-2715`) einfügen:
   ```
   - **NET-4a** [active] [gtk] — On a fresh install the first-run wizard asks
     the online-sources question once, in the same dialog as the music folder
     and the Rhythmbox import: with the gate still shut it opens on the
     first-enable defaults — Radio preselected, Podcasts and YouTube off — and
     with the gate already open it opens on the stored module states instead,
     so a choice made in Preferences is displayed, never overwritten. Both
     exits — "Skip for Now" and "Set Up Library" — close the discovery banner
     of `NET-4`, so the question is never asked twice. No source chosen leaves
     the gate shut and writes no module. An existing library never sees the
     wizard and keeps the banner.
   ```
   Format wörtlich nach dem Muster von `NET-4` — der Parser liest
   `^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(active|planned|replaced)` und danach
   `\[(core|gtk|e2e|manual)\]` (`check-ux-traceability.sh:24-32`). `[gtk]` ist
   richtig: die Abdeckung liegt in `crates/reprise-gnome`.
2. `NET-4` um einen Satz ergänzen, der die Regel ausdrücklich auf
   **Bestandsinstallationen** begrenzt (E2) — etwa: „This banner is the path for
   an *existing* installation; a fresh install is asked once by the first-run
   wizard of `NET-4a` and never sees the banner. 'Never a modal' constrains this
   banner, not that wizard." Ohne diesen Satz steht eine `[active]`-Regel im
   Widerspruch zum Produkt.
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
   Prüfe darin, **gegen den Widget-Baum und nicht gegen einen Screenshot**:
   - die Gruppen in Reihenfolge (Ordner, Import, Online-Quellen),
   - drei `SwitchRow`s mit `ONLINE_SOURCES_USE_RADIO/PODCASTS/YOUTUBE` als
     Titel und den `*_SUBTITLE`-Strings als Untertitel,
   - Radio `is_active()`, Podcasts und YouTube nicht,
   - der Rhythmbox-Switch inaktiv,
   - die Ordnerzeile mit Titel `NO_LIBRARY_FOLDER` und Button-Label
     `CHOOSE_FOLDER`,
   - Fußnote trägt `ONBOARDING_ONLINE_SOURCES_FOOTER`.
   Beginne mit `let _main_context = crate::ui::test_main_context::lock_main_context();`
   und `if gtk4::init().is_err() { return; }` — Muster
   `online_discovery_banner.rs:143-146`.
4. Ein zweiter regel-benannter Display-Test für E5 — die Schalter am
   Widget-Baum, nicht nur die Core-Funktion:
   ```rust
   #[test]
   #[ignore = "requires a display; run via xvfb-run"]
   fn net_4a_an_open_gate_makes_the_wizard_show_the_stored_sources() { … }
   ```
   Gate an, Podcasts an, Radio aus in die DB schreiben, Gruppe bauen, und die
   drei `SwitchRow::is_active()` prüfen: Podcasts an, Radio aus, YouTube aus.
5. Weitere `#[ignore]`-Tests, ohne Regel-ID im Namen (sie messen
   Strukturvarianten, keine eigene Regel):
   - Ordnerblock fehlt, wenn `library_root` gesetzt ist,
   - Rhythmbox-Block fehlt ohne Fund (`rhythmbox_offer(…, false) == None`,
     `first_run.rs:48-50`),
   - `build(&db, || {})` nach `ExistingLibrary` liefert weiter `Some` —
     der Widget-Zweig aus `online_discovery_banner.rs:69`,
   - der gewählte Zustand der Ordnerzeile: Titel `LIBRARY_FOLDER`, Untertitel
     der volle Pfad, Button-Label `ONBOARDING_CHANGE_FOLDER` (E4).

**Akzeptanzkriterium:**
```
scripts/check-ux-traceability.sh
xvfb-run -a cargo test -p reprise-gnome --bin reprise net_4a_ -- --ignored
```
Erstes Kommando grün; zweites meldet `2 passed` und **nennt beide Testnamen**.
Verlasse dich nicht auf die Bilanzzeile allein — `--exact` mit einem veralteten
Namen beendet sich mit 0, nachdem es nichts gelaufen ist (deshalb prüft
`check-display-tests.sh:191-196` zusätzlich auf `test result: ok. 1 passed;`).
Benutze hier bewusst den Präfix-Filter `net_4a_`, nicht `--exact`.

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
| `rusqlite` | `:52` | 113 | Pattern `rusqlite::\|use rusqlite\|params!\|\.prepare\(\|\.query_row\(\|Connection` (`:131`). Der Plan vermeidet solche Nennungen bewusst (Task 4 Punkt 2, Task 5 Punkt 1) — trotzdem messen. |
| `filesystem` | `:53` | 13 | Pattern `std::fs::\|use std::fs\|File::open\|…` (`:133`). Bleibt unberührt: E1 verbietet jede Ordner-Inhaltsprüfung. `glib::user_special_dir` und `PathBuf` matchen das Pattern nicht. |
| `threads` / `workers` | `:54-55` | 15 / 7 | Unberührt: keine neuen Threads, keine neue `*worker*.rs`. |
| `view_floor` | `:39` | 2116 | Unberührt: nichts wandert nach `reprise-view`. |
| Dead-code-Allowlist | `:199-246` | byte-genau | Unberührt, solange **kein** neues `#[allow(dead_code)]` entsteht. Neue Strings brauchen keins: `strings_online_sources.rs` trägt bereits ein datei-weites, und die neuen `strings.rs`-Konstanten haben Konsumenten. |

Zähltiefe beachten: `*_tests.rs` und `#[cfg(test)]`-Blöcke auf Spaltenebene 0
sind ausgeschlossen (`:77-86`) — die Auslagerung nach `first_run_tests.rs` aus
Task 4 hält Testcode aus allen Budgets heraus.

**Schritte:** `scripts/check-frontend-thinness.sh` laufen lassen, die vom Skript
gemeldeten Ist-Zahlen **wörtlich** übernehmen (nicht schätzen, nicht rechnen),
und die Änderung in der Commit-Nachricht begründen — das Skript verlangt das
selbst (`:43-45`: „Never raise one without a reason recorded in the commit
message"). Diese Task läuft **zuletzt**, nachdem aller Produktionscode steht;
eine frühere Messung veraltet.

**Akzeptanzkriterium:**
```
scripts/check-frontend-thinness.sh
```
endet mit `Frontend thinness lint passed`.

---

## Schreibreihenfolge in `apply_wizard_selection`

Der Auftrag verlangt: Gate zuerst, Module danach — „sonst überschreibt die Saat
die Auswahl". Das Ergebnis ist richtig, die Begründung trifft den Code nicht.

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
   das, was unter „Was ausdrücklich unangetastet bleibt" steht.

**Folge für die Tests:** Ein Test, der behauptet „Gate vor Modulen", würde bei
umgekehrter Reihenfolge trotzdem grün — er misst nichts. Prüfe deshalb das
**Ergebnis** (die Modulzustände nach `apply_wizard_selection`), nicht die
Reihenfolge. Der Testfall in Task 3 wählt bewusst „Radio aus, Podcasts an", also
die Inverse der Saat: nur so wäre eine gewinnende Saat überhaupt sichtbar.

## Was ausdrücklich unangetastet bleibt

- **Das Banner bleibt für Bestandsinstallationen.**
  `FirstRunDecision::ExistingLibrary` markiert `onboarding_completed` weiter
  still (`first_run.rs:123-127`) und darf `online_discovery_banner_completed`
  **nicht** setzen — diese Leute sehen den Assistenten nie.
- **`AlreadyCompleted`** unverändert (`first_run.rs:98-100`).
- **Escape und Ctrl+W schließen den Dialog weiterhin, ohne etwas zu schreiben.**
  `bind_closable_dialog` (`first_run.rs:190`) ruft `complete` nicht; der
  Assistent kommt beim nächsten Start wieder, weil `library_root` leer und
  `onboarding_completed` false bleiben. Das ist heutiges Verhalten und bleibt so
  — bewusst unveränderte Kante, jetzt entschärft durch E5: der wiederkommende
  Assistent zeigt, was inzwischen gilt, statt es zu überschreiben.
- **Die anderen Module des Gates** — New Releases, Concerts, Artwork, Online
  Lyrics — rührt der Assistent nicht an; sie behalten ihre First-Enable-Defaults
  aus `online_sources.rs:88-98`.
- **Nichts wird gelöscht:** keine Abos, keine Favoriten, keine bestehende
  Modulentscheidung. `set_enabled` respektiert bereits entschiedene Module
  (`online_sources.rs:63`), und E5 schließt den letzten Pfad, auf dem der
  Assistent selbst eine überschrieben hätte.
- **Das Gate-Verhalten selbst** und die Preferences-Seite bleiben, wie sie sind.
- **`settings::set_library_root` schreibt der Dialog nicht** — zwei Schreiber
  wären zwei Wahrheiten.
- **Nicht in diesem Auftrag:** der neue Banner-Text für Bestandsinstallationen
  („can now" ist falsch — `strings_online_sources.rs:48-50`); das ist ein
  separater Auftrag.

## Gate-Liste

Abgeleitet aus `scripts/check-merge-readiness.sh` (`origin/dev`), in dessen
Reihenfolge. `scripts/ci-quality.sh:31` ruft genau dieses Skript mit
`--no-fetch` auf — es gibt keine zweite Kette.

**Baseline zuerst — vor der ersten Änderung.** Display-Tests sind im Rudel
flaky, und einige sind auf `dev` bereits rot; Rot ist nicht automatisch die
eigene Schuld. Also einmal:
```
scripts/check-display-tests.sh > "$SCRATCH/display-baseline.log" 2>&1
```
und die Bilanzzeile am Ende festhalten (`== display test summary ==`,
`check-display-tests.sh:246-249`). Nach der Arbeit dieselbe Messung; nur die
**Differenz** zählt. Ganze Logs nie zurücklesen — `grep` auf `^failed:` und die
Namensliste darunter genügt.

Während der Arbeit, nach jeder Task:
```
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test -p reprise-core online_sources
cargo test -p reprise-gnome --bin reprise first_run
```

Vor dem PR, in dieser Reihenfolge (die einschlägigen Stufen aus
`check-merge-readiness.sh:51-121`). Jede Zeile liest mehr, als eine einzelne
Task besitzt — deshalb gehören sie alle hierher und nicht in eine Task:
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

Was jede dieser Stufen hier misst — sie sind der Grund, warum sie einzeln
aufgeführt sind:

1. `check-architecture.sh` — deckelt jede `.rs`-Datei bei 800 Zeilen
   (`:18-24`). Betroffen: `first_run.rs` (heute 373) plus alles, was Task 5
   hinzufügt; deshalb die Auslagerung nach `first_run_sources.rs` und
   `first_run_tests.rs`.
2. `check-frontend-thinness.sh` — zählt über **alle** `.rs` unter
   `crates/reprise-gnome/src` und vergleicht auf Gleichheit (`:104-114`). Die
   Ist-Zahl steht erst fest, wenn aller Produktionscode steht; Task 7 läuft
   deshalb zuletzt.
3. `check-ux-traceability.sh` — liest `docs/ux-rules.md` **und** alle Tests
   unter `crates/` (`:43-49`, `:81-90`). Regel und Abdeckung müssen sich in
   beide Richtungen finden.
4. `check-display-tests.sh` — fährt **alle** ignorierten Tests des Crates, nicht
   nur die neuen. Gegen die vorab genommene Baseline halten.
5. `cargo test --locked --workspace --exclude reprise-platform-linux` — die
   einzige Messung, die Core und Frontend zusammen sieht.

Die vollständige Kette liegt in einem Kommando:
```
scripts/check-merge-readiness.sh
```
Sie verlangt einen **sauberen** Arbeitsbaum inklusive untracked Dateien
(`check-merge-readiness.sh:37-41`) und dass die Basis Vorfahre von `HEAD` ist
(`:43-46`). Sie läuft lange; erwarte nicht, dass sie in einem Rutsch durchläuft.

---

## Parallelität

**Ergebnis: ein Strang. Der Schnitt wurde versucht und verworfen.** Das ist ein
gültiges Ergebnis, keine ausgelassene Prüfung.

Der naheliegende Schnitt wäre gewesen: Strang 1 `core-first-enable`
(`crates/reprise-core/src/online_sources.rs`, Tasks 2–3), Strang 2
`gnome-welcome-dialog` (Strings, Dialog, Schreibpfad, Regel, Budgets, Tasks 1
und 4–7). Vier Gründe, jeder für sich hinreichend:

1. **Strang 2 kompiliert nicht, solange Strang 1 läuft.** Er referenziert
   `first_enable_default_for`, `WizardSourceSelection`,
   `current_or_first_enable_defaults` und `apply_wizard_selection` — vier
   Symbole, die es vor Strang 1 nicht gibt. Ein Strang, der nicht baut, kann
   nicht parallel verifiziert werden; er kann nur warten.
2. **„Merge-Reihenfolge 1 vor 2 ohne Ausnahme" ist eine Sequenz, keine
   Parallelität.** Wenn die einzige zulässige Reihenfolge festliegt und der
   zweite Strang auf den ersten wartet, ist der Gewinn null und der Overhead
   zweier Worktrees real.
3. **E5 verstärkt die Kopplung.** `current_or_first_enable_defaults` ist eine
   weitere Core-Funktion, die ausschließlich die UI liest — der Schnitt trennt
   damit nicht zwei Aufgaben, sondern eine Funktion von ihrem einzigen Aufrufer.
4. **`check-frontend-thinness.sh` ist global und hart.** Es misst Gleichheit
   über **alle** `.rs` unter `crates/reprise-gnome/src` (`:104-114`); die
   Ist-Zahl stünde erst nach dem Merge beider Stränge fest. Task 7 müsste in
   Strang 2 laufen **und** nach dem Merge zwingend ein zweites Mal — eine
   Doppelmessung, die es bei einem Strang nicht gibt.

Ein dritter Strang „nur Strings" (Task 1) wurde ebenfalls verworfen: Task 1 ist
ein Zehnzeiler, aber `strings.rs` ist eine viel angefasste Datei — ein eigener
Strang darauf produziert mehr Konfliktfläche mit anderen laufenden Arbeiten, als
er Zeit spart.

Die Post-Merge-Cross-Checks, die ein Zwei-Strang-Schnitt gebraucht hätte, sind
vollständig in die **Gate-Liste vor dem PR** übernommen; keine davon entfällt.

**Vorgehen:** ein Worktree, ein Branch, die sieben Tasks in Reihenfolge, jede
einzeln committfähig.
