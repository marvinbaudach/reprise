//! Widget hierarchy for the non-modal Android device dashboard.

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::TransferProfile;

use super::device_sync_runtime::DeviceView;
use super::device_sync_storage_bar::StorageBar;

const OVERVIEW_WIDTH_CHARS: i32 = 42;

pub(super) struct DeviceDashboard {
    pub(super) root: gtk4::ScrolledWindow,
    /// The vertical content column, exposed so the caller can append
    /// further cards (the Content/Next-synchronization panel, design 7a)
    /// below the hero and playlist body without this module needing to
    /// know about that panel's type.
    pub(super) content: gtk4::Box,
    pub(super) device_name: gtk4::Label,
    pub(super) connection: gtk4::Label,
    pub(super) device_last_sync: gtk4::Label,
    pub(super) profile: gtk4::DropDown,
    pub(super) playlist_list: gtk4::ListBox,
    pub(super) playlist_summary: gtk4::Label,
    pub(super) changes: gtk4::Label,
    pub(super) storage_name: gtk4::Label,
    pub(super) storage_summary: gtk4::Label,
    pub(super) storage_bar: StorageBar,
    pub(super) notice_box: gtk4::Box,
    pub(super) notice_title: gtk4::Label,
    pub(super) notice_detail: gtk4::Label,
    pub(super) progress_box: gtk4::Box,
    pub(super) progress_title: gtk4::Label,
    pub(super) progress_detail: gtk4::Label,
    pub(super) progress_speed: gtk4::Label,
    pub(super) progress_bar: gtk4::ProgressBar,
    pub(super) primary: gtk4::Button,
    pub(super) eject: gtk4::Button,
    /// Holds the "Recent transfers" card, refilled on every update (MTP-20).
    pub(super) history: gtk4::Box,
}

pub(super) fn build(device: &DeviceView, profile_labels: &[&str]) -> DeviceDashboard {
    let device_name = label(&device.name, "title-1");
    let connection = label("MTP connected", "caption");
    connection.add_css_class("pill");
    connection.add_css_class("success");
    let device_last_sync = label("", "dim-label");
    let status = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    status.set_valign(gtk4::Align::Center);
    status.append(&connection);
    status.append(&device_last_sync);
    let identity = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    identity.set_hexpand(true);
    identity.append(&device_name);
    identity.append(&status);

    let icon = gtk4::Image::from_gicon(&device.icon);
    icon.set_pixel_size(72);
    icon.add_css_class("reprise-device-dashboard-icon");
    let hero_top = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
    hero_top.set_valign(gtk4::Align::Center);
    hero_top.append(&icon);
    hero_top.append(&identity);

    let eject = gtk4::Button::builder()
        .icon_name("media-eject-symbolic")
        .label("Eject")
        .build();
    let primary = gtk4::Button::with_mnemonic("_Sync now");
    primary.add_css_class("suggested-action");
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    actions.set_valign(gtk4::Align::Center);
    actions.append(&primary);
    actions.append(&eject);
    hero_top.append(&actions);

    let storage_name = label("Device storage", "heading");
    let storage_summary = detail_label();
    let storage_bar = StorageBar::new();
    let storage = gtk4::Box::new(gtk4::Orientation::Vertical, 7);
    storage.append(&storage_name);
    storage.append(storage_bar.widget());
    storage.append(&storage_summary);
    let hero_content = card_content();
    hero_content.set_spacing(18);
    hero_content.append(&hero_top);
    hero_content.append(&storage);
    let hero = card(&hero_content);

    let playlist_title = label("Playlists", "title-2");
    let playlist_summary = label("", "dim-label");
    playlist_summary.set_halign(gtk4::Align::End);
    let playlist_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    playlist_header.append(&playlist_title);
    playlist_summary.set_hexpand(true);
    playlist_header.append(&playlist_summary);
    let playlist_list = gtk4::ListBox::new();
    playlist_list.set_show_separators(true);
    playlist_list.set_selection_mode(gtk4::SelectionMode::None);
    let playlist_content = card_content();
    playlist_content.set_spacing(12);
    playlist_content.append(&playlist_header);
    playlist_content.append(&playlist_list);
    let playlists = card(&playlist_content);
    playlists.set_hexpand(true);

    let overview_title = label("Sync overview", "title-2");
    let profile_title = label("Transfer profile", "heading");
    let profile_model = gtk4::StringList::new(profile_labels);
    let profile = gtk4::DropDown::builder()
        .model(&profile_model)
        .hexpand(true)
        .build();
    profile.update_property(&[gtk4::accessible::Property::Label("Transfer profile")]);
    let policy = label(
        "Lossless files use this encoder. Lossy and unknown files stay unchanged.",
        "dim-label",
    );
    policy.set_wrap(true);
    constrain_overview_width(&policy);

    // Deliberately not "Next synchronization" — that title now belongs to
    // the Content panel's cross-category diff (`MTP-22`/`MTP-37`) appended
    // below this card. `changes` here is playlist-scoped only
    // (`page.changes`), so it gets its own, narrower heading rather than
    // implying it is the complete picture.
    let changes_heading = label("Playlist changes", "heading");
    let changes = detail_label();
    constrain_overview_width(&changes);

    let notice_title = label("", "heading");
    let notice_detail = detail_label();
    constrain_overview_width(&notice_detail);
    let notice_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    notice_box.add_css_class("error");
    notice_box.set_visible(false);
    notice_box.append(&notice_title);
    notice_box.append(&notice_detail);

    let progress_title = label("", "heading");
    let progress_detail = detail_label();
    constrain_overview_width(&progress_detail);
    let speed_title = label("Transfer speed", "dim-label");
    let progress_speed = label("—", "dim-label");
    progress_speed.set_xalign(1.0);
    progress_speed.set_width_chars(10);
    progress_speed.add_css_class("numeric");
    let speed_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    speed_title.set_hexpand(true);
    speed_row.append(&speed_title);
    speed_row.append(&progress_speed);
    let progress_bar = gtk4::ProgressBar::new();
    progress_bar.set_show_text(false);
    progress_bar.update_property(&[gtk4::accessible::Property::Label(
        "Synchronization progress",
    )]);
    let progress_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    progress_box.set_visible(false);
    progress_box.append(&progress_title);
    progress_box.append(&progress_detail);
    progress_box.append(&speed_row);
    progress_box.append(&progress_bar);

    let overview_content = card_content();
    overview_content.append(&overview_title);
    overview_content.append(&profile_title);
    overview_content.append(&profile);
    overview_content.append(&policy);
    overview_content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    overview_content.append(&changes_heading);
    overview_content.append(&changes);
    overview_content.append(&notice_box);
    overview_content.append(&progress_box);
    let overview = card(&overview_content);
    overview.set_size_request(340, -1);

    let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 24);
    body.append(&playlists);
    body.append(&overview);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
    content.set_margin_top(28);
    content.set_margin_bottom(28);
    content.set_margin_start(32);
    content.set_margin_end(32);
    let history = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&hero);
    content.append(&body);
    content.append(&history);

    let clamp = adw::Clamp::builder()
        .maximum_size(1120)
        .tightening_threshold(900)
        .child(&content)
        .build();
    let root = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&clamp)
        .build();
    root.add_css_class("reprise-device-dashboard");

    DeviceDashboard {
        root,
        content,
        device_name,
        connection,
        device_last_sync,
        profile,
        playlist_list,
        playlist_summary,
        changes,
        storage_name,
        storage_summary,
        storage_bar,
        notice_box,
        notice_title,
        notice_detail,
        progress_box,
        progress_title,
        progress_detail,
        progress_speed,
        progress_bar,
        primary,
        eject,
        history,
    }
}

fn card_content() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content
}

fn card(child: &impl IsA<gtk4::Widget>) -> adw::Bin {
    let card = adw::Bin::builder().child(child).build();
    card.add_css_class("card");
    card
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class(class);
    label
}

fn detail_label() -> gtk4::Label {
    let label = label("", "dim-label");
    label.set_wrap(true);
    label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    label
}

fn constrain_overview_width(label: &gtk4::Label) {
    label.set_width_chars(OVERVIEW_WIDTH_CHARS);
    label.set_max_width_chars(OVERVIEW_WIDTH_CHARS);
}

pub(super) fn profile_labels(label: impl Fn(TransferProfile) -> &'static str) -> [&'static str; 3] {
    TransferProfile::ALL.map(label)
}
