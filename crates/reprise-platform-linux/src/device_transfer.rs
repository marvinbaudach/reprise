//! Just-in-time Opus transcoding for device synchronization.

use std::collections::VecDeque;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use gstreamer as gst;
use gstreamer::prelude::*;

const PIPELINE_DESCRIPTION: &str = "uridecodebin name=decoder ! audioconvert ! audioresample ! \
    opusenc name=encoder ! oggmux ! filesink name=output";
const BUS_POLL_INTERVAL: gst::ClockTime = gst::ClockTime::from_mseconds(100);
const SUPPORTED_BITRATES: [u32; 5] = [64, 96, 128, 160, 192];
pub const ENCODER_WORKERS: usize = 2;
pub const MAX_READY_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct EncodeRequest {
    pub token: usize,
    pub source: PathBuf,
    pub output: PathBuf,
    pub bitrate_kbps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyFile {
    pub token: usize,
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug)]
pub struct EncodeOutcome {
    pub token: usize,
    pub result: Result<ReadyFile, TranscodeError>,
}

struct ReadyState {
    files: VecDeque<EncodeOutcome>,
    buffered_bytes: u64,
    workers: usize,
}

struct SharedReady {
    state: Mutex<ReadyState>,
    available: Condvar,
    space: Condvar,
}

type Encode = dyn Fn(&EncodeRequest, &AtomicBool) -> Result<u64, TranscodeError> + Send + Sync;

struct WorkerCompletionGuard<'a> {
    ready: &'a SharedReady,
}

impl Drop for WorkerCompletionGuard<'_> {
    fn drop(&mut self) {
        let mut state = lock(&self.ready.state);
        state.workers = state.workers.saturating_sub(1);
        self.ready.available.notify_all();
    }
}

/// Two encoder workers feeding a byte-bounded ready-file ring buffer. The
/// consumer is expected to copy each yielded file to MTP before requesting
/// more, which keeps temporary disk use bounded around 200 MiB.
pub struct EncoderPipeline {
    ready: Arc<SharedReady>,
    cancelled: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

impl EncoderPipeline {
    pub fn start(
        requests: Vec<EncodeRequest>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, std::io::Error> {
        let encode: Arc<Encode> = Arc::new(|request, cancelled| {
            transcode_to_opus(
                &request.source,
                &request.output,
                request.bitrate_kbps,
                cancelled,
            )
        });
        Self::start_with_encoder(requests, cancelled, &encode)
    }

    fn start_with_encoder(
        requests: Vec<EncodeRequest>,
        cancelled: Arc<AtomicBool>,
        encode: &Arc<Encode>,
    ) -> Result<Self, std::io::Error> {
        let requests = Arc::new(Mutex::new(VecDeque::from(requests)));
        let ready = Arc::new(SharedReady {
            state: Mutex::new(ReadyState {
                files: VecDeque::new(),
                buffered_bytes: 0,
                workers: ENCODER_WORKERS,
            }),
            available: Condvar::new(),
            space: Condvar::new(),
        });
        let mut workers = Vec::with_capacity(ENCODER_WORKERS);
        for index in 0..ENCODER_WORKERS {
            let worker_requests = requests.clone();
            let worker_ready = ready.clone();
            let worker_cancelled = cancelled.clone();
            let worker_encode = encode.clone();
            match std::thread::Builder::new()
                .name(format!("reprise-device-encoder-{index}"))
                .spawn(move || {
                    encoder_worker(
                        &worker_requests,
                        &worker_ready,
                        &worker_cancelled,
                        worker_encode.as_ref(),
                    );
                }) {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    cancelled.store(true, Ordering::SeqCst);
                    ready.space.notify_all();
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            ready,
            cancelled,
            workers,
        })
    }

    pub fn next(&self) -> Option<EncodeOutcome> {
        let mut state = lock(&self.ready.state);
        loop {
            if let Some(file) = state.files.pop_front() {
                if let Ok(file) = &file.result {
                    state.buffered_bytes = state.buffered_bytes.saturating_sub(file.size);
                }
                self.ready.space.notify_all();
                return Some(file);
            }
            if state.workers == 0 {
                return None;
            }
            state = self
                .ready
                .available
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl Drop for EncoderPipeline {
    fn drop(&mut self) {
        if self.workers.iter().any(|worker| !worker.is_finished()) {
            self.cancelled.store(true, Ordering::SeqCst);
            self.ready.space.notify_all();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn encoder_worker(
    requests: &Mutex<VecDeque<EncodeRequest>>,
    ready: &SharedReady,
    cancelled: &AtomicBool,
    encode: &Encode,
) {
    let _completion = WorkerCompletionGuard { ready };
    while !cancelled.load(Ordering::SeqCst) {
        let Some(request) = lock(requests).pop_front() else {
            break;
        };
        let token = request.token;
        let result = encode(&request, cancelled).map(|size| ReadyFile {
            token,
            path: request.output,
            size,
        });
        let outcome = EncodeOutcome { token, result };
        let size = outcome.result.as_ref().map_or(0, |file| file.size);
        let mut state = lock(&ready.state);
        while state.buffered_bytes > 0
            && state.buffered_bytes.saturating_add(size) > MAX_READY_BYTES
            && !cancelled.load(Ordering::SeqCst)
        {
            state = ready
                .space
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        if cancelled.load(Ordering::SeqCst) {
            if let Ok(file) = outcome.result {
                let _ = std::fs::remove_file(file.path);
            }
            break;
        }
        state.buffered_bytes = state.buffered_bytes.saturating_add(size);
        state.files.push_back(outcome);
        ready.available.notify_one();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
pub enum TranscodeError {
    InvalidBitrate(u32),
    InvalidPath,
    Gstreamer(String),
    Io(std::io::Error),
    Cancelled,
}

impl fmt::Display for TranscodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBitrate(value) => write!(formatter, "unsupported Opus bitrate: {value}"),
            Self::InvalidPath => formatter.write_str("transcode path is not representable"),
            Self::Gstreamer(error) => write!(formatter, "GStreamer transcode failed: {error}"),
            Self::Io(error) => write!(formatter, "transcode file I/O failed: {error}"),
            Self::Cancelled => formatter.write_str("transcode cancelled"),
        }
    }
}

impl std::error::Error for TranscodeError {}

impl From<std::io::Error> for TranscodeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Decodes one local source into an Ogg-contained Opus file. This function is
/// blocking by design and must run on an encoder worker, never the GTK thread.
pub fn transcode_to_opus(
    source: &Path,
    output: &Path,
    bitrate_kbps: u32,
    cancelled: &AtomicBool,
) -> Result<u64, TranscodeError> {
    if !SUPPORTED_BITRATES.contains(&bitrate_kbps) {
        return Err(TranscodeError::InvalidBitrate(bitrate_kbps));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Err(TranscodeError::Cancelled);
    }
    gst::init().map_err(|error| TranscodeError::Gstreamer(error.to_string()))?;
    let source_uri = gst::glib::filename_to_uri(source, None)
        .map_err(|_| TranscodeError::InvalidPath)?
        .to_string();
    let output_path = output.to_str().ok_or(TranscodeError::InvalidPath)?;
    let pipeline = gst::parse::launch(PIPELINE_DESCRIPTION)
        .map_err(|error| TranscodeError::Gstreamer(error.to_string()))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| TranscodeError::Gstreamer("parser did not create a pipeline".into()))?;
    pipeline
        .by_name("decoder")
        .ok_or_else(|| TranscodeError::Gstreamer("missing decoder".into()))?
        .set_property("uri", source_uri);
    pipeline
        .by_name("encoder")
        .ok_or_else(|| TranscodeError::Gstreamer("missing Opus encoder".into()))?
        .set_property(
            "bitrate",
            i32::try_from(bitrate_kbps * 1_000).unwrap_or(i32::MAX),
        );
    pipeline
        .by_name("output")
        .ok_or_else(|| TranscodeError::Gstreamer("missing file output".into()))?
        .set_property("location", output_path);

    let result = run_pipeline(&pipeline, cancelled);
    let _ = pipeline.set_state(gst::State::Null);
    if let Err(error) = result {
        let _ = std::fs::remove_file(output);
        return Err(error);
    }
    Ok(std::fs::metadata(output)?.len())
}

fn run_pipeline(pipeline: &gst::Pipeline, cancelled: &AtomicBool) -> Result<(), TranscodeError> {
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|error| TranscodeError::Gstreamer(error.to_string()))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| TranscodeError::Gstreamer("pipeline has no bus".into()))?;
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err(TranscodeError::Cancelled);
        }
        let Some(message) = bus.timed_pop_filtered(
            BUS_POLL_INTERVAL,
            &[gst::MessageType::Eos, gst::MessageType::Error],
        ) else {
            continue;
        };
        match message.view() {
            gst::MessageView::Eos(_) => return Ok(()),
            gst::MessageView::Error(error) => {
                return Err(TranscodeError::Gstreamer(format!(
                    "{} ({:?})",
                    error.error(),
                    error.debug()
                )));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::AtomicBool;

    fn write_silent_wav(path: &std::path::Path) {
        const SAMPLE_RATE: u32 = 8_000;
        const SAMPLES: u32 = SAMPLE_RATE / 10;
        const DATA_BYTES: u32 = SAMPLES * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + DATA_BYTES).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&DATA_BYTES.to_le_bytes());
        wav.resize(wav.len() + DATA_BYTES as usize, 0);
        fs::write(path, wav).unwrap();
    }

    #[test]
    fn gstreamer_transcode_writes_an_ogg_opus_file() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.wav");
        let output = temp.path().join("encoded.opus");
        write_silent_wav(&source);

        let size = super::transcode_to_opus(&source, &output, 96, &AtomicBool::new(false)).unwrap();

        assert!(size > 0);
        assert!(fs::read(output).unwrap().starts_with(b"OggS"));
    }

    #[test]
    fn pre_cancelled_transcode_leaves_no_output() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.wav");
        let output = temp.path().join("encoded.opus");
        write_silent_wav(&source);
        let cancelled = AtomicBool::new(true);

        assert!(matches!(
            super::transcode_to_opus(&source, &output, 96, &cancelled),
            Err(super::TranscodeError::Cancelled)
        ));
        assert!(!output.exists());
    }

    #[test]
    fn encoder_pipeline_uses_two_workers_and_yields_every_ready_file() {
        let temp = tempfile::tempdir().unwrap();
        let requests = (0..3)
            .map(|token| {
                let source = temp.path().join(format!("source-{token}.wav"));
                write_silent_wav(&source);
                super::EncodeRequest {
                    token,
                    source,
                    output: temp.path().join(format!("encoded-{token}.opus")),
                    bitrate_kbps: 64,
                }
            })
            .collect();

        assert_eq!(super::ENCODER_WORKERS, 2);
        assert_eq!(super::MAX_READY_BYTES, 200 * 1024 * 1024);
        let pipeline =
            super::EncoderPipeline::start(requests, std::sync::Arc::new(AtomicBool::new(false)))
                .unwrap();
        let mut ready = Vec::new();
        while let Some(outcome) = pipeline.next() {
            ready.push(outcome.result.unwrap());
        }
        ready.sort_by_key(|file| file.token);

        assert_eq!(ready.len(), 3);
        assert_eq!(
            ready.iter().map(|file| file.token).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(ready
            .iter()
            .all(|file| fs::read(&file.path).unwrap().starts_with(b"OggS")));
    }

    #[test]
    fn worker_panics_do_not_leave_the_pipeline_waiting_forever() {
        let requests = (0..super::ENCODER_WORKERS)
            .map(|token| super::EncodeRequest {
                token,
                source: format!("source-{token}.wav").into(),
                output: format!("encoded-{token}.opus").into(),
                bitrate_kbps: 64,
            })
            .collect();
        let encode: std::sync::Arc<super::Encode> =
            std::sync::Arc::new(|_, _| panic!("forced encoder panic"));
        let pipeline = std::sync::Arc::new(
            super::EncoderPipeline::start_with_encoder(
                requests,
                std::sync::Arc::new(AtomicBool::new(false)),
                &encode,
            )
            .unwrap(),
        );
        let waiting_pipeline = pipeline.clone();
        let (completed_tx, completed_rx) = std::sync::mpsc::channel();
        let waiting = std::thread::spawn(move || {
            completed_tx
                .send(waiting_pipeline.next().is_none())
                .unwrap();
        });

        assert_eq!(
            completed_rx.recv_timeout(std::time::Duration::from_secs(1)),
            Ok(true)
        );
        waiting.join().unwrap();
    }
}
