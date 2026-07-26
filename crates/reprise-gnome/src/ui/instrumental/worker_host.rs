//! Supervisor for the packaged instrumental-render worker process.
//!
//! The GTK process never loads the inference implementation or ONNX Runtime.
//! It starts the separately packaged `reprise-worker` executable for one
//! queue-draining run, observes it from a lightweight monitor thread, and tells
//! the GTK view to re-read `ai_jobs` while work is active. A native
//! decoder/runtime crash can therefore terminate only the worker process.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use reprise_core::ai_staging::StagingStore;

/// The worker's lease remains long enough for heartbeats while making a
/// crashed render reclaimable on a later run.
const WORKER_LEASE_SECS: i64 = 120;
/// The UI reads durable job progress at most four times per second.
const PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SUPERVISOR_THREAD_NAME: &str = "reprise-instrumental-supervisor";

/// Build-time path embedded by Meson. Bare Cargo builds intentionally have no
/// production worker path and therefore cannot expose a fake render backend.
const PACKAGED_WORKER_PATH: Option<&str> = option_env!("REPRISE_INSTRUMENTAL_WORKER");

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkerCommandSpec {
    executable: PathBuf,
    args: Vec<OsString>,
}

impl WorkerCommandSpec {
    fn packaged(db_path: &Path, staging: &StagingStore) -> Result<Self, WorkerStartError> {
        let executable = PACKAGED_WORKER_PATH
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .ok_or(WorkerStartError::MissingPackagedWorker)?;
        Ok(Self::for_paths(executable, db_path, staging.root()))
    }

    fn for_paths(executable: PathBuf, db_path: &Path, staging_dir: &Path) -> Self {
        Self {
            executable,
            args: vec![
                OsString::from("--db"),
                db_path.as_os_str().to_owned(),
                OsString::from("--staging-dir"),
                staging_dir.as_os_str().to_owned(),
                OsString::from("jobs"),
                OsString::from("work"),
                OsString::from("--once"),
                OsString::from("--lease"),
                OsString::from(WORKER_LEASE_SECS.to_string()),
            ],
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        command
    }
}

/// Why the GTK client could not start its packaged worker supervisor.
#[derive(Debug, thiserror::Error)]
pub(in crate::ui) enum WorkerStartError {
    #[error("this build has no packaged instrumental worker path")]
    MissingPackagedWorker,
    #[error("could not start instrumental supervisor thread: {0}")]
    Supervisor(#[source] std::io::Error),
}

#[derive(Debug)]
struct SupervisorState {
    requested_generation: u64,
    handled_generation: u64,
    child: Option<Child>,
}

struct Shared {
    state: Mutex<SupervisorState>,
    changed: Condvar,
    stopping: AtomicBool,
    progress_tx: async_channel::Sender<()>,
}

struct Inner {
    shared: Arc<Shared>,
    monitor: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        shutdown(&self.shared, &self.monitor);
    }
}

/// Window-owned handle to the out-of-process renderer. Clones share one
/// supervisor and therefore never run more than one worker process at a time.
#[derive(Clone)]
pub(in crate::ui) struct InstrumentalWorker {
    inner: Arc<Inner>,
    progress_rx: async_channel::Receiver<()>,
}

impl InstrumentalWorker {
    /// Starts an idle supervisor for the Meson-packaged worker. Rendering
    /// begins only after [`wake`](Self::wake), so merely opening Reprise never
    /// loads a multi-gigabyte model when the queue is empty.
    pub(in crate::ui) fn new(
        db_path: &Path,
        staging: &StagingStore,
    ) -> Result<Self, WorkerStartError> {
        let spec = WorkerCommandSpec::packaged(db_path, staging)?;
        Self::start(spec)
    }

    fn start(spec: WorkerCommandSpec) -> Result<Self, WorkerStartError> {
        let (progress_tx, progress_rx) = async_channel::bounded(1);
        let shared = Arc::new(Shared {
            state: Mutex::new(SupervisorState {
                requested_generation: 0,
                handled_generation: 0,
                child: None,
            }),
            changed: Condvar::new(),
            stopping: AtomicBool::new(false),
            progress_tx,
        });
        let monitor = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name(SUPERVISOR_THREAD_NAME.into())
                .spawn(move || supervise(&spec, &shared))
                .map_err(WorkerStartError::Supervisor)?
        };
        Ok(Self {
            inner: Arc::new(Inner {
                shared,
                monitor: Mutex::new(Some(monitor)),
            }),
            progress_rx,
        })
    }

    /// Requests another finite queue-draining run. If a worker is already
    /// active, the generation is retained and one final drain starts after it
    /// exits, closing the enqueue-vs-exit race without parallel inference.
    pub(in crate::ui) fn wake(&self) {
        let mut state = self.inner.shared.state.lock().unwrap();
        state.requested_generation = state.requested_generation.wrapping_add(1);
        self.inner.shared.changed.notify_all();
    }

    /// Coalesced refresh ticks while the child runs and once when it exits.
    /// The GTK task re-reads durable job rows; no backend-private progress
    /// crosses the process boundary.
    pub(in crate::ui) fn progress_receiver(&self) -> async_channel::Receiver<()> {
        self.progress_rx.clone()
    }

    /// Stops supervising new work. A currently rendering `--once` child is
    /// deliberately detached so closing the UI does not destroy hours of
    /// compute; it drains the already-visible queue and exits by itself.
    pub(in crate::ui) fn shutdown(&self) {
        shutdown(&self.inner.shared, &self.inner.monitor);
    }

    #[cfg(test)]
    fn is_idle(&self) -> bool {
        self.inner.shared.state.lock().unwrap().child.is_none()
    }
}

fn supervise(spec: &WorkerCommandSpec, shared: &Arc<Shared>) {
    loop {
        if shared.stopping.load(Ordering::SeqCst) {
            detach_child(shared);
            return;
        }

        let should_spawn = {
            let state = shared.state.lock().unwrap();
            state.child.is_none() && state.handled_generation != state.requested_generation
        };
        if should_spawn {
            spawn_generation(spec, shared);
            continue;
        }

        let child_active = shared.state.lock().unwrap().child.is_some();
        if child_active {
            std::thread::sleep(PROGRESS_POLL_INTERVAL);
            poll_child(shared);
            continue;
        }

        let mut state = shared.state.lock().unwrap();
        while !shared.stopping.load(Ordering::SeqCst)
            && state.child.is_none()
            && state.handled_generation == state.requested_generation
        {
            state = shared.changed.wait(state).unwrap();
        }
    }
}

fn spawn_generation(spec: &WorkerCommandSpec, shared: &Shared) {
    let requested = shared.state.lock().unwrap().requested_generation;
    match spec.command().spawn() {
        Ok(child) => {
            let mut state = shared.state.lock().unwrap();
            state.handled_generation = requested;
            state.child = Some(child);
            let _ = shared.progress_tx.try_send(());
        }
        Err(error) => {
            tracing::error!(
                %error,
                worker = %spec.executable.display(),
                "instrumental: could not start packaged worker"
            );
            let mut state = shared.state.lock().unwrap();
            state.handled_generation = requested;
            let _ = shared.progress_tx.try_send(());
        }
    }
}

fn poll_child(shared: &Shared) {
    let mut state = shared.state.lock().unwrap();
    let Some(child) = state.child.as_mut() else {
        return;
    };
    match child.try_wait() {
        Ok(Some(status)) => {
            state.child.take();
            if !status.success() {
                tracing::warn!(%status, "instrumental: worker process exited unsuccessfully");
            }
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(%error, "instrumental: could not inspect worker process");
            state.child.take();
        }
    }
    let _ = shared.progress_tx.try_send(());
}

fn detach_child(shared: &Shared) {
    let mut state = shared.state.lock().unwrap();
    if state.child.take().is_some() {
        tracing::info!("instrumental: detached active worker during app shutdown");
    }
}

fn shutdown(shared: &Shared, monitor: &Mutex<Option<JoinHandle<()>>>) {
    shared.stopping.store(true, Ordering::SeqCst);
    shared.changed.notify_all();
    if let Some(handle) = monitor.lock().unwrap().take() {
        if handle.join().is_err() {
            tracing::error!("instrumental: supervisor thread panicked");
        }
    }
}

#[cfg(test)]
#[path = "worker_host_tests.rs"]
mod tests;
