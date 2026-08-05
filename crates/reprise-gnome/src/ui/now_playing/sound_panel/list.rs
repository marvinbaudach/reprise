use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::sound_neighbours::SoundNeighbour;

use crate::ui::cover_loader::CoverLoader;

type IdCallback = Rc<dyn Fn(i64)>;
type AlbumCallback = Rc<dyn Fn(i64, &str, &str)>;

#[derive(Default)]
pub(super) struct RowCallbacks {
    play: RefCell<Option<IdCallback>>,
    play_next: RefCell<Option<IdCallback>>,
    add_to_queue: RefCell<Option<IdCallback>>,
    open_album: RefCell<Option<AlbumCallback>>,
}

impl RowCallbacks {
    pub(super) fn set_play(&self, callback: impl Fn(i64) + 'static) {
        *self.play.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_play_next(&self, callback: impl Fn(i64) + 'static) {
        *self.play_next.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_add_to_queue(&self, callback: impl Fn(i64) + 'static) {
        *self.add_to_queue.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_open_album(&self, callback: impl Fn(i64, &str, &str) + 'static) {
        *self.open_album.borrow_mut() = Some(Rc::new(callback));
    }
}

pub(super) struct MatchList {
    root: gtk4::Box,
    cover_loader: Rc<CoverLoader>,
    callbacks: Rc<RowCallbacks>,
}

impl MatchList {
    pub(super) fn new(cover_loader: Rc<CoverLoader>) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        root.add_css_class("reprise-sound-matches");
        Self {
            root,
            cover_loader,
            callbacks: Rc::new(RowCallbacks::default()),
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn callbacks(&self) -> Rc<RowCallbacks> {
        self.callbacks.clone()
    }

    pub(super) fn render(&self, matches: &[SoundNeighbour]) {
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        for neighbour in matches {
            self.root
                .append(&build_row(neighbour, &self.cover_loader, &self.callbacks));
        }
    }
}

fn build_row(
    neighbour: &SoundNeighbour,
    cover_loader: &Rc<CoverLoader>,
    callbacks: &Rc<RowCallbacks>,
) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.add_css_class("reprise-sound-match");
    row.set_focusable(true);
    row.set_accessible_role(gtk4::AccessibleRole::Button);
    row.set_cursor_from_name(Some("pointer"));

    let cover = gtk4::Image::builder()
        .pixel_size(34)
        .width_request(34)
        .height_request(34)
        .build();
    CoverLoader::set_placeholder(&cover);
    let generation = Rc::new(Cell::new(1));
    cover_loader.load_into(&cover, &neighbour.path, ThumbnailSize::List, 1, &generation);

    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    text.set_hexpand(true);
    let title = gtk4::Label::builder()
        .label(&neighbour.title)
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    let artist = gtk4::Label::builder()
        .label(&neighbour.artist)
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    artist.add_css_class("dim-label");
    let similarity = gtk4::ProgressBar::new();
    similarity.set_fraction(f64::from(neighbour.percentile.clamp(0.0, 100.0)) / 100.0);
    similarity.add_css_class("reprise-sound-similarity");
    text.append(&title);
    text.append(&artist);
    text.append(&similarity);
    let percentile = gtk4::Label::new(Some(&format!("{:.0}%", neighbour.percentile)));
    percentile.add_css_class("numeric");
    row.append(&cover);
    row.append(&text);
    row.append(&percentile);

    let id = neighbour.track_id;
    let click = gtk4::GestureClick::new();
    click.set_button(1);
    click.connect_released({
        let callbacks = callbacks.clone();
        move |_, _, _, _| invoke(&callbacks.play, id)
    });
    row.add_controller(click);
    install_context_menu(
        &row,
        id,
        &neighbour.album,
        &neighbour.album_artist,
        callbacks,
    );
    row
}

pub(super) fn build_context_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&crate::ui::strings::text(
            crate::ui::strings::CONTEXT_MENU_PLAY_NEXT,
        )),
        Some("sound.play-next"),
    );
    menu.append(
        Some(&crate::ui::strings::text(
            crate::ui::strings::CONTEXT_MENU_ADD_TO_QUEUE,
        )),
        Some("sound.add-to-queue"),
    );
    menu.append(
        Some(&crate::ui::strings::text(
            crate::ui::strings::CONTEXT_MENU_GO_TO_ALBUM,
        )),
        Some("sound.open-album"),
    );
    menu
}

fn install_context_menu(
    row: &gtk4::Box,
    id: i64,
    album: &str,
    album_artist: &str,
    callbacks: &Rc<RowCallbacks>,
) {
    let menu = build_context_menu_model();
    let actions = gio::SimpleActionGroup::new();
    add_action(&actions, "play-next", id, callbacks, ActionKind::PlayNext);
    add_action(
        &actions,
        "add-to-queue",
        id,
        callbacks,
        ActionKind::AddToQueue,
    );
    let open_album = gio::SimpleAction::new("open-album", None);
    open_album.connect_activate({
        let callbacks = callbacks.clone();
        let album = album.to_owned();
        let album_artist = album_artist.to_owned();
        move |_, _| {
            let callback = callbacks.open_album.borrow().clone();
            if let Some(callback) = callback {
                callback(id, &album, &album_artist);
            }
        }
    });
    actions.add_action(&open_album);
    row.insert_action_group("sound", Some(&actions));
    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(row);
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    let right_click = gtk4::GestureClick::new();
    right_click.set_button(3);
    right_click.connect_pressed(move |_, _, x, y| {
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
            x.round() as i32,
            y.round() as i32,
            1,
            1,
        )));
        popover.popup();
    });
    row.add_controller(right_click);
}

fn add_action(
    actions: &gio::SimpleActionGroup,
    name: &str,
    id: i64,
    callbacks: &Rc<RowCallbacks>,
    kind: ActionKind,
) {
    let action = gio::SimpleAction::new(name, None);
    let callbacks = callbacks.clone();
    action.connect_activate(move |_, _| {
        let callback = match kind {
            ActionKind::PlayNext => &callbacks.play_next,
            ActionKind::AddToQueue => &callbacks.add_to_queue,
        };
        invoke(callback, id);
    });
    actions.add_action(&action);
}

#[derive(Clone, Copy)]
enum ActionKind {
    PlayNext,
    AddToQueue,
}

fn invoke(slot: &RefCell<Option<IdCallback>>, id: i64) {
    let callback = slot.borrow().clone();
    if let Some(callback) = callback {
        callback(id);
    }
}
