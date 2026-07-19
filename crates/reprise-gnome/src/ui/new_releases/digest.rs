//! Row-less New Releases digest reached only from the header popover.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::strings;

use super::release_cover::LazyReleaseCover;

const HERO_COVER_EDGE: i32 = 72;
const ROW_COVER_EDGE: i32 = 34;

struct Shared {
    conn: Rc<RefCell<rusqlite::Connection>>,
    rows: gtk4::Box,
    hidden_footer: gtk4::Box,
}

pub(in crate::ui) struct NewReleasesDigest {
    root: gtk4::ScrolledWindow,
    shared: Rc<Shared>,
}

impl NewReleasesDigest {
    pub(in crate::ui) fn new(conn: Rc<RefCell<rusqlite::Connection>>) -> Rc<Self> {
        let title = gtk4::Label::new(Some(&strings::text(strings::NEW_RELEASES)));
        title.add_css_class("title-1");
        title.set_xalign(0.0);

        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        let hidden_footer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_top(24);
        content.set_margin_bottom(96);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.append(&title);
        content.append(&rows);
        content.append(&hidden_footer);

        let clamp = adw::Clamp::builder()
            .maximum_size(720)
            .child(&content)
            .build();
        let root = gtk4::ScrolledWindow::builder()
            .child(&clamp)
            .hexpand(true)
            .vexpand(true)
            .build();
        let shared = Rc::new(Shared {
            conn,
            rows,
            hidden_footer,
        });
        let shared_on_map = shared.clone();
        root.connect_map(move |_| render(&shared_on_map));
        let digest = Rc::new(Self { root, shared });
        digest.refresh();
        digest
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    pub(in crate::ui) fn refresh(&self) {
        render(&self.shared);
    }
}

fn render(shared: &Rc<Shared>) {
    clear_box(&shared.rows);
    clear_box(&shared.hidden_footer);
    let today = chrono::Local::now().date_naive();
    let all_releases =
        reprise_core::artist_news::query_releases(&shared.conn.borrow(), true, today)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not query New Releases digest");
                Vec::new()
            });
    let releases = all_releases
        .iter()
        .filter(|release| !release.hidden)
        .cloned()
        .collect::<Vec<_>>();
    for (index, release) in releases.iter().enumerate() {
        shared
            .rows
            .append(&build_release_row(shared, release, index == 0));
    }

    let hidden = all_releases.iter().filter(|release| release.hidden).count();
    if hidden > 0 {
        let show = gtk4::Button::with_label(&strings::new_releases_hidden(hidden));
        show.add_css_class("flat");
        show.add_css_class("pill");
        show.set_halign(gtk4::Align::Center);
        let shared_on_show = shared.clone();
        show.connect_clicked(move |_| {
            if let Err(error) =
                reprise_core::artist_news::show_hidden_releases(&shared_on_show.conn.borrow())
            {
                tracing::warn!(%error, "could not restore hidden New Releases");
                return;
            }
            render(&shared_on_show);
        });
        shared.hidden_footer.append(&show);
    }
}

fn build_release_row(
    shared: &Rc<Shared>,
    release: &reprise_core::artist_news::StoredRelease,
    hero: bool,
) -> gtk4::Box {
    let cover = LazyReleaseCover::new(
        &release.release_group_mbid,
        &release.artist_name,
        &release.fallback_accent,
        if hero {
            HERO_COVER_EDGE
        } else {
            ROW_COVER_EDGE
        },
    );
    let title = gtk4::Label::new(Some(&release.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    if hero {
        title.add_css_class("title-3");
    }
    let meta = gtk4::Label::new(Some(&format!(
        "{} · {} · {}",
        release.artist_name, release.release_type, release.first_release_date
    )));
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    meta.add_css_class("dim-label");
    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk4::Align::Center);
    text.append(&title);
    text.append(&meta);

    let hide = gtk4::Button::with_label(&strings::text(strings::HIDE_RELEASE));
    hide.add_css_class("flat");
    hide.add_css_class("pill");
    hide.set_valign(gtk4::Align::Center);
    let shared = shared.clone();
    let mbid = release.release_group_mbid.clone();
    hide.connect_clicked(move |_| {
        if let Err(error) =
            reprise_core::artist_news::set_release_hidden(&shared.conn.borrow(), &mbid, true)
        {
            tracing::warn!(%error, release_group_mbid = mbid, "could not hide New Release");
            return;
        }
        render(&shared);
    });

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row.add_css_class("card");
    row.set_margin_top(2);
    row.set_margin_bottom(2);
    row.set_margin_start(2);
    row.set_margin_end(2);
    row.append(cover.widget());
    row.append(&text);
    row.append(&hide);
    row
}

fn clear_box(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
