use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::radio::icy::IcyProbe;
use reprise_core::radio::playlist::PlaylistKind;
use reprise_core::radio::search::StationCandidate;
use reprise_core::radio::{self, RadioError};
use rusqlite::Connection;

use crate::ui::{one_shot_task, strings};

type AddedCallback = Rc<dyn Fn()>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddInput {
    Empty,
    Search(String),
    Url(String),
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
pub(super) struct StationPreview {
    pub name: String,
    pub stream_url: String,
    pub uuid: Option<String>,
    pub favicon_url: Option<String>,
    pub genre: Option<String>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub country_code: Option<String>,
    pub votes: Option<i64>,
    pub playlist_kind: Option<PlaylistKind>,
}

impl StationPreview {
    pub(super) fn manual(name: &str, stream_url: &str) -> Self {
        Self {
            name: name.into(),
            stream_url: stream_url.into(),
            uuid: None,
            favicon_url: None,
            genre: None,
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            votes: None,
            playlist_kind: playlist_kind(stream_url),
        }
    }

    fn with_probe(mut self, probe: IcyProbe) -> Self {
        if let Some(name) = probe.name {
            self.name = name;
        }
        self.genre = probe.genre;
        self.codec = probe.content_type;
        self.bitrate_kbps = probe.bitrate_kbps;
        self
    }

    fn with_candidate(mut self, candidate: StationCandidate) -> Self {
        self.uuid = Some(candidate.uuid);
        self.name = candidate.name;
        self.favicon_url = candidate.favicon_url;
        self.genre = candidate.genre;
        self.codec = candidate.codec;
        self.bitrate_kbps = candidate.bitrate_kbps;
        self.country_code = candidate.country_code;
        self.votes = Some(candidate.votes);
        self
    }

    fn into_new_station(self) -> radio::station::NewStation {
        radio::station::NewStation {
            uuid: self.uuid,
            name: self.name,
            stream_url: self.stream_url,
            homepage: None,
            favicon_url: self.favicon_url,
            genre: self.genre,
            codec: self.codec,
            bitrate_kbps: self.bitrate_kbps,
            country_code: self.country_code,
            votes: self.votes,
        }
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
    conn: Rc<RefCell<Connection>>,
    on_added: AddedCallback,
}

impl RadioAddDialog {
    pub(super) fn new(conn: Rc<RefCell<Connection>>, on_added: impl Fn() + 'static) -> Rc<Self> {
        let entry = gtk4::SearchEntry::builder()
            .placeholder_text(strings::text(strings::RADIO_DIALOG_HINT))
            .build();
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

        let footnote = gtk4::Label::new(Some(&strings::text(strings::RADIO_COMMUNITY_FOOTNOTE)));
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
        content.append(&spinner);
        content.append(&status);
        content.append(&results);
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
        this
    }

    pub(super) fn present(self: &Rc<Self>, parent: &impl IsA<gtk4::Widget>) {
        self.widgets.entry.set_text("");
        self.render(AddDialogState::default());
        self.widgets.dialog.present(Some(parent));
        self.widgets.entry.grab_focus();
    }

    fn submit(self: &Rc<Self>, input: &str) {
        let input = classify_input(input);
        if input == AddInput::Empty {
            self.render(AddDialogState::default());
            return;
        }
        let (state, generation) = self.state.borrow().clone().begin(&input);
        self.render(state);
        let result = match input {
            AddInput::Search(terms) => {
                let order = radio::config::load(&self.conn.borrow())
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
            let tile = gtk4::Image::from_icon_name("network-wireless-symbolic");
            tile.set_pixel_size(40);
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
            let add = gtk4::Button::with_label(&strings::text(strings::RADIO_ADD_RESULT));
            let conn = self.conn.clone();
            let on_added = self.on_added.clone();
            add.connect_clicked(move |button| {
                let station = station_from_candidate(candidate.clone());
                match radio::station::add_or_restore(&conn.borrow(), &station, now_unix()) {
                    Ok(_) => {
                        button.set_icon_name("object-select-symbolic");
                        button.set_sensitive(false);
                        on_added();
                    }
                    Err(error) => {
                        tracing::warn!(%error, "could not add radio search result");
                        button.set_label(&strings::text(strings::RADIO_ADD_FAILED));
                    }
                }
            });
            content.append(&tile);
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
        let kind = gtk4::Label::new(Some(&strings::text(if preview.playlist_kind.is_some() {
            strings::RADIO_PLAYLIST_DETECTED
        } else {
            strings::RADIO_STREAM_DETECTED
        })));
        kind.set_xalign(0.0);
        let tile = gtk4::Image::from_icon_name("network-wireless-symbolic");
        tile.set_pixel_size(40);
        let name = gtk4::Label::new(Some(&preview.name));
        name.set_xalign(0.0);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.append(&tile);
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
        match radio::station::add_or_restore(
            &self.conn.borrow(),
            &preview.into_new_station(),
            now_unix(),
        ) {
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
mod tests {
    use super::*;

    #[test]
    fn src_3_radio_add_dialog_submits_search_or_url_through_one_field() {
        assert_eq!(
            classify_input("ambient radio"),
            AddInput::Search("ambient radio".into())
        );
        assert_eq!(
            classify_input("https://radio.example/listen.pls"),
            AddInput::Url("https://radio.example/listen.pls".into())
        );
        assert_eq!(classify_input("   "), AddInput::Empty);
    }

    #[test]
    fn dialog_state_ignores_stale_results_and_requires_a_valid_preview() {
        let state = AddDialogState::default();
        let (state, first) = state.begin(&AddInput::Search("metal".into()));
        let (state, second) = state.begin(&AddInput::Url("https://radio.example/live".into()));
        assert!(matches!(state.phase, AddDialogPhase::Previewing));
        assert_eq!(
            state.clone().accept(first, AddResult::Search(Vec::new())),
            state
        );
        let preview = StationPreview::manual("Example", "https://radio.example/live");
        let accepted = state.accept(second, AddResult::Preview(preview));
        assert!(matches!(accepted.phase, AddDialogPhase::Preview(_)));
        assert!(accepted.can_confirm());
    }

    #[test]
    fn rad_4_playlist_type_is_detected_without_consuming_a_live_stream() {
        assert_eq!(
            playlist_kind("https://radio.example/listen.PLS?token=1"),
            Some(reprise_core::radio::playlist::PlaylistKind::Pls)
        );
        assert_eq!(
            playlist_kind("https://radio.example/listen.m3u8"),
            Some(reprise_core::radio::playlist::PlaylistKind::M3u)
        );
        assert_eq!(playlist_kind("https://radio.example/live"), None);
    }
}
