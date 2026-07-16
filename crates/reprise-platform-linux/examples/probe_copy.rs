//! Field-debugging probe: performs one real MTP copy through the same
//! `DeviceStorage` path the sync uses, printing the exact failure.
//! `cargo run -p reprise-platform-linux --example probe_copy`

use gio::glib;
use reprise_platform_linux::device_sync::{DeviceMonitor, DeviceStorage};

fn main() {
    let context = glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    let main_loop = glib::MainLoop::new(Some(&context), false);

    let quit = main_loop.clone();
    context.spawn_local(async move {
        // Give the proxy volume monitor time to seed, then pick the device.
        glib::timeout_future_seconds(2).await;
        let monitor = DeviceMonitor::new();
        let Some(device) = monitor.devices().into_iter().next() else {
            println!("NO DEVICE");
            quit.quit();
            return;
        };
        println!("device: {} root={}", device.name, device.root_uri);

        let storage = DeviceStorage::from_uri(&device.root_uri);

        println!("--- inspect ---");
        match storage.inspect().await {
            Ok(contents) => println!("inspect OK: {contents:?}"),
            Err(error) => println!("inspect ERR: {error}"),
        }
        println!("--- available_bytes ---");
        match storage.available_bytes().await {
            Ok(bytes) => println!("available: {bytes:?}"),
            Err(error) => println!("available ERR: {error}"),
        }

        // Write a small local source file and copy it to the device.
        let source_path = std::env::temp_dir().join("reprise-probe-source.mp3");
        std::fs::write(&source_path, vec![0u8; 4096]).unwrap();
        let source = gio::File::for_path(&source_path);
        let cancellable = gio::Cancellable::new();

        println!("--- copy_track Probe/Probe Track.mp3 ---");
        let result = storage
            .copy_track(
                &source,
                "Probe/Probe Track.mp3",
                4096,
                &cancellable,
                |copied, total| println!("  progress {copied}/{total}"),
            )
            .await;
        match result {
            Ok(outcome) => println!("copy OK: {outcome:?}"),
            Err(error) => println!("copy ERR: {error}"),
        }

        println!("--- replace_playlist Probe.m3u8 ---");
        match storage
            .replace_playlist("Probe", b"#EXTM3U\n".to_vec())
            .await
        {
            Ok(()) => println!("playlist OK"),
            Err(error) => println!("playlist ERR: {error}"),
        }

        quit.quit();
    });

    main_loop.run();
}
