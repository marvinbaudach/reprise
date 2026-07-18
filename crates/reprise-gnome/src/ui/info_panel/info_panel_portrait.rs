//! Artist portrait state for the information panel.
//!
//! This stays independent from Artist News so portraits can remain enabled
//! when the news module is disabled. Every context change advances a local
//! generation, preventing a response for an older selection from painting.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::artist_portrait::PortraitOutcome;

use super::artist_portrait_worker::{ArtistPortraitRequest, ArtistPortraitRuntime};

pub(in crate::ui) struct InfoPanelPortrait {
    picture: gtk4::Picture,
    runtime: Rc<ArtistPortraitRuntime>,
    generation: Cell<u64>,
    artist: RefCell<String>,
}

impl InfoPanelPortrait {
    pub(in crate::ui) fn new(
        picture: gtk4::Picture,
        runtime: &Rc<ArtistPortraitRuntime>,
    ) -> Rc<Self> {
        let portrait = Rc::new(Self {
            picture,
            runtime: runtime.clone(),
            generation: Cell::new(0),
            artist: RefCell::new(String::new()),
        });
        let alive = Rc::downgrade(&portrait);
        let target = alive.clone();
        runtime.subscribe_enabled(
            move || alive.upgrade().is_some(),
            move |enabled| {
                let Some(portrait) = target.upgrade() else {
                    return;
                };
                if enabled {
                    portrait.request_current();
                } else {
                    portrait.clear_picture();
                }
            },
        );
        portrait
    }

    pub(in crate::ui) fn set_artist(self: &Rc<Self>, artist: &str) {
        self.generation.set(self.generation.get().wrapping_add(1));
        *self.artist.borrow_mut() = artist.trim().to_string();
        self.clear_picture();
        self.request_current();
    }

    pub(in crate::ui) fn clear(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.artist.borrow_mut().clear();
        self.clear_picture();
    }

    fn request_current(self: &Rc<Self>) {
        let artist = self.artist.borrow().clone();
        if !self.runtime.enabled.get() || artist.is_empty() {
            return;
        }
        let generation = self.generation.get();
        let (sender, receiver) = async_channel::bounded(1);
        self.runtime.request(ArtistPortraitRequest {
            generation,
            artist,
            force: false,
            response: sender,
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(response) = receiver.recv().await else {
                return;
            };
            let Some(portrait) = weak.upgrade() else {
                return;
            };
            if response.generation != portrait.generation.get()
                || response.artist.as_str() != portrait.artist.borrow().as_str()
                || !portrait.runtime.enabled.get()
            {
                return;
            }
            if let Ok(PortraitOutcome::Found(path)) = response.result {
                if let Ok(texture) = gtk4::gdk::Texture::from_filename(&path) {
                    portrait.picture.set_paintable(Some(&texture));
                    portrait.picture.set_visible(true);
                }
            }
        });
    }

    fn clear_picture(&self) {
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        self.picture.set_visible(false);
    }
}
