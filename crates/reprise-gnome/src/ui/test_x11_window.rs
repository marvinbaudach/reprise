//! Test-only X11 plumbing shared by the compact-mode and session-restore
//! display tests: a window manager to answer maximize requests, and the raw
//! XID lookup those tests drive with `xdotool`.
//!
//! A maximize is a request only a window manager answers — GTK asks for it,
//! but nothing changes the window's state without something on the other end
//! of the wire. Under Xvfb there is no window manager unless a test starts
//! one, so [`TestWindowManager`] spawns `openbox` for the duration of the
//! test and kills it on drop. `openbox` was only ever installed on the
//! workstation; #920 had to add it to the CI container's own package list
//! after a test using it merged without that line and left dev's contract
//! job red.
//!
//! Driving `xdotool` against a specific window needs that window's X11 XID,
//! and GDK4 does not expose one — [`x11_window_id`] reaches for it through
//! `gdk4_x11::ffi::gdk_x11_surface_get_xid`, a raw FFI binding that cannot be
//! called without `unsafe` wherever it lives. That is why this module, not
//! either display-test file, is the one named on `check_frontend_allowlist`'s
//! list in `scripts/check-architecture.sh`.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use libadwaita as adw;

/// Starts `openbox` for the life of the guard and kills it on drop.
///
/// Display tests that maximize or otherwise change window-manager state hold
/// one of these for their whole body; nothing here reacts to the window
/// itself, it only needs to exist for the X server to have something to talk
/// to.
pub(crate) struct TestWindowManager(Child);

impl TestWindowManager {
    pub(crate) fn start() -> Self {
        let child = Command::new("openbox")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the display regression needs the test window manager");
        std::thread::sleep(Duration::from_millis(250));
        Self(child)
    }
}

impl Drop for TestWindowManager {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Reads the X11 XID of `window`'s surface, formatted for `xdotool`.
///
/// Panics if the window has no surface yet or the display is not X11 — both
/// are test setup errors, not conditions a display test should recover from.
pub(crate) fn x11_window_id(window: &adw::ApplicationWindow) -> String {
    use gtk4::prelude::*;

    let surface = window
        .surface()
        .unwrap()
        .downcast::<gdk4_x11::X11Surface>()
        .unwrap();
    unsafe { gdk4_x11::ffi::gdk_x11_surface_get_xid(surface.as_ptr() as *mut _).to_string() }
}

/// Runs `xdotool` with `args` and returns its stdout.
///
/// Asserts a zero exit status: a failed `xdotool` call almost always means
/// the window it targeted was not where the test expected, which is worth
/// failing loudly on rather than letting a later assertion report the wrong
/// symptom.
pub(crate) fn xdotool(args: &[&str]) -> String {
    let output = Command::new("xdotool")
        .args(args)
        .output()
        .expect("the display regression needs xdotool");
    assert!(output.status.success(), "xdotool command failed: {args:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Polls `condition` on the main loop until it holds, or panics after 3s.
///
/// The window manager applies state changes asynchronously, so a test cannot
/// just call `maximize()` or `windowmove` and check the result on the next
/// line — it has to give both the X server and openbox a chance to round-trip
/// the request first.
pub(crate) fn wait_for_window_state(label: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() && Instant::now() < deadline {
        while gtk4::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(condition(), "window manager did not reach: {label}");
}
