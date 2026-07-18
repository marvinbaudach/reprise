//! Lyrics presentation and active-line scrolling.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::lyrics::{LyricsBody, LyricsError};

use super::lyrics_strings;

pub(in crate::ui) const ACTIVE_LINE_CLASS: &str = "lyrics-line-active";
pub(in crate::ui) const INLINE_RETRY_CLASS: &str = "lyrics-inline-retry";
const LINE_CLASS: &str = "lyrics-line";
const LINE_NEIGHBOR_CLASS: &str = "lyrics-line-neighbor";
const LINE_NEAR_CLASS: &str = "lyrics-line-near";
const LINE_DISTANT_CLASS: &str = "lyrics-line-distant";
const LINE_GAP_CLASS: &str = "lyrics-line-gap";
const LINE_UNDERLINE_CLASS: &str = "lyrics-line-underline";
const UNSYNCED_CLASS: &str = "lyrics-unsynced";
const CONTENT_PAGE: &str = "content";
const LOADING_PAGE: &str = "loading";
const STATUS_PAGE: &str = "status";

type RetryCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
type StatusCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
type FooterCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Clone)]
struct LyricsLine {
    root: gtk4::Box,
    label: gtk4::Label,
    underline: gtk4::Box,
    timed: bool,
}

pub(in crate::ui) struct LyricsView {
    root: gtk4::Stack,
    content: gtk4::Box,
    scrolled: gtk4::ScrolledWindow,
    loading_track: gtk4::Label,
    status: gtk4::Label,
    retry: gtk4::Button,
    lines: RefCell<Vec<LyricsLine>>,
    active_line: Cell<Option<usize>>,
    active_alpha: Cell<u8>,
    footer_text: RefCell<String>,
    loading: Cell<bool>,
    can_retry: Cell<bool>,
    on_retry: RetryCallback,
    on_status_changed: StatusCallback,
    on_footer_changed: FooterCallback,
}

impl LyricsView {
    pub(in crate::ui) fn new() -> Rc<Self> {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 13);
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
            .selectable(false)
            .build();
        let retry = gtk4::Button::with_label(&lyrics_strings::text(lyrics_strings::RETRY));
        retry.add_css_class(INLINE_RETRY_CLASS);
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
            .transition_duration(crate::ui::motion::STANDARD_MS)
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
            lines: RefCell::new(Vec::new()),
            active_line: Cell::new(None),
            active_alpha: Cell::new(100),
            footer_text: RefCell::new(String::new()),
            loading: Cell::new(false),
            can_retry: Cell::new(false),
            on_retry,
            on_status_changed: Rc::new(RefCell::new(None)),
            on_footer_changed: Rc::new(RefCell::new(None)),
        });
        view.show_empty();
        view
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(in crate::ui) fn show_empty(&self) {
        self.clear_lines();
        self.show_status(
            &lyrics_strings::text(lyrics_strings::PLAY_TO_SEE_LYRICS),
            false,
        );
    }

    pub(in crate::ui) fn show_loading(&self, title: &str, artist: &str) {
        self.clear_lines();
        self.set_footer("");
        self.loading_track
            .set_text(&format!("{}\n{}", title.trim(), artist.trim()));
        self.root.set_visible_child_name(LOADING_PAGE);
        self.set_feedback(true, false);
    }

    pub(in crate::ui) fn show_result(&self, body: &LyricsBody) {
        self.clear_lines();
        self.set_footer(&lyrics_strings::text(lyrics_footer(body)));
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

    pub(in crate::ui) fn show_error(&self, error: &LyricsError) {
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

    #[cfg(test)]
    pub(in crate::ui) fn set_active_line(&self, index: Option<usize>) {
        self.set_active_line_at(index, None, 0);
    }

    pub(in crate::ui) fn set_active_line_at(
        &self,
        index: Option<usize>,
        timestamp_ms: Option<i64>,
        position_ms: i64,
    ) {
        let index = index.filter(|index| {
            self.lines
                .borrow()
                .get(*index)
                .is_some_and(|line| line.timed)
        });
        let alpha = match (index, timestamp_ms) {
            (Some(_), Some(timestamp)) => active_line_alpha(timestamp, position_ms),
            _ => 100,
        };
        let line_changed = index != self.active_line.get();
        if !line_changed && alpha == self.active_alpha.get() {
            return;
        }
        self.active_line.set(index);
        self.active_alpha.set(alpha);
        self.apply_line_hierarchy();
        if !line_changed {
            return;
        }
        let label = index.and_then(|index| {
            self.lines
                .borrow()
                .get(index)
                .map(|line| line.label.clone())
        });
        if let Some(label) = label {
            self.scroll_to_label(&label);
        }
    }

    pub(in crate::ui) fn set_on_retry(&self, callback: impl Fn() + 'static) {
        *self.on_retry.borrow_mut() = Some(Rc::new(callback));
    }

    #[allow(dead_code)]
    pub(in crate::ui) fn retry(&self) {
        let callback = self.on_retry.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    #[allow(dead_code)]
    pub(in crate::ui) fn set_on_status_changed(&self, callback: impl Fn() + 'static) {
        *self.on_status_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_footer_changed(&self, callback: impl Fn() + 'static) {
        *self.on_footer_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn footer_text(&self) -> String {
        self.footer_text.borrow().clone()
    }

    #[allow(dead_code)]
    pub(in crate::ui) fn is_loading(&self) -> bool {
        self.loading.get()
    }

    #[allow(dead_code)]
    pub(in crate::ui) fn can_retry(&self) -> bool {
        self.can_retry.get()
    }

    pub(in crate::ui) fn smoke_snapshot(
        &self,
        expected: &str,
        rejected: &str,
    ) -> (usize, Option<usize>, bool) {
        let lines = self.lines.borrow();
        let latest = lines
            .iter()
            .any(|line| line.label.text().contains(expected))
            && lines
                .iter()
                .all(|line| !line.label.text().contains(rejected));
        (lines.len(), self.active_line.get(), latest)
    }

    fn append_line(&self, text: &str, timed: bool) {
        let label = gtk4::Label::builder()
            .label(text)
            .xalign(0.5)
            .justify(gtk4::Justification::Center)
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::WordChar)
            .selectable(false)
            .build();
        let underline = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        underline.add_css_class(LINE_UNDERLINE_CLASS);
        underline.set_halign(gtk4::Align::Center);
        underline.set_visible(false);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        root.add_css_class(LINE_CLASS);
        root.set_halign(gtk4::Align::Fill);
        root.append(&label);
        root.append(&underline);
        if timed {
            root.add_css_class(LINE_DISTANT_CLASS);
        } else {
            root.add_css_class(UNSYNCED_CLASS);
        }
        self.content.append(&root);
        self.lines.borrow_mut().push(LyricsLine {
            root,
            label,
            underline,
            timed,
        });
    }

    fn clear_lines(&self) {
        self.active_line.set(None);
        self.active_alpha.set(100);
        self.lines.borrow_mut().clear();
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
    }

    fn show_status(&self, message: &str, retry: bool) {
        self.set_footer("");
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

    fn set_footer(&self, text: &str) {
        if self.footer_text.borrow().as_str() == text {
            return;
        }
        *self.footer_text.borrow_mut() = text.to_owned();
        let callback = self.on_footer_changed.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    fn apply_line_hierarchy(&self) {
        let active = self.active_line.get();
        let active_alpha = self.active_alpha.get();
        for (index, line) in self.lines.borrow().iter().enumerate() {
            if !line.timed {
                continue;
            }
            for class in [
                ACTIVE_LINE_CLASS,
                LINE_NEIGHBOR_CLASS,
                LINE_NEAR_CLASS,
                LINE_DISTANT_CLASS,
                LINE_GAP_CLASS,
            ] {
                line.root.remove_css_class(class);
                line.label.remove_css_class(class);
            }
            let class = match line_alpha(active, index) {
                100 => ACTIVE_LINE_CLASS,
                45 => LINE_NEIGHBOR_CLASS,
                32 => LINE_NEAR_CLASS,
                _ => LINE_DISTANT_CLASS,
            };
            line.root.add_css_class(class);
            let is_active = active == Some(index);
            if is_active {
                line.label.add_css_class(ACTIVE_LINE_CLASS);
            }
            line.underline.set_visible(is_active);
            if is_active && active_alpha == 60 {
                line.root.add_css_class(LINE_GAP_CLASS);
            }
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
    pub(in crate::ui) fn line_labels(&self) -> Vec<gtk4::Label> {
        self.lines
            .borrow()
            .iter()
            .map(|line| line.label.clone())
            .collect()
    }

    #[cfg(test)]
    pub(in crate::ui) fn visible_state_name(&self) -> Option<gtk4::glib::GString> {
        self.root.visible_child_name()
    }

    #[cfg(test)]
    pub(in crate::ui) fn status_text(&self) -> String {
        self.status.text().to_string()
    }

    #[cfg(test)]
    pub(in crate::ui) fn retry_is_visible(&self) -> bool {
        self.retry.is_visible()
    }

    #[cfg(test)]
    pub(in crate::ui) fn retry_has_css_class(&self, class: &str) -> bool {
        self.retry.has_css_class(class)
    }

    #[cfg(test)]
    pub(in crate::ui) fn scroll_values(&self) -> (f64, f64) {
        let adjustment = self.scrolled.vadjustment();
        (
            adjustment.value(),
            (adjustment.upper() - adjustment.page_size()).max(0.0),
        )
    }
}

pub(in crate::ui) fn line_alpha(active: Option<usize>, index: usize) -> u8 {
    let Some(active) = active else {
        return 28;
    };
    match active.abs_diff(index) {
        0 => 100,
        1 => 45,
        2 => 32,
        _ => 28,
    }
}

pub(in crate::ui) fn active_line_alpha(timestamp_ms: i64, position_ms: i64) -> u8 {
    if position_ms.saturating_sub(timestamp_ms) > 10_000 {
        60
    } else {
        100
    }
}

pub(in crate::ui) fn lyrics_footer(body: &LyricsBody) -> &'static str {
    match body {
        LyricsBody::Synced(_) => lyrics_strings::SYNCED_LRCLIB,
        LyricsBody::Plain(_) => lyrics_strings::LYRICS_TAGS,
        LyricsBody::Instrumental => "",
    }
}

pub(in crate::ui) fn centered_scroll_value(
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

/// Active synchronized-line emphasis; installed app-wide by [`super::style`].
pub(in crate::ui) fn css() -> String {
    format!(
        ".{LINE_CLASS} {{ font-size: 13px; color: #ffffff; }}\n\
         .{LINE_DISTANT_CLASS} {{ opacity: 0.28; }}\n\
         .{LINE_NEAR_CLASS} {{ opacity: 0.32; }}\n\
         .{LINE_NEIGHBOR_CLASS} {{ opacity: 0.45; }}\n\
         .{ACTIVE_LINE_CLASS} {{ opacity: 1; }}\n\
         .{ACTIVE_LINE_CLASS} label {{ font-size: 15px; font-weight: 700; color: #ffffff; }}\n\
         .{LINE_GAP_CLASS} {{ opacity: 0.60; }}\n\
         .{LINE_UNDERLINE_CLASS} {{ min-width: 26px; min-height: 2.5px; \
           background-color: @reprise_player_accent; }}\n\
         .{UNSYNCED_CLASS} {{ font-size: 13px; color: alpha(#ffffff, 0.65); }}\n\
         .{INLINE_RETRY_CLASS} {{ padding: 4px 10px; min-height: 0; }}"
    )
}

#[cfg(test)]
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_1_lyrics_pages_use_the_standard_motion_token() {
    gtk4::init().unwrap();

    let lyrics = LyricsView::new();

    assert_eq!(
        lyrics.root.transition_duration(),
        crate::ui::motion::STANDARD_MS
    );
    assert_eq!(
        lyrics.root.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
}
