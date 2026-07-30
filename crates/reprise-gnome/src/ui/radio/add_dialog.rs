use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::db::Db;
use reprise_core::radio::playlist::PlaylistKind;
use reprise_core::radio::search::{SearchCriteria, SearchOrder, StationCandidate};
use reprise_core::radio::{self, RadioError};

use super::radio_chips::{self, NearYouAction};
use super::station_preview::StationPreview;
use crate::ui::{one_shot_task, source_add_action, strings};

type AddedCallback = Rc<dyn Fn()>;
type LocationSettingsCallback = Rc<dyn Fn()>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddInput {
    Empty,
    Search(String),
    Url(String),
}

/// `NET-1a` / `C1`: `online_sources::network_allowed(conn,
/// &modules::SOURCE_IMAGES_MODULE)`, computed fresh at every call so each
/// favicon tile reflects the current gate — this dialog never lets the
/// widget read settings itself. A free function (rather than a method) so
/// its wiring is testable without constructing the GTK dialog.
pub(super) fn images_allowed(db: &Db) -> bool {
    reprise_core::online_sources::network_allowed(db, &reprise_core::modules::SOURCE_IMAGES_MODULE)
        .unwrap_or(false)
}

pub(super) fn classify_input(input: &str) -> AddInput {
    let input = input.trim();
    if input.is_empty() {
        return AddInput::Empty;
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        AddInput::Url(input.to_owned())
    } else {
        AddInput::Search(input.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddDialogPhase {
    Idle,
    Searching,
    Results(Vec<StationCandidate>),
    Previewing,
    Preview(StationPreview),
    Error(AddFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AddFailure {
    Search,
    Preview,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AddDialogState {
    pub phase: AddDialogPhase,
    generation: u64,
}

impl Default for AddDialogState {
    fn default() -> Self {
        Self {
            phase: AddDialogPhase::Idle,
            generation: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddResult {
    Search(Vec<StationCandidate>),
    Preview(StationPreview),
    Error,
}

impl AddDialogState {
    pub(super) fn begin(mut self, input: &AddInput) -> (Self, u64) {
        self.generation = self.generation.wrapping_add(1);
        self.phase = match input {
            AddInput::Empty => AddDialogPhase::Idle,
            AddInput::Search(_) => AddDialogPhase::Searching,
            AddInput::Url(_) => AddDialogPhase::Previewing,
        };
        let generation = self.generation;
        (self, generation)
    }

    pub(super) fn accept(mut self, generation: u64, result: AddResult) -> Self {
        if self.generation != generation {
            return self;
        }
        let failure = if matches!(self.phase, AddDialogPhase::Searching) {
            AddFailure::Search
        } else {
            AddFailure::Preview
        };
        self.phase = match result {
            AddResult::Search(rows) => AddDialogPhase::Results(rows),
            AddResult::Preview(preview) => AddDialogPhase::Preview(preview),
            AddResult::Error => AddDialogPhase::Error(failure),
        };
        self
    }

    pub(super) fn can_confirm(&self) -> bool {
        matches!(self.phase, AddDialogPhase::Preview(_))
    }
}

pub(super) fn playlist_kind(value: &str) -> Option<PlaylistKind> {
    let path = value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase();
    if path.ends_with(".pls") {
        Some(PlaylistKind::Pls)
    } else if path.ends_with(".m3u") || path.ends_with(".m3u8") {
        Some(PlaylistKind::M3u)
    } else {
        None
    }
}

struct DialogWidgets {
    dialog: adw::Dialog,
    entry: gtk4::SearchEntry,
    /// `RAD-5`: the three shortcut chips, kept addressable for tests.
    chip_metal: gtk4::Button,
    chip_top_voted: gtk4::Button,
    chip_near_you: gtk4::Button,
    spinner: gtk4::Spinner,
    status: gtk4::Label,
    results: gtk4::ListBox,
    preview: gtk4::Box,
    confirm: gtk4::Button,
    fetch_metadata: gtk4::Switch,
    fetch_row: gtk4::Box,
}

pub(super) struct RadioAddDialog {
    widgets: DialogWidgets,
    state: Rc<RefCell<AddDialogState>>,
    conn: Rc<Db>,
    on_added: AddedCallback,
    /// `RAD-5`: wired after construction (`set_on_location_settings`), once
    /// the caller can reach Preferences — see `RadioView`/`window.rs`. Not
    /// set at all in tests that never call the setter, in which case a
    /// no-location "Near you" click is silently a no-op rather than a panic.
    on_location_settings: RefCell<Option<LocationSettingsCallback>>,
    /// `NET-3` point 4: the same connectivity seam `RadioView` reads for
    /// `NET-3b`'s Play affordance (shared `Rc`, not a copy) — offline
    /// disables search and skips the ICY probe for a pasted URL.
    connectivity: Rc<Cell<Connectivity>>,
}

impl RadioAddDialog {
    pub(super) fn new(
        conn: Rc<Db>,
        connectivity: Rc<Cell<Connectivity>>,
        on_added: impl Fn() + 'static,
    ) -> Rc<Self> {
        let entry = gtk4::SearchEntry::builder()
            .placeholder_text(strings::text(strings::RADIO_DIALOG_HINT))
            .build();
        // `RAD-5`: the three one-click radio-browser searches.
        let chips = radio_chips::build();
        let spinner = gtk4::Spinner::new();
        let status = gtk4::Label::new(None);
        status.set_xalign(0.0);
        status.set_wrap(true);
        status.add_css_class("dim-label");
        let results = gtk4::ListBox::new();
        results.add_css_class("boxed-list");
        results.set_selection_mode(gtk4::SelectionMode::None);
        let preview = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        preview.add_css_class("card");
        preview.set_visible(false);

        let fetch_label = gtk4::Label::new(Some(&strings::text(strings::RADIO_FETCH_METADATA)));
        fetch_label.set_hexpand(true);
        fetch_label.set_xalign(0.0);
        let fetch_metadata = gtk4::Switch::builder().active(true).build();
        let fetch_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        fetch_row.append(&fetch_label);
        fetch_row.append(&fetch_metadata);

        let cancel = gtk4::Button::with_label(&strings::text(strings::RADIO_CANCEL));
        let confirm = gtk4::Button::with_label(&strings::text(strings::RADIO_ADD));
        confirm.add_css_class("suggested-action");
        confirm.set_sensitive(false);
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        buttons.set_halign(gtk4::Align::End);
        buttons.append(&cancel);
        buttons.append(&confirm);

        // SRC-7: the same two-line footing as the podcast and channel dialogs —
        // where the results come from, and why added ones stop appearing.
        let footnote = gtk4::Label::new(Some(&format!(
            "{}\n{}",
            strings::text(strings::RADIO_COMMUNITY_FOOTNOTE),
            strings::text(strings::SOURCE_SUBSCRIBED_DROP_OUT)
        )));
        footnote.set_xalign(0.0);
        footnote.set_wrap(true);
        footnote.add_css_class("dim-label");
        footnote.add_css_class("caption");

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.append(&entry);
        content.append(&chips.root);
        content.append(&spinner);
        content.append(&status);
        // SRC-8: the result list is the only part that may grow. A bare
        // GtkListBox contributes every row's natural height, so fifty hits push
        // the footer — and every Add action with it — past the window edge.
        let results_scroller = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .propagate_natural_height(true)
            .vexpand(true)
            .child(&results)
            .build();
        // SRC-8: same clearance from the overlay scrollbar as the podcast and
        // channel dialogs, so no row action sits underneath it.
        results.set_margin_end(6);
        content.append(&results_scroller);
        content.append(&preview);
        content.append(&fetch_row);
        content.append(&footnote);
        content.append(&buttons);

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(
            &strings::text(strings::RADIO_DIALOG_TITLE),
            "",
        )));
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));
        let dialog = adw::Dialog::builder()
            .child(&toolbar)
            .content_width(560)
            .content_height(620)
            .build();

        let this = Rc::new(Self {
            widgets: DialogWidgets {
                dialog,
                entry,
                chip_metal: chips.metal,
                chip_top_voted: chips.top_voted,
                chip_near_you: chips.near_you,
                spinner,
                status,
                results,
                preview,
                confirm,
                fetch_metadata,
                fetch_row,
            },
            state: Rc::new(RefCell::new(AddDialogState::default())),
            conn,
            on_added: Rc::new(on_added),
            on_location_settings: RefCell::new(None),
            connectivity,
        });
        {
            let weak = Rc::downgrade(&this);
            this.widgets.entry.connect_activate(move |entry| {
                if let Some(this) = weak.upgrade() {
                    this.submit(&entry.text());
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.widgets.confirm.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.confirm_preview();
                }
            });
        }
        {
            let dialog = this.widgets.dialog.downgrade();
            cancel.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.widgets.chip_metal.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.run_chip_search(
                        &strings::text(strings::RADIO_CHIP_METAL_DE),
                        radio_chips::metal_in_germany_criteria(),
                    );
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.widgets.chip_top_voted.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.run_chip_search(
                        &strings::text(strings::RADIO_CHIP_TOP_VOTED),
                        SearchCriteria::default(),
                    );
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.widgets.chip_near_you.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.run_near_you();
                }
            });
        }
        this
    }

    /// `RAD-5`: wired after construction, once the caller (`RadioView`,
    /// then `window.rs`) can reach Preferences — the same deep-link shape
    /// `PreferencesContext::present_plugins` already uses for the Online
    /// Lyrics settings button, reused rather than inventing a second
    /// navigation mechanism.
    pub(super) fn set_on_location_settings(&self, callback: impl Fn() + 'static) {
        *self.on_location_settings.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn present(self: &Rc<Self>, parent: &impl IsA<gtk4::Widget>) {
        self.widgets.entry.set_text("");
        self.render(AddDialogState::default());
        // `NET-3` point 4: the reason search is unavailable is visible
        // immediately, before the user types anything.
        if self.connectivity.get().is_offline() {
            self.widgets
                .status
                .set_text(&strings::text(strings::RADIO_SEARCH_NEEDS_NETWORK));
            self.widgets.status.set_visible(true);
        }
        self.widgets.dialog.present(Some(parent));
        self.widgets.entry.grab_focus();
    }

    fn submit(self: &Rc<Self>, input: &str) {
        let input = classify_input(input);
        if input == AddInput::Empty {
            self.render(AddDialogState::default());
            return;
        }
        // NET-1a: radio-browser search and the ICY probe are both network
        // paths, so the switch is honoured before either is dispatched.
        let allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::RADIO_MODULE,
        )
        .unwrap_or(false);
        if !allowed {
            self.widgets
                .status
                .set_text(&strings::text(strings::ONLINE_SOURCES_TURNED_OFF));
            return;
        }
        // `NET-3` point 4: search needs the network and is refused offline;
        // a URL still proceeds below, just without the ICY probe.
        if matches!(input, AddInput::Search(_)) && self.connectivity.get().is_offline() {
            self.widgets
                .status
                .set_text(&strings::text(strings::RADIO_SEARCH_NEEDS_NETWORK));
            self.widgets.status.set_visible(true);
            return;
        }
        if let AddInput::Url(url) = &input {
            if self.connectivity.get().is_offline() {
                self.submit_url_offline(url);
                return;
            }
        }
        let (state, generation) = self.state.borrow().clone().begin(&input);
        self.render(state);
        let result = match input {
            AddInput::Search(terms) => {
                let order = radio::config::load(&self.conn)
                    .unwrap_or_default()
                    .search_order;
                one_shot_task::spawn("reprise-radio-search", move || {
                    radio::search::search(&terms, order).map(AddResult::Search)
                })
            }
            AddInput::Url(url) => {
                let fetch_metadata = self.widgets.fetch_metadata.is_active();
                one_shot_task::spawn("reprise-radio-preview", move || {
                    preview_url(&url, fetch_metadata).map(AddResult::Preview)
                })
            }
            AddInput::Empty => return,
        };
        self.dispatch(generation, result);
    }

    /// `RAD-5`: runs a chip's fixed [`SearchCriteria`] the same way a
    /// free-text search runs — same NET-1a gate, same generation-guarded
    /// state machine, same results rendering — just skipping
    /// [`classify_input`]. The entry field shows `entry_text` (the chip's
    /// own label) purely so the user can see what produced the results;
    /// re-submitting it verbatim would not reproduce the same query, since
    /// [`radio::search::search`] matches station names, not tags/countries.
    fn run_chip_search(self: &Rc<Self>, entry_text: &str, criteria: SearchCriteria) {
        self.widgets.entry.set_text(entry_text);
        // NET-1a: identical gate to `submit` — radio-browser search is a
        // network path regardless of which affordance triggered it.
        let allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::RADIO_MODULE,
        )
        .unwrap_or(false);
        if !allowed {
            self.widgets
                .status
                .set_text(&strings::text(strings::ONLINE_SOURCES_TURNED_OFF));
            return;
        }
        let (state, generation) = self
            .state
            .borrow()
            .clone()
            .begin(&AddInput::Search(entry_text.to_owned()));
        self.render(state);
        // `RAD-5`: chips always order by votes — "Top voted" would
        // otherwise silently follow whatever order the user last picked
        // for free-text search, which is not what its label promises.
        let result = one_shot_task::spawn("reprise-radio-search", move || {
            radio::search::search_by(&criteria, SearchOrder::Votes).map(AddResult::Search)
        });
        self.dispatch(generation, result);
    }

    /// `RAD-5`/`O-4`: "Near you" reuses the app-level, already-consented
    /// location instead of asking for its own — it never queries the XDG
    /// portal or a geocoder itself. [`radio_chips::near_you_action`] is the
    /// pure decision; this method just carries it out.
    fn run_near_you(self: &Rc<Self>) {
        let location = reprise_core::location::app_location(&self.conn)
            .ok()
            .flatten();
        match radio_chips::near_you_action(location.as_ref()) {
            NearYouAction::Search(criteria) => {
                self.run_chip_search(&strings::text(strings::RADIO_CHIP_NEAR_YOU), criteria);
            }
            NearYouAction::OpenLocationSettings => {
                if let Some(callback) = self.on_location_settings.borrow().clone() {
                    callback();
                }
            }
        }
    }

    /// Shared by [`submit`](Self::submit) and
    /// [`run_chip_search`](Self::run_chip_search): awaits the spawned
    /// task's single result and applies it under the same
    /// generation-guarded state machine, so a stale response from an
    /// earlier search or chip click can never overwrite a newer one.
    fn dispatch(
        self: &Rc<Self>,
        generation: u64,
        result: std::io::Result<async_channel::Receiver<Result<AddResult, RadioError>>>,
    ) {
        let receiver = match result {
            Ok(receiver) => receiver,
            Err(error) => {
                tracing::warn!(%error, "could not start radio add task");
                let state = self
                    .state
                    .borrow()
                    .clone()
                    .accept(generation, AddResult::Error);
                self.render(state);
                return;
            }
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let result = receiver.recv().await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            let result = match result {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    tracing::warn!(%error, "radio add task failed");
                    AddResult::Error
                }
                Err(error) => {
                    tracing::warn!(%error, "radio add task closed without a result");
                    AddResult::Error
                }
            };
            let state = this.state.borrow().clone().accept(generation, result);
            this.render(state);
        });
    }

    /// `NET-3` point 4: the URL path while offline — no ICY probe, straight
    /// to the normal `Preview`/confirm step with a locally-built preview
    /// (`playlist_kind` detection is pure URL parsing, no network). Unlike
    /// Podcasts, there is no later background refresh for radio that would
    /// enrich this with real name/genre/bitrate metadata — the user can
    /// re-add the station once online to pick that up, or edit it by hand.
    fn submit_url_offline(self: &Rc<Self>, url: &str) {
        // `render` after `begin` persists the bumped generation into
        // `self.state` — required before `accept` below can recognise it,
        // exactly like the async (online) path does between dispatch and
        // its task's response; here both calls just happen synchronously
        // instead of across an await.
        let (state, generation) = self
            .state
            .borrow()
            .clone()
            .begin(&AddInput::Url(url.to_owned()));
        self.render(state);
        let preview = StationPreview::manual(&strings::text(strings::RADIO_STREAM_DETECTED), url);
        let state = self
            .state
            .borrow()
            .clone()
            .accept(generation, AddResult::Preview(preview));
        self.render(state);
    }

    fn render(self: &Rc<Self>, state: AddDialogState) {
        self.state.replace(state.clone());
        self.widgets.spinner.stop();
        self.widgets.spinner.set_visible(false);
        self.widgets.status.set_visible(false);
        self.widgets.status.remove_css_class("error");
        self.widgets.preview.set_visible(false);
        self.widgets.confirm.set_sensitive(state.can_confirm());
        self.widgets.results.remove_all();
        self.widgets.fetch_row.set_visible(matches!(
            &state.phase,
            AddDialogPhase::Previewing
                | AddDialogPhase::Preview(_)
                | AddDialogPhase::Error(AddFailure::Preview)
        ));
        match state.phase {
            AddDialogPhase::Idle => {}
            AddDialogPhase::Searching | AddDialogPhase::Previewing => {
                self.widgets.spinner.start();
                self.widgets.spinner.set_visible(true);
                self.widgets
                    .status
                    .set_text(&strings::text(strings::RADIO_SEARCHING));
                self.widgets.status.set_visible(true);
            }
            AddDialogPhase::Results(rows) => self.render_results(rows),
            AddDialogPhase::Preview(preview) => self.render_preview(&preview),
            AddDialogPhase::Error(failure) => {
                self.widgets.status.set_text(&strings::text(match failure {
                    AddFailure::Search => strings::RADIO_SEARCH_FAILED,
                    AddFailure::Preview => strings::RADIO_PREVIEW_FAILED,
                }));
                self.widgets.status.add_css_class("error");
                self.widgets.status.set_visible(true);
            }
        }
    }

    fn render_results(self: &Rc<Self>, rows: Vec<StationCandidate>) {
        let favorites = radio::station::list(&self.conn).map_or_else(
            |error| {
                tracing::warn!(%error, "could not load radio favorites for search filtering");
                Vec::new()
            },
            |rows| {
                rows.into_iter()
                    .map(|row| (row.uuid.unwrap_or_default(), row.stream_url))
                    .collect()
            },
        );
        let rows = radio::search::filter_new_stations(rows, &favorites);
        self.widgets.status.remove_css_class("error");
        self.widgets.status.set_text(&format!(
            "{} · {}",
            strings::text(strings::RADIO_RESULTS_HEADER),
            strings::radio_results_count(rows.len())
        ));
        self.widgets.status.set_visible(true);
        for candidate in rows {
            let row = gtk4::ListBoxRow::new();
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
            let tile = crate::ui::podcasts::source_image::SourceImage::new(
                candidate.favicon_url.as_deref(),
                "network-wireless-symbolic",
                40,
                images_allowed(&self.conn),
            );
            let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
            copy.set_hexpand(true);
            let title = gtk4::Label::new(Some(&candidate.name));
            title.set_xalign(0.0);
            let details =
                gtk4::Label::new(Some(&radio::search::format_candidate_details(&candidate)));
            details.set_xalign(0.0);
            details.add_css_class("dim-label");
            details.add_css_class("caption");
            copy.append(&title);
            copy.append(&details);
            // SRC-7: the same compact action the podcast and channel dialogs use.
            let station_name = candidate.name.clone();
            let add =
                source_add_action::add_button(source_add_action::AddActionKind::Add, &station_name);
            let conn = self.conn.clone();
            let on_added = self.on_added.clone();
            add.connect_clicked(move |button| {
                let station = station_from_candidate(candidate.clone());
                let result = {
                    let conn = &conn;
                    radio::station::add_or_restore(conn, &station, now_unix())
                };
                match result {
                    Ok(_) => {
                        on_added();
                        // SRC-7: acknowledge in place instead of removing the
                        // row, so the add stays visible.
                        source_add_action::mark_added(
                            button,
                            source_add_action::AddActionKind::Add,
                            &station_name,
                        );
                    }
                    Err(error) => {
                        tracing::warn!(%error, "could not add radio search result");
                        button.set_tooltip_text(Some(&strings::text(strings::RADIO_ADD_FAILED)));
                    }
                }
            });
            content.append(tile.widget());
            content.append(&copy);
            content.append(&add);
            row.set_child(Some(&content));
            self.widgets.results.append(&row);
        }
    }

    fn render_preview(&self, preview: &StationPreview) {
        while let Some(child) = self.widgets.preview.first_child() {
            self.widgets.preview.remove(&child);
        }
        let favorites = radio::station::list(&self.conn).map_or_else(
            |error| {
                tracing::warn!(%error, "could not load radio favorites for preview filtering");
                Vec::new()
            },
            |rows| {
                rows.into_iter()
                    .map(|row| (row.uuid.unwrap_or_default(), row.stream_url))
                    .collect()
            },
        );
        if radio::search::station_is_known(preview.uuid.as_deref(), &preview.stream_url, &favorites)
        {
            self.widgets
                .status
                .set_text(&strings::text(strings::RADIO_ALREADY_FAVORITE));
            self.widgets.status.set_visible(true);
            self.widgets.confirm.set_sensitive(false);
            return;
        }
        let kind = gtk4::Label::new(Some(&strings::text(if preview.playlist_kind.is_some() {
            strings::RADIO_PLAYLIST_DETECTED
        } else {
            strings::RADIO_STREAM_DETECTED
        })));
        kind.set_xalign(0.0);
        let tile = crate::ui::podcasts::source_image::SourceImage::new(
            preview.favicon_url.as_deref(),
            "network-wireless-symbolic",
            40,
            images_allowed(&self.conn),
        );
        let name = gtk4::Label::new(Some(&preview.name));
        name.set_xalign(0.0);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.append(tile.widget());
        row.append(&name);
        self.widgets.preview.append(&kind);
        self.widgets.preview.append(&row);
        self.widgets.preview.set_visible(true);
    }

    fn confirm_preview(&self) {
        let preview = match &self.state.borrow().phase {
            AddDialogPhase::Preview(preview) => preview.clone(),
            _ => return,
        };
        let result = {
            let conn = &self.conn;
            radio::station::add_or_restore(conn, &preview.into_new_station(), now_unix())
        };
        match result {
            Ok(_) => {
                (self.on_added)();
                self.widgets.dialog.close();
            }
            Err(error) => {
                tracing::warn!(%error, "could not add radio preview");
                self.widgets
                    .status
                    .set_text(&strings::text(strings::RADIO_ADD_FAILED));
                self.widgets.status.add_css_class("error");
                self.widgets.status.set_visible(true);
            }
        }
    }
}

fn station_from_candidate(candidate: StationCandidate) -> radio::station::NewStation {
    radio::station::NewStation {
        uuid: Some(candidate.uuid),
        name: candidate.name,
        stream_url: candidate.url_resolved,
        homepage: None,
        favicon_url: candidate.favicon_url,
        genre: candidate.genre,
        codec: candidate.codec,
        bitrate_kbps: candidate.bitrate_kbps,
        country_code: candidate.country_code,
        votes: Some(candidate.votes),
    }
}

fn preview_url(url: &str, fetch_metadata: bool) -> Result<StationPreview, RadioError> {
    let kind = playlist_kind(url);
    let stream_url = match kind {
        Some(kind) => {
            let body = radio::http::get(url)?;
            if radio::playlist::is_hls_manifest(&body) {
                url.to_owned()
            } else {
                radio::playlist::resolve_playlist(&body, kind).ok_or_else(|| {
                    RadioError::Parse("playlist did not contain a playable stream URL".into())
                })?
            }
        }
        None => url.to_owned(),
    };
    let probe = radio::icy::probe(&stream_url)?;
    let mut preview = StationPreview::manual(
        probe
            .name
            .as_deref()
            .unwrap_or(strings::RADIO_STREAM_DETECTED),
        &stream_url,
    )
    .with_probe(probe);
    preview.playlist_kind = kind;
    if fetch_metadata {
        if let Some(candidate) = radio::search::find_by_url(&stream_url)? {
            preview = preview.with_candidate(candidate);
        }
    }
    Ok(preview)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
#[path = "add_dialog_tests.rs"]
mod tests;
