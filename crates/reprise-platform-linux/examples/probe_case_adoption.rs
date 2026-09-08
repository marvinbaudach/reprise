//! Field probe for the case-only directory collision: creates a folder on the
//! device, then asks for a fold-equal spelling of it and reports whether the
//! copy lands. Everything it writes lives under `/Music/Reprise Probe` and is
//! removed again.
//! `cargo run -p reprise-platform-linux --example probe_case_adoption`

use gio::glib;
use reprise_platform_linux::device_sync::{DeviceMonitor, DeviceStorage};

const TARGET: &str = "/Music/Reprise Probe";
const BYTES: u64 = 4096;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .without_time()
        .with_target(false)
        .init();
    let context = glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    let main_loop = glib::MainLoop::new(Some(&context), false);

    let quit = main_loop.clone();
    context.spawn_local(async move {
        glib::timeout_future_seconds(2).await;
        let Some(device) = DeviceMonitor::new().devices().into_iter().next() else {
            println!("NO DEVICE");
            quit.quit();
            return;
        };
        // A fresh pair per run: a leftover folder from an earlier run would
        // make `make_directory` answer EXISTS and the probe would prove nothing.
        let tag = std::env::var("PROBE_TAG").unwrap_or_else(|_| "X".into());
        let resident = format!("Alpha Beta {tag}");
        let desired = format!("alpha beta {tag}");
        println!("device: {} root={}", device.name, device.root_uri);
        println!("resident={resident:?} desired={desired:?}");
        let storage = DeviceStorage::from_uri(&device.root_uri);

        let source_path = std::env::temp_dir().join("reprise-case-probe.bin");
        std::fs::write(&source_path, vec![0u8; BYTES as usize]).unwrap();
        let source = gio::File::for_path(&source_path);
        let cancellable = gio::Cancellable::new();

        println!("--- seed: {resident}/seed.bin ---");
        match storage
            .replace_managed(
                None,
                TARGET,
                &source,
                &format!("{resident}/seed.bin"),
                BYTES,
                &cancellable,
                |_, _| {},
            )
            .await
        {
            Ok(outcome) => println!("seed OK: {outcome:?}"),
            Err(error) => {
                println!("seed ERR: {error}");
                quit.quit();
                return;
            }
        }

        println!("--- probe: {desired}/probe.bin (fold-equal spelling) ---");
        let probe = storage
            .replace_managed(
                None,
                TARGET,
                &source,
                &format!("{desired}/probe.bin"),
                BYTES,
                &cancellable,
                |_, _| {},
            )
            .await;
        match &probe {
            Ok(outcome) => println!("PROBE OK: {outcome:?}"),
            Err(error) => println!("PROBE ERR: {error}"),
        }

        for path in [
            format!("{resident}/probe.bin"),
            format!("{desired}/probe.bin"),
        ] {
            let found = storage.read_managed(None, TARGET, &path).await;
            println!(
                "landed at {path}: {}",
                match found {
                    Ok(Some(bytes)) => format!("yes ({} bytes)", bytes.len()),
                    Ok(None) => "no".into(),
                    Err(error) => format!("read ERR: {error}"),
                }
            );
        }

        println!("--- cleanup ---");
        for path in [
            format!("{resident}/seed.bin"),
            format!("{resident}/probe.bin"),
            format!("{desired}/probe.bin"),
            resident.clone(),
            desired.clone(),
        ] {
            match storage.delete_managed(None, TARGET, &path).await {
                Ok(removed) => println!("  {path}: removed={removed}"),
                Err(error) => println!("  {path}: ERR {error}"),
            }
        }
        match storage
            .delete_managed(None, "/Music", "Reprise Probe")
            .await
        {
            Ok(removed) => println!("  {TARGET}: removed={removed}"),
            Err(error) => println!("  {TARGET}: ERR {error}"),
        }
        let _ = std::fs::remove_file(&source_path);

        quit.quit();
    });

    main_loop.run();
}
