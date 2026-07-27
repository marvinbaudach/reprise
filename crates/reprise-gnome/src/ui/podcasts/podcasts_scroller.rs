//! Shrinkable episode scroller that preserves the structural player bar.

pub(super) fn build_episode_scroller(content: &gtk4::Widget) -> gtk4::ScrolledWindow {
    gtk4::ScrolledWindow::builder()
        .child(content)
        .hexpand(true)
        .vexpand(true)
        .propagate_natural_height(false)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn play_7b_podcast_episode_table_scrolls_inside_the_player_bar_boundary() {
        gtk4::init().unwrap();
        let table = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let scroller = build_episode_scroller(table.upcast_ref());

        assert_eq!(
            scroller.child(),
            Some(table.clone().upcast::<gtk4::Widget>())
        );
        assert!(scroller.vexpands());
        assert!(!scroller.propagates_natural_height());
    }
}
