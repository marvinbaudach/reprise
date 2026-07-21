use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::related_artists::{self, RelatedArtistError, RelatedArtistSuggestion};

use super::Shared;
use crate::ui::strings;

pub(in crate::ui) struct DiscoveryState {
    suggestions: Vec<RelatedArtistSuggestion>,
    hidden_mbids: HashSet<String>,
}

impl DiscoveryState {
    pub(in crate::ui) fn new(suggestions: Vec<RelatedArtistSuggestion>) -> Self {
        Self {
            suggestions,
            hidden_mbids: HashSet::new(),
        }
    }

    fn with_hidden(suggestions: Vec<RelatedArtistSuggestion>, hidden: Vec<String>) -> Self {
        Self {
            suggestions,
            hidden_mbids: hidden.into_iter().collect(),
        }
    }

    pub(in crate::ui) fn visible(&self) -> Vec<&RelatedArtistSuggestion> {
        self.suggestions
            .iter()
            .filter(|item| !self.hidden_mbids.contains(&item.artist_mbid))
            .collect()
    }

    pub(in crate::ui) fn hidden(&self) -> Vec<&RelatedArtistSuggestion> {
        self.suggestions
            .iter()
            .filter(|item| self.hidden_mbids.contains(&item.artist_mbid))
            .collect()
    }

    pub(in crate::ui) fn hide(&mut self, mbid: &str) {
        self.hidden_mbids.insert(mbid.to_string());
    }

    pub(in crate::ui) fn restore(&mut self, mbid: &str) {
        self.hidden_mbids.remove(mbid);
    }
}

pub(in crate::ui) fn append(page: &adw::PreferencesPage, shared: &Rc<Shared>, seed_ids: Vec<i64>) {
    let group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::MIX_DISCOVERY_TITLE))
        .description(strings::text(strings::MIX_DISCOVERY_DESCRIPTION))
        .build();
    let find = gtk4::Button::with_label(&strings::text(strings::MIX_DISCOVERY_FIND));
    find.set_halign(gtk4::Align::Start);
    find.add_css_class("pill");
    group.add(&find);
    let visible = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    group.add(&visible);
    let hidden_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::MIX_DISCOVERY_HIDDEN))
        .build();
    let hidden = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    hidden_group.add(&hidden);
    hidden_group.set_visible(false);
    page.add(&group);
    page.add(&hidden_group);

    let state = Rc::new(RefCell::new(DiscoveryState::new(Vec::new())));
    find.connect_clicked({
        let shared = Rc::clone(shared);
        let group = group.clone();
        let visible = visible.clone();
        let hidden = hidden.clone();
        let hidden_group = hidden_group.clone();
        let state = Rc::clone(&state);
        move |button| {
            let enabled = reprise_core::modules::is_enabled(
                &shared.conn.borrow(),
                &reprise_core::modules::RELATED_ARTISTS_MODULE,
            )
            .unwrap_or(false);
            if !enabled {
                group.set_description(Some(&strings::text(strings::MIX_DISCOVERY_DISABLED)));
                return;
            }
            let all_urls = match related_artists::provider_urls(&shared.conn.borrow(), &seed_ids) {
                Ok(urls) if !urls.is_empty() => urls,
                Ok(_) => {
                    group.set_description(Some(&strings::text(strings::MIX_DISCOVERY_NO_SEED_ID)));
                    return;
                }
                Err(error) => {
                    group.set_description(Some(&format!(
                        "{}: {error}",
                        strings::text(strings::MIX_DISCOVERY_FAILED)
                    )));
                    return;
                }
            };
            let now = chrono::Utc::now().timestamp();
            let urls =
                related_artists::provider_urls_needing_fetch(&shared.conn.borrow(), &seed_ids, now)
                    .unwrap_or(all_urls);
            button.set_sensitive(false);
            group.set_description(Some(&strings::text(strings::MIX_DISCOVERY_LOADING)));
            let button = button.clone();
            let shared = Rc::clone(&shared);
            let group = group.clone();
            let visible = visible.clone();
            let hidden = hidden.clone();
            let hidden_group = hidden_group.clone();
            let state = Rc::clone(&state);
            let seed_ids = seed_ids.clone();
            glib::spawn_future_local(async move {
                let fetched = gio::spawn_blocking(move || {
                    let mut bodies = HashMap::new();
                    for url in urls {
                        let body = related_artists::fetch_listenbrainz(&url)
                            .map_err(|error| error.to_string())?;
                        bodies.insert(url, body);
                    }
                    Ok::<_, String>(bodies)
                })
                .await;
                button.set_sensitive(true);
                let mut bodies = match fetched {
                    Ok(Ok(bodies)) => bodies,
                    Ok(Err(error)) => {
                        group.set_description(Some(&format!(
                            "{}: {error}",
                            strings::text(strings::MIX_DISCOVERY_FAILED)
                        )));
                        return;
                    }
                    Err(_) => {
                        group.set_description(Some(&strings::text(strings::MIX_DISCOVERY_FAILED)));
                        return;
                    }
                };
                let result = related_artists::discover_related_artists(
                    &shared.conn.borrow(),
                    &seed_ids,
                    now,
                    |url| {
                        bodies.remove(url).ok_or_else(|| {
                            RelatedArtistError::Network("missing provider response".into())
                        })
                    },
                );
                match result {
                    Ok(suggestions) => {
                        let hidden_mbids =
                            related_artists::hidden_artist_mbids(&shared.conn.borrow())
                                .unwrap_or_default();
                        *state.borrow_mut() =
                            DiscoveryState::with_hidden(suggestions, hidden_mbids);
                        render(&shared, &state, &visible, &hidden, &hidden_group);
                        let empty = state.borrow().suggestions.is_empty();
                        group.set_description(Some(&strings::text(if empty {
                            strings::MIX_DISCOVERY_EMPTY
                        } else {
                            strings::MIX_DISCOVERY_DESCRIPTION
                        })));
                    }
                    Err(error) => group.set_description(Some(&format!(
                        "{}: {error}",
                        strings::text(strings::MIX_DISCOVERY_FAILED)
                    ))),
                }
            });
        }
    });
}

fn render(
    shared: &Rc<Shared>,
    state: &Rc<RefCell<DiscoveryState>>,
    visible: &gtk4::ListBox,
    hidden: &gtk4::ListBox,
    hidden_group: &adw::PreferencesGroup,
) {
    clear(visible);
    clear(hidden);
    let visible_items = state
        .borrow()
        .visible()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let hidden_items = state
        .borrow()
        .hidden()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    for suggestion in visible_items {
        visible.append(&suggestion_row(
            shared,
            state,
            visible,
            hidden,
            hidden_group,
            &suggestion,
            false,
        ));
    }
    for suggestion in &hidden_items {
        hidden.append(&suggestion_row(
            shared,
            state,
            visible,
            hidden,
            hidden_group,
            suggestion,
            true,
        ));
    }
    hidden_group.set_visible(!hidden_items.is_empty());
}

#[allow(clippy::too_many_arguments)]
fn suggestion_row(
    shared: &Rc<Shared>,
    state: &Rc<RefCell<DiscoveryState>>,
    visible: &gtk4::ListBox,
    hidden: &gtk4::ListBox,
    hidden_group: &adw::PreferencesGroup,
    suggestion: &RelatedArtistSuggestion,
    is_hidden: bool,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&suggestion.artist_name)
        .subtitle(strings::mix_discovery_subtitle(
            &suggestion.reason,
            &suggestion.source,
        ))
        .use_markup(false)
        .build();
    if !is_hidden {
        let open = gtk4::Button::with_label(&strings::text(strings::MIX_DISCOVERY_OPEN));
        let uri = format!("https://musicbrainz.org/artist/{}", suggestion.artist_mbid);
        open.connect_clicked(move |_| {
            if let Err(error) =
                gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE)
            {
                tracing::warn!(%error, "mix discovery: could not open artist");
            }
        });
        row.add_suffix(&open);
    }
    let toggle = gtk4::Button::with_label(&strings::text(if is_hidden {
        strings::MIX_DISCOVERY_RESTORE
    } else {
        strings::MIX_DISCOVERY_HIDE
    }));
    toggle.connect_clicked({
        let shared = Rc::clone(shared);
        let state = Rc::clone(state);
        let visible = visible.clone();
        let hidden = hidden.clone();
        let hidden_group = hidden_group.clone();
        let mbid = suggestion.artist_mbid.clone();
        move |_| {
            if related_artists::set_hidden(&shared.conn.borrow(), &mbid, !is_hidden).is_err() {
                return;
            }
            if is_hidden {
                state.borrow_mut().restore(&mbid);
            } else {
                state.borrow_mut().hide(&mbid);
            }
            render(&shared, &state, &visible, &hidden, &hidden_group);
        }
    });
    row.add_suffix(&toggle);
    row
}

fn clear(list: &gtk4::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::related_artists::RelatedArtistSuggestion;

    use super::DiscoveryState;

    fn suggestion() -> RelatedArtistSuggestion {
        RelatedArtistSuggestion {
            artist_mbid: "33333333-3333-3333-3333-333333333333".into(),
            artist_name: "New Artist".into(),
            seed_artist_mbid: "11111111-1111-1111-1111-111111111111".into(),
            seed_artist_name: "Seed Artist".into(),
            total_listen_count: 42,
            source: "ListenBrainz".into(),
            reason: "Related to Seed Artist".into(),
        }
    }

    #[test]
    fn ac_16_discovery_stays_separate_and_hidden_artists_can_be_restored() {
        let mut state = DiscoveryState::new(vec![suggestion()]);
        assert_eq!(state.visible()[0].artist_name, "New Artist");
        assert!(state.hidden().is_empty());
        state.hide("33333333-3333-3333-3333-333333333333");
        assert!(state.visible().is_empty());
        assert_eq!(state.hidden()[0].source, "ListenBrainz");
        state.restore("33333333-3333-3333-3333-333333333333");
        assert_eq!(state.visible().len(), 1);
    }
}
