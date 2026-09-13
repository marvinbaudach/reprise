use std::time::{Duration, Instant};

use gtk4::prelude::*;

#[derive(Clone, Copy)]
pub(crate) enum LabelSettle {
    UntilText,
    ObserveFor(Duration),
}

pub(crate) fn rendered_label_texts(
    content: &gtk4::Widget,
    default_width: i32,
    default_height: i32,
    settle: LabelSettle,
    after_present: impl FnOnce(),
) -> (Vec<String>, Duration) {
    fn collect(widget: &gtk4::Widget, labels: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            let text = label.text();
            if !text.is_empty() {
                labels.push(text.to_string());
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, labels);
            child = current.next_sibling();
        }
    }

    let window = libadwaita::Window::builder()
        .default_width(default_width)
        .default_height(default_height)
        .content(content)
        .build();
    window.present();
    after_present();

    let started = Instant::now();
    match settle {
        LabelSettle::UntilText => {
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || {
                    let mut labels = Vec::new();
                    collect(content, &mut labels);
                    !labels.is_empty()
                },
            );
        }
        LabelSettle::ObserveFor(duration) => {
            crate::ui::test_settle::settle_for(duration);
        }
    }
    let elapsed = started.elapsed();

    let mut labels = Vec::new();
    collect(content, &mut labels);
    window.close();
    (labels, elapsed)
}
