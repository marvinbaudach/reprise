//! Lyrics presentation and active-line scrolling.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::lyrics::{LyricsBody, LyricsError};

use super::lyrics_strings;

pub(super) const ACTIVE_LINE_CLASS: &str = "lyrics-line-active";
const CONTENT_PAGE: &str = "content";
const LOADING_PAGE: &str = "loading";
const STATUS_PAGE: &str = "status";

type RetryCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
type StatusCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub(super) struct LyricsView {
    root: gtk4::Stack,
    content: gtk4::Box,
    scrolled: gtk4::ScrolledWindow,
    loading_track: gtk4::Label,
    status: gtk4::Label,
    retry: gtk4::Button,
    line_labels: RefCell<Vec<gtk4::Label>>,
    active_line: Cell<Option<usize>>,
    loading: Cell<bool>,
    can_retry: Cell<bool>,
    on_retry: RetryCallback,
    on_status_changed: StatusCallback,
}

impl LyricsView {
    pub(super) fn new() -> Rc<Self> {
        install_css();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.set_valign(gtk4::Align::Start);
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();

        let loading_spinner = gtk4::Spinner::new();
        loading_spinner.start();
        let loading_title =
            gtk4::Label::new(Some(&lyrics_strings::text(lyrics_strings::LOADING_LYRICS)));
        loading_title.add_css_class("heading");
        let loading_track = gtk4::Label::builder()
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .build();
        loading_track.add_css_class("dim-label");
        let loading = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        loading.set_halign(gtk4::Align::Center);
        loading.set_valign(gtk4::Align::Center);
        loading.append(&loading_spinner);
        loading.append(&loading_title);
        loading.append(&loading_track);

        let status = gtk4::Label::builder()
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .selectable(true)
            .build();
        let retry = gtk4::Button::with_label(&lyrics_strings::text(lyrics_strings::RETRY));
        retry.add_css_class("suggested-action");
        retry.set_halign(gtk4::Align::Center);
        let status_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        status_box.set_margin_start(24);
        status_box.set_margin_end(24);
        status_box.set_halign(gtk4::Align::Center);
        status_box.set_valign(gtk4::Align::Center);
        status_box.append(&status);
        status_box.append(&retry);

        let root = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        root.add_named(&scrolled, Some(CONTENT_PAGE));
        root.add_named(&loading, Some(LOADING_PAGE));
        root.add_named(&status_box, Some(STATUS_PAGE));

        let on_retry: RetryCallback = Rc::new(RefCell::new(None));
        let retry_callback = on_retry.clone();
        retry.connect_clicked(move |_| {
            let callback = retry_callback.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });

        let view = Rc::new(Self {
            root,
            content,
            scrolled,
            loading_track,
            status,
            retry,
            line_labels: RefCell::new(Vec::new()),
            active_line: Cell::new(None),
            loading: Cell::new(false),
            can_retry: Cell::new(false),
            on_retry,
            on_status_changed: Rc::new(RefCell::new(None)),
        });
        view.show_empty();
        view
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn show_empty(&self) {
        self.clear_lines();
        self.show_status(
            &lyrics_strings::text(lyrics_strings::PLAY_TO_SEE_LYRICS),
            false,
        );
    }

    pub(super) fn show_loading(&self, title: &str, artist: &str) {
        self.clear_lines();
        self.loading_track
            .set_text(&format!("{}\n{}", title.trim(), artist.trim()));
        self.root.set_visible_child_name(LOADING_PAGE);
        self.set_feedback(true, false);
    }

    pub(super) fn show_result(&self, body: &LyricsBody) {
        self.clear_lines();
        match body {
            LyricsBody::Synced(lines) => {
                for line in lines {
                    self.append_line(&line.text, true);
                }
                self.root.set_visible_child_name(CONTENT_PAGE);
                self.set_feedback(false, false);
            }
            LyricsBody::Plain(text) => {
                self.append_line(text, false);
                self.root.set_visible_child_name(CONTENT_PAGE);
                self.set_feedback(false, false);
            }
            LyricsBody::Instrumental => {
                self.show_status(&lyrics_strings::text(lyrics_strings::INSTRUMENTAL), false);
            }
        }
    }

    pub(super) fn show_error(&self, error: &LyricsError) {
        self.clear_lines();
        let (message, retry) = match error {
            LyricsError::NotFound | LyricsError::MissingMetadata => {
                (lyrics_strings::NO_LYRICS_FOUND, false)
            }
            LyricsError::Temporary | LyricsError::InvalidResponse => {
                (lyrics_strings::LYRICS_UNAVAILABLE, true)
            }
        };
        self.show_status(&lyrics_strings::text(message), retry);
    }

    pub(super) fn set_active_line(&self, index: Option<usize>) {
        if index == self.active_line.get() {
            return;
        }
        if let Some(previous) = self.active_line.replace(index) {
            let previous = self.line_labels.borrow().get(previous).cloned();
            if let Some(label) = previous {
                label.remove_css_class(ACTIVE_LINE_CLASS);
            }
        }
        let Some(index) = index else {
            return;
        };
        let label = self.line_labels.borrow().get(index).cloned();
        let Some(label) = label else {
            self.active_line.set(None);
            return;
        };
        label.add_css_class(ACTIVE_LINE_CLASS);
        self.scroll_to_label(&label);
    }

    pub(super) fn set_on_retry(&self, callback: impl Fn() + 'static) {
        *self.on_retry.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn retry(&self) {
        let callback = self.on_retry.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    pub(super) fn set_on_status_changed(&self, callback: impl Fn() + 'static) {
        *self.on_status_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn is_loading(&self) -> bool {
        self.loading.get()
    }

    pub(super) fn can_retry(&self) -> bool {
        self.can_retry.get()
    }

    fn append_line(&self, text: &str, timed: bool) {
        let label = gtk4::Label::builder()
            .label(text)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::WordChar)
            .selectable(true)
            .build();
        label.set_margin_top(if timed { 8 } else { 0 });
        label.set_margin_bottom(if timed { 8 } else { 0 });
        self.content.append(&label);
        self.line_labels.borrow_mut().push(label);
    }

    fn clear_lines(&self) {
        self.active_line.set(None);
        self.line_labels.borrow_mut().clear();
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
    }

    fn show_status(&self, message: &str, retry: bool) {
        self.status.set_text(message);
        self.retry.set_visible(retry);
        self.root.set_visible_child_name(STATUS_PAGE);
        self.set_feedback(false, retry);
    }

    fn set_feedback(&self, loading: bool, can_retry: bool) {
        let loading_changed = self.loading.replace(loading) != loading;
        let retry_changed = self.can_retry.replace(can_retry) != can_retry;
        if !loading_changed && !retry_changed {
            return;
        }
        let callback = self.on_status_changed.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    fn scroll_to_label(&self, label: &gtk4::Label) {
        let label = label.clone();
        let content = self.content.clone();
        let adjustment = self.scrolled.vadjustment();
        gtk4::glib::idle_add_local_once(move || {
            let Some(point) = label.compute_point(&content, &gtk4::graphene::Point::new(0.0, 0.0))
            else {
                return;
            };
            let value = centered_scroll_value(
                f64::from(point.y()),
                f64::from(label.height()),
                adjustment.page_size(),
                adjustment.upper(),
            );
            adjustment.set_value(value);
        });
    }

    #[cfg(test)]
    pub(super) fn line_labels(&self) -> Vec<gtk4::Label> {
        self.line_labels.borrow().clone()
    }

    #[cfg(test)]
    pub(super) fn visible_state_name(&self) -> Option<gtk4::glib::GString> {
        self.root.visible_child_name()
    }

    #[cfg(test)]
    pub(super) fn status_text(&self) -> String {
        self.status.text().to_string()
    }

    #[cfg(test)]
    pub(super) fn retry_is_visible(&self) -> bool {
        self.retry.is_visible()
    }

    #[cfg(test)]
    pub(super) fn scroll_values(&self) -> (f64, f64) {
        let adjustment = self.scrolled.vadjustment();
        (
            adjustment.value(),
            (adjustment.upper() - adjustment.page_size()).max(0.0),
        )
    }
}

pub(super) fn centered_scroll_value(
    row_y: f64,
    row_height: f64,
    page_size: f64,
    upper: f64,
) -> f64 {
    if !row_y.is_finite() || !row_height.is_finite() || !page_size.is_finite() || !upper.is_finite()
    {
        return 0.0;
    }
    let maximum = (upper - page_size).max(0.0);
    (row_y + row_height / 2.0 - page_size / 2.0).clamp(0.0, maximum)
}

fn install_css() {
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&format!(
        ".{ACTIVE_LINE_CLASS} {{ color: @accent_color; font-weight: 700; }}"
    ));
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
