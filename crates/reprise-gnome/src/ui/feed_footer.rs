//! Shared live-state footer for network-backed feeds.

use chrono::{DateTime, Datelike, Local, Timelike};
use gtk4::prelude::*;

use crate::ui::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum FeedFooterState {
    Loaded { at: i64 },
    Cached { at: i64 },
    Fetching { checked: usize, total: usize },
    Failed { latest: i64 },
    Offline { latest: i64 },
    NeverFetched,
    NoCredentials,
    NetworkOff,
    ModuleOff,
}

#[derive(Clone, Copy)]
pub(in crate::ui) struct FeedFooterCopy {
    pub updating: &'static str,
    pub no_credentials: &'static str,
    pub failed: fn(&str) -> String,
    pub offline: fn(&str) -> String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum FeedFooterDot {
    Live,
    Dimmed,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum FeedFooterProgress {
    Indeterminate,
    Fraction(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct FeedFooterPresentation {
    pub text: String,
    pub dot: FeedFooterDot,
    pub progress: Option<FeedFooterProgress>,
    pub reload_visible: bool,
    pub visible: bool,
}

pub(in crate::ui) fn presentation(
    state: FeedFooterState,
    now: DateTime<Local>,
) -> FeedFooterPresentation {
    presentation_with_copy(state, now, strings::concerts_feed_footer_copy())
}

pub(in crate::ui) fn presentation_with_copy(
    state: FeedFooterState,
    now: DateTime<Local>,
    copy: FeedFooterCopy,
) -> FeedFooterPresentation {
    let stable = |text: String| FeedFooterPresentation {
        text,
        dot: FeedFooterDot::Live,
        progress: None,
        reload_visible: true,
        visible: true,
    };
    let unavailable = |text: String, reload_visible| FeedFooterPresentation {
        text,
        dot: FeedFooterDot::Dimmed,
        progress: None,
        reload_visible,
        visible: true,
    };

    match state {
        FeedFooterState::Loaded { at } => stable(strings::feed_loaded_at(&format_time(at, now))),
        FeedFooterState::Cached { at } => stable(strings::feed_checked_at(&format_time(at, now))),
        FeedFooterState::Fetching { checked, total } => FeedFooterPresentation {
            text: strings::text(copy.updating),
            dot: FeedFooterDot::Dimmed,
            progress: Some(if total == 0 {
                FeedFooterProgress::Indeterminate
            } else {
                FeedFooterProgress::Fraction((checked.min(total) as f64) / (total as f64))
            }),
            reload_visible: false,
            visible: true,
        },
        FeedFooterState::Failed { latest } => {
            unavailable((copy.failed)(&format_time(latest, now)), true)
        }
        FeedFooterState::Offline { latest } => {
            unavailable((copy.offline)(&format_time(latest, now)), true)
        }
        FeedFooterState::NeverFetched => unavailable(strings::text(strings::FEED_NOT_LOADED), true),
        FeedFooterState::NoCredentials => unavailable(strings::text(copy.no_credentials), false),
        FeedFooterState::NetworkOff => unavailable(strings::text(strings::FEED_NETWORK_OFF), false),
        FeedFooterState::ModuleOff => FeedFooterPresentation {
            text: String::new(),
            dot: FeedFooterDot::Hidden,
            progress: None,
            reload_visible: false,
            visible: false,
        },
    }
}

fn format_time(timestamp: i64, now: DateTime<Local>) -> String {
    let Some(value) =
        DateTime::from_timestamp(timestamp, 0).map(|value| value.with_timezone(&Local))
    else {
        return crate::ui::date_format::current()
            .date
            .render(Some(1970), Some(1), Some(1));
    };
    if value.date_naive() == now.date_naive() {
        crate::ui::date_format::current()
            .clock
            .render(i64::from(value.hour()), i64::from(value.minute()))
    } else {
        crate::ui::date_format::current().date.render(
            Some(value.year()),
            Some(value.month()),
            Some(value.day()),
        )
    }
}

pub(in crate::ui) struct FeedFooter {
    root: gtk4::Box,
    dot: gtk4::Box,
    label: gtk4::Label,
    reload: gtk4::Button,
    progress: gtk4::ProgressBar,
}

impl FeedFooter {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-feed-footer");
        root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.add_css_class("reprise-feed-footer-row");
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("reprise-feed-footer-dot");
        dot.set_size_request(6, 6);
        dot.set_valign(gtk4::Align::Center);
        row.append(&dot);
        let label = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        label.add_css_class("caption");
        row.append(&label);

        let reload = gtk4::Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text(strings::text(strings::FEED_RELOAD))
            .build();
        reload.add_css_class("flat");
        reload.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::FEED_RELOAD,
        ))]);
        row.append(&reload);

        let progress = gtk4::ProgressBar::new();
        progress.set_valign(gtk4::Align::Center);
        progress.set_size_request(96, -1);
        row.append(&progress);
        root.append(&row);

        Self {
            root,
            dot,
            label,
            reload,
            progress,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(in crate::ui) fn connect_reload(&self, callback: impl Fn() + 'static) {
        self.reload.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn apply(&self, state: FeedFooterState) {
        self.apply_presentation(&presentation(state, Local::now()));
    }

    #[cfg(test)]
    pub(in crate::ui) fn text(&self) -> String {
        self.label.text().to_string()
    }

    #[cfg(test)]
    pub(in crate::ui) fn reload_is_visible(&self) -> bool {
        self.reload.is_visible()
    }

    #[cfg(test)]
    pub(in crate::ui) fn progress_is_visible(&self) -> bool {
        self.progress.is_visible()
    }

    fn apply_presentation(&self, value: &FeedFooterPresentation) {
        self.root.set_visible(value.visible);
        self.label.set_label(&value.text);
        self.dot.remove_css_class("live");
        if value.dot == FeedFooterDot::Live {
            self.dot.add_css_class("live");
        }
        self.reload.set_visible(value.reload_visible);
        self.progress.set_visible(value.progress.is_some());
        match value.progress {
            Some(FeedFooterProgress::Fraction(fraction)) => {
                self.progress.set_fraction(fraction);
            }
            Some(FeedFooterProgress::Indeterminate) => self.progress.pulse(),
            None => {}
        }
    }
}

pub(in crate::ui) fn css() -> String {
    ".reprise-feed-footer-row { padding: 6px 12px; }\n\
     .reprise-feed-footer-dot {\
       min-width: 6px; min-height: 6px;\
       border-radius: 999px;\
       background-color: @reprise_hint_fg_color;\
     }\n\
     .reprise-feed-footer-dot.live { background-color: @accent_bg_color; }"
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 8, 14, 15, 0, 0).unwrap()
    }

    #[test]
    fn conc_15_the_footer_never_claims_up_to_date_while_fetching() {
        let at = now().timestamp() - 28 * 60;
        let cases = [
            (FeedFooterState::Loaded { at }, true, true, false),
            (FeedFooterState::Cached { at }, true, true, false),
            (
                FeedFooterState::Fetching {
                    checked: 3,
                    total: 12,
                },
                false,
                false,
                true,
            ),
            (FeedFooterState::Failed { latest: at }, false, true, false),
            (FeedFooterState::Offline { latest: at }, false, true, false),
            (FeedFooterState::NeverFetched, false, true, false),
            (FeedFooterState::NoCredentials, false, false, false),
            (FeedFooterState::NetworkOff, false, false, false),
            (FeedFooterState::ModuleOff, false, false, false),
        ];
        for (state, up_to_date, reload, progress) in cases {
            let result = presentation(state, now());
            assert_eq!(result.text.starts_with("Up to date"), up_to_date);
            assert_eq!(result.reload_visible, reload);
            assert_eq!(result.progress.is_some(), progress);
        }
        assert!(!presentation(FeedFooterState::ModuleOff, now()).visible);
        assert_eq!(
            presentation(
                FeedFooterState::Fetching {
                    checked: 1,
                    total: 4,
                },
                now(),
            )
            .progress,
            Some(FeedFooterProgress::Fraction(0.25))
        );
        assert_eq!(
            presentation(
                FeedFooterState::Fetching {
                    checked: 0,
                    total: 0,
                },
                now(),
            )
            .progress,
            Some(FeedFooterProgress::Indeterminate)
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn feed_footer_live_dot_stays_six_pixels_square() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&css());
        let footer = FeedFooter::new();
        footer.apply(FeedFooterState::Loaded {
            at: chrono::Utc::now().timestamp(),
        });
        let window = gtk4::Window::builder()
            .default_width(500)
            .child(footer.widget())
            .build();
        window.present();
        crate::ui::source_context_surface::settle_layout();

        assert_eq!(footer.dot.width(), 6);
        assert_eq!(footer.dot.height(), 6);
    }
}
