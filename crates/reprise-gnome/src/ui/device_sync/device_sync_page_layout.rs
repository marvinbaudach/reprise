//! Widget hierarchy for the non-modal Android device dashboard.

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::TransferProfile;

use super::device_sync_dock::DeviceSyncDock;
use super::device_sync_playlist_card::PlaylistCard;
use super::device_sync_runtime::DeviceView;
use super::device_sync_storage_bar::StorageBar;

pub(super) const MUSIC_TRANSFER_PROFILE_HEADING: &str =
    super::device_sync_strings::MUSIC_TRANSFER_PROFILE_HEADING;

/// The width the content column clamps to — and, because `elide` keeps the
/// hero shrinkable, also a ceiling the page never *demands*. The two are the
/// same number on purpose: a minimum above the clamp is precisely what
/// pushes the now-playing column out of the window (`NPP-1`).
pub(super) const CONTENT_MAX_WIDTH: i32 = 1_120;

pub(super) struct DeviceDashboard {
    pub(super) root: gtk4::Box,
    pub(super) scroller: gtk4::ScrolledWindow,
    pub(super) dock: DeviceSyncDock,
    /// The complete vertical page column. Its direct children are owned here:
    /// hero, playlist/cards body, and [`Self::on_device`].
    /// The caller only fills the two section containers.
    pub(super) content: gtk4::Box,
    /// Holds the externally owned "On this device" section.
    pub(super) on_device: gtk4::Box,
    pub(super) device_name: gtk4::Button,
    pub(super) connection: gtk4::Label,
    pub(super) device_last_sync: gtk4::Label,
    pub(super) profile: gtk4::DropDown,
    pub(super) changes: gtk4::Label,
    pub(super) storage_name: gtk4::Label,
    pub(super) storage_summary: gtk4::Label,
    pub(super) storage_bar: StorageBar,
    pub(super) eject: gtk4::Button,
}

pub(super) fn build(
    device: &DeviceView,
    profile_labels: &[&str],
    playlist_card: &PlaylistCard,
) -> DeviceDashboard {
    let device_name = gtk4::Button::with_label(&device.name);
    device_name.add_css_class("flat");
    device_name.add_css_class("title-1");
    device_name.set_halign(gtk4::Align::Start);
    device_name.update_property(&[gtk4::accessible::Property::Label(
        &super::device_sync_strings::text(super::device_sync_strings::RENAME_DEVICE),
    )]);
    if let Some(name_label) = device_name.child().and_downcast::<gtk4::Label>() {
        // Ellipsize for `NPP-1`, but do NOT arm the ellipsis tooltip here: the
        // inner label's tooltip wins over the button's, and the button's is the
        // one carrying "Rename device" — or, for an unrememberable phone, the
        // explanation of why it cannot be renamed. Arming both would hide that
        // behind the plain full name on exactly the long names that ellipsize.
        // `DeviceSyncPage::update` puts the full name into the button tooltip
        // instead, so nothing is lost.
        name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    }
    let connection = label("MTP connected", "caption");
    connection.add_css_class("pill");
    connection.add_css_class("success");
    elide(&connection);
    let device_last_sync = label("", "dim-label");
    elide(&device_last_sync);
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
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    actions.set_valign(gtk4::Align::Center);
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

    let profile_title = label(
        &super::device_sync_strings::text(MUSIC_TRANSFER_PROFILE_HEADING),
        "title-2",
    );
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

    // Deliberately not "Next synchronization" — that title now belongs to
    // the Content panel's cross-category diff (`MTP-22`/`MTP-37`) appended
    // below this card. `changes` here is playlist-scoped only
    // (`page.changes`), so it gets its own, narrower heading rather than
    // implying it is the complete picture.
    let changes_heading = label("Playlist changes", "heading");
    let changes = detail_label();

    let profile_content = card_content();
    profile_content.append(&profile_title);
    profile_content.append(&profile);
    profile_content.append(&policy);
    let profile_card = card(&profile_content);
    profile_card.set_hexpand(true);
    let changes_content = card_content();
    changes_content.append(&changes_heading);
    changes_content.append(&changes);
    let changes_card = card(&changes_content);
    changes_card.set_hexpand(true);
    let card_pair = adw::WrapBox::new();
    card_pair.set_child_spacing(24);
    card_pair.set_line_spacing(16);
    card_pair.set_natural_line_length(760);
    card_pair.set_wrap_policy(adw::WrapPolicy::Natural);
    card_pair.set_justify(adw::JustifyMode::Fill);
    card_pair.set_justify_last_line(true);
    card_pair.append(&profile_card);
    card_pair.append(&changes_card);

    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
    body.append(playlist_card.root());
    body.append(&card_pair);

    let on_device = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
    content.set_margin_top(28);
    content.set_margin_bottom(28);
    content.set_margin_start(32);
    content.set_margin_end(32);
    content.append(&hero);
    content.append(&body);
    content.append(&on_device);

    let clamp = adw::Clamp::builder()
        .maximum_size(CONTENT_MAX_WIDTH)
        .tightening_threshold(900)
        .child(&content)
        .build();
    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&clamp)
        .build();
    scroller.set_vexpand(true);
    let dock = DeviceSyncDock::new();
    dock.root().set_hexpand(true);
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(&scroller);
    root.append(dock.root());
    root.add_css_class("reprise-device-dashboard");
    debug_assert_eq!(scroller.parent().as_ref(), Some(root.upcast_ref()));
    debug_assert_eq!(dock.root().parent().as_ref(), Some(root.upcast_ref()));

    DeviceDashboard {
        root,
        scroller,
        dock,
        content,
        on_device,
        device_name,
        connection,
        device_last_sync,
        profile,
        changes,
        storage_name,
        storage_summary,
        storage_bar,
        eject,
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

/// Lets a hero label give up width instead of dictating the whole page's
/// minimum. `NPP-1`'s second pitfall applies to the content pane just as it
/// does inside the panel: a label without `ellipsize` reports its full text
/// width as its *minimum*, `AdwOverlaySplitView` hands the content pane that
/// minimum, and the fixed 300 px now-playing column is pushed out of the
/// window — visibly so, since the identity line then tracked the primary
/// button's label ("Download & sync" vs "Cancel") pixel for pixel. The
/// natural width stays uncapped on purpose: the surrounding `AdwClamp` is
/// what decides how much of the text is actually shown, and the tooltip
/// carries the rest.
fn elide(label: &gtk4::Label) {
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    crate::ui::ellipsis_tooltip::arm(label);
}

fn detail_label() -> gtk4::Label {
    let label = label("", "dim-label");
    label.set_wrap(true);
    label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    label
}

pub(super) fn profile_labels(label: impl Fn(TransferProfile) -> &'static str) -> [&'static str; 3] {
    TransferProfile::ALL.map(label)
}
