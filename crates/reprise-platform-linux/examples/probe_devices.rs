//! Field-debugging probe: prints the raw GIO volume/mount view and what
//! `DeviceMonitor::devices()` projects from it. Run on a machine with the
//! phone attached: `cargo run -p reprise-platform-linux --example probe_devices`

use gio::glib;
use gio::prelude::*;

fn dump(monitor: &gio::VolumeMonitor, label: &str) {
    println!("=== {label}: volumes ===");
    for volume in monitor.volumes() {
        let root = volume.activation_root().map(|file| file.uri().to_string());
        println!(
            "volume name={:?} uuid={:?} activation_root={:?} mounted={}",
            volume.name(),
            volume.uuid(),
            root,
            volume.get_mount().is_some()
        );
    }
    println!("=== {label}: mounts ===");
    for mount in monitor.mounts() {
        println!(
            "mount name={:?} uuid={:?} root={:?} shadowed={}",
            mount.name(),
            mount.uuid(),
            mount.root().uri(),
            mount.is_shadowed()
        );
    }
}

fn main() {
    let context = glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    let monitor = gio::VolumeMonitor::get();
    dump(&monitor, "immediately after get()");

    // Let the proxy volume monitors seed over D-Bus, then look again.
    let main_loop = glib::MainLoop::new(Some(&context), false);
    let quit = main_loop.clone();
    glib::timeout_add_seconds_local(2, move || {
        quit.quit();
        glib::ControlFlow::Break
    });
    main_loop.run();
    dump(&monitor, "after 2s main loop");

    println!("=== DeviceMonitor::devices() ===");
    let device_monitor = reprise_platform_linux::device_sync::DeviceMonitor::new();
    for descriptor in device_monitor.devices() {
        println!(
            "descriptor id={} name={} root={} reconnectable={}",
            descriptor.id, descriptor.name, descriptor.root_uri, descriptor.reconnectable
        );
    }
}
