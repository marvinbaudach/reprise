//! Loopback transport for yt-dlp-resolved Googlevideo audio streams.

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

/// Measured on 2026-08-18: Googlevideo accepted 1,000,000 bytes while a
/// 1,048,575-byte request was rejected. Keep every origin request at the
/// smaller, proven boundary.
pub const WINDOW_BYTES: u64 = 1_000_000;

const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MAX_WINDOW_ATTEMPTS: usize = 3;
const MAX_UNAUTHENTICATED_CLIENTS: usize = 8;
const REQUEST_HEADER_TIMEOUT: Duration = Duration::from_secs(5);
const CLIENT_WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const ORIGIN_TIMEOUT: Duration = Duration::from_secs(15);
const TOKEN_BYTES: usize = 16;

type Refresh = dyn Fn() -> Result<StreamSource, StreamProxyError> + Send + Sync;

static SERVER: Mutex<Option<Arc<Server>>> = Mutex::new(None);
static ACTIVE_TOKEN: Mutex<Option<StreamToken>> = Mutex::new(None);

#[derive(Debug, thiserror::Error)]
pub enum StreamProxyError {
    #[error("could not start the local YouTube stream proxy: {0}")]
    Start(std::io::Error),
    #[error("could not generate a local YouTube stream token: {0}")]
    Random(String),
    #[error("could not start a local YouTube stream worker: {0}")]
    Worker(std::io::Error),
    #[error("YouTube did not report a usable audio stream length")]
    InvalidLength,
    #[error("the YouTube audio stream could not be refreshed: {0}")]
    Refresh(String),
    #[error("the YouTube audio stream stopped responding")]
    Upstream,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamSource {
    pub url: String,
    pub total_len: u64,
}

impl StreamSource {
    pub fn new(url: impl Into<String>, total_len: u64) -> Result<Self, StreamProxyError> {
        if total_len == 0 {
            return Err(StreamProxyError::InvalidLength);
        }
        Ok(Self {
            url: url.into(),
            total_len,
        })
    }
}

#[derive(Debug)]
pub struct StreamToken {
    id: String,
    playback_url: String,
}

impl StreamToken {
    #[must_use]
    pub fn playback_url(&self) -> &str {
        &self.playback_url
    }
}

/// Makes this registration the process's current YouTube playback session.
/// Replacing it invalidates the previous session before returning its URL.
pub fn activate(token: StreamToken) -> String {
    let playback_url = token.playback_url.clone();
    let previous = ACTIVE_TOKEN.lock().unwrap().replace(token);
    if let Some(previous) = previous {
        revoke_registration(&previous);
    }
    playback_url
}

/// Invalidates the process's current YouTube playback session, if any.
pub fn revoke_active() {
    let token = ACTIVE_TOKEN.lock().unwrap().take();
    if let Some(token) = token {
        revoke_registration(&token);
    }
}

struct Server {
    address: SocketAddr,
    registrations: Arc<Mutex<HashMap<String, Arc<Registration>>>>,
    agent: ureq::Agent,
}

struct Registration {
    source_url: Mutex<String>,
    total_len: u64,
    refresh: Option<Arc<Refresh>>,
    refresh_used: AtomicBool,
    source_version: AtomicU64,
    active: AtomicBool,
    agent: ureq::Agent,
}

struct UnauthenticatedClient {
    count: Arc<AtomicUsize>,
    active: bool,
}

impl UnauthenticatedClient {
    fn try_acquire(count: &Arc<AtomicUsize>) -> Option<Self> {
        count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_UNAUTHENTICATED_CLIENTS).then_some(current + 1)
            })
            .ok()?;
        Some(Self {
            count: Arc::clone(count),
            active: true,
        })
    }

    fn authenticated(mut self) {
        self.release();
    }

    fn release(&mut self) {
        if self.active {
            self.count.fetch_sub(1, Ordering::AcqRel);
            self.active = false;
        }
    }
}

impl Drop for UnauthenticatedClient {
    fn drop(&mut self) {
        self.release();
    }
}

enum FetchFailure {
    Forbidden,
    Retryable,
    Fatal,
}

/// Registers a signed stream without a refresh callback.
pub fn register(url: impl Into<String>, total_len: u64) -> Result<StreamToken, StreamProxyError> {
    register_inner(StreamSource::new(url, total_len)?, None)
}

/// Registers a signed stream that may be resolved once more after an origin
/// 403. The callback owns provider knowledge; the proxy only swaps sources.
pub fn register_with_refresh(
    url: impl Into<String>,
    total_len: u64,
    refresh: impl Fn() -> Result<StreamSource, StreamProxyError> + Send + Sync + 'static,
) -> Result<StreamToken, StreamProxyError> {
    register_inner(StreamSource::new(url, total_len)?, Some(Arc::new(refresh)))
}

/// Adapts yt-dlp's resolved metadata to the transport contract while leaving
/// all re-resolution work in the caller-provided callback.
pub fn register_resolved_with_refresh<E>(
    audio: super::ytdlp::ResolvedAudio,
    refresh: impl Fn() -> Result<super::ytdlp::ResolvedAudio, E> + Send + Sync + 'static,
) -> Result<StreamToken, StreamProxyError>
where
    E: std::fmt::Display,
{
    let source = source_from_resolved(audio)?;
    register_with_refresh(source.url, source.total_len, move || {
        let audio = refresh().map_err(|error| StreamProxyError::Refresh(error.to_string()))?;
        source_from_resolved(audio)
    })
}

fn source_from_resolved(
    audio: super::ytdlp::ResolvedAudio,
) -> Result<StreamSource, StreamProxyError> {
    StreamSource::new(
        audio.stream_url,
        audio.content_len.ok_or(StreamProxyError::InvalidLength)?,
    )
}

fn register_inner(
    source: StreamSource,
    refresh: Option<Arc<Refresh>>,
) -> Result<StreamToken, StreamProxyError> {
    let server = running_server()?;
    let registration = Arc::new(Registration {
        source_url: Mutex::new(source.url),
        total_len: source.total_len,
        refresh,
        refresh_used: AtomicBool::new(false),
        source_version: AtomicU64::new(0),
        active: AtomicBool::new(true),
        // Agent clones share one process-local origin connection pool.
        // Request-specific transport policy is applied in `fetch_once`.
        agent: server.agent.clone(),
    });
    let mut registrations = server.registrations.lock().unwrap();
    let id = loop {
        let candidate = generate_token_id()?;
        if !registrations.contains_key(&candidate) {
            break candidate;
        }
    };
    registrations.insert(id.clone(), registration);
    Ok(StreamToken {
        playback_url: format!("http://{}/{id}", server.address),
        id,
    })
}

fn generate_token_id() -> Result<String, StreamProxyError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut random = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut random).map_err(|error| StreamProxyError::Random(error.to_string()))?;
    let mut id = String::with_capacity(TOKEN_BYTES * 2);
    for byte in random {
        id.push(char::from(HEX[usize::from(byte >> 4)]));
        id.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(id)
}

/// Invalidates a registration. Existing client writes stop at the next
/// window boundary; later requests receive 404 immediately.
pub fn revoke(token: &StreamToken) {
    {
        let mut active = ACTIVE_TOKEN.lock().unwrap();
        if active.as_ref().is_some_and(|active| active.id == token.id) {
            active.take();
        }
    }
    revoke_registration(token);
}

fn revoke_registration(token: &StreamToken) {
    let server = SERVER.lock().unwrap().clone();
    let Some(server) = server else {
        return;
    };
    let registration = server.registrations.lock().unwrap().remove(&token.id);
    if let Some(registration) = registration {
        registration.active.store(false, Ordering::Release);
    }
}

fn running_server() -> Result<Arc<Server>, StreamProxyError> {
    let mut slot = SERVER.lock().unwrap();
    if let Some(server) = slot.as_ref() {
        return Ok(Arc::clone(server));
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(StreamProxyError::Start)?;
    let address = listener.local_addr().map_err(StreamProxyError::Start)?;
    let registrations = Arc::new(Mutex::new(HashMap::new()));
    let server = Arc::new(Server {
        address,
        registrations: Arc::clone(&registrations),
        agent: ureq::Agent::new_with_defaults(),
    });
    thread::Builder::new()
        .name("reprise-youtube-stream-proxy".to_owned())
        .spawn(move || listen(listener, registrations))
        .map_err(StreamProxyError::Start)?;
    *slot = Some(Arc::clone(&server));
    Ok(server)
}

fn listen(listener: TcpListener, registrations: Arc<Mutex<HashMap<String, Arc<Registration>>>>) {
    let unauthenticated_clients = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                tracing::error!(%error, "local YouTube stream proxy listener stopped");
                break;
            }
        };
        if let Err(error) = configure_client_stream(&stream) {
            tracing::warn!(%error, "local YouTube stream proxy rejected an unconfigurable client");
            continue;
        }
        let Some(unauthenticated) = UnauthenticatedClient::try_acquire(&unauthenticated_clients)
        else {
            let _ = stream.shutdown(Shutdown::Both);
            continue;
        };
        let registrations = Arc::clone(&registrations);
        if let Err(error) = thread::Builder::new()
            .name("reprise-youtube-stream-client".to_owned())
            .spawn(move || serve_client(stream, &registrations, unauthenticated))
        {
            tracing::error!(%error, "local YouTube stream proxy could not start a client worker");
        }
    }
    drop(listener);
    drop(registrations);
}

fn configure_client_stream(stream: &TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(REQUEST_HEADER_TIMEOUT))?;
    stream.set_write_timeout(Some(CLIENT_WRITE_TIMEOUT))
}

fn serve_client(
    mut stream: TcpStream,
    registrations: &Mutex<HashMap<String, Arc<Registration>>>,
    unauthenticated: UnauthenticatedClient,
) {
    let Some(request) = read_client_request(&mut stream) else {
        write_empty_response(&mut stream, 400, &[]);
        return;
    };
    if request.method != "GET" {
        write_empty_response(&mut stream, 405, &[("Allow", "GET")]);
        return;
    }
    let Some(id) = request
        .target
        .strip_prefix('/')
        .filter(|id| !id.is_empty() && !id.contains(['/', '?', '#']))
    else {
        write_empty_response(&mut stream, 404, &[]);
        return;
    };
    let registration = find_registration(&registrations.lock().unwrap(), id);
    let Some(registration) = registration else {
        write_empty_response(&mut stream, 404, &[]);
        return;
    };
    unauthenticated.authenticated();
    let Some(start) = request.range_start else {
        if request.had_range {
            // souphttpsrc was measured sending only an open `bytes=N-`
            // range for seeks. Closed ranges stay deliberately unsupported
            // instead of adding an unneeded second response-length path.
            write_empty_response(
                &mut stream,
                416,
                &[(
                    "Content-Range",
                    &format!("bytes */{}", registration.total_len),
                )],
            );
            return;
        }
        stream_registration(&mut stream, &registration, 0, false);
        return;
    };
    if start >= registration.total_len {
        write_empty_response(
            &mut stream,
            416,
            &[(
                "Content-Range",
                &format!("bytes */{}", registration.total_len),
            )],
        );
        return;
    }
    stream_registration(&mut stream, &registration, start, true);
}

fn find_registration(
    registrations: &HashMap<String, Arc<Registration>>,
    id: &str,
) -> Option<Arc<Registration>> {
    let mut found = None;
    for (candidate, registration) in registrations {
        if constant_time_eq(candidate.as_bytes(), id.as_bytes()) {
            found = Some(Arc::clone(registration));
        }
    }
    found
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

struct ClientRequest {
    method: String,
    target: String,
    range_start: Option<u64>,
    had_range: bool,
}

fn read_client_request(stream: &mut TcpStream) -> Option<ClientRequest> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 || bytes.len() + read > MAX_REQUEST_BYTES {
            return None;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    let request = std::str::from_utf8(&bytes).ok()?;
    let mut lines = request.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let target = request_line.next()?.to_owned();
    let range = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("Range")
            .then(|| value.trim().to_owned())
    });
    let range_start = range.as_deref().and_then(parse_open_range);
    Some(ClientRequest {
        method,
        target,
        range_start,
        had_range: range.is_some(),
    })
}

fn parse_open_range(value: &str) -> Option<u64> {
    // The measured souphttpsrc seek contract is `bytes=N-`. A closed
    // `bytes=N-M` range is intentionally rejected with 416 by `serve_client`.
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    if start.is_empty() || !end.is_empty() || value.contains(',') {
        return None;
    }
    start.parse().ok()
}

fn stream_registration(
    stream: &mut TcpStream,
    registration: &Arc<Registration>,
    start: u64,
    partial: bool,
) {
    let Ok(mut current) = fetch_window(registration, start) else {
        write_empty_response(stream, 502, &[]);
        return;
    };
    let content_len = registration.total_len - start;
    let status = if partial { 206 } else { 200 };
    let mut headers = vec![
        ("Content-Length", content_len.to_string()),
        ("Accept-Ranges", "bytes".to_owned()),
        ("Connection", "close".to_owned()),
    ];
    if partial {
        headers.push((
            "Content-Range",
            format!(
                "bytes {start}-{}/{}",
                registration.total_len - 1,
                registration.total_len
            ),
        ));
    }
    if write_headers(stream, status, &headers).is_err() {
        return;
    }
    let mut offset = start;
    loop {
        if !registration.active.load(Ordering::Acquire) {
            return;
        }
        let next_offset = offset + current.len() as u64;
        let next = (next_offset < registration.total_len).then(|| {
            let registration = Arc::clone(registration);
            thread::Builder::new()
                .name("reprise-youtube-stream-prefetch".to_owned())
                .spawn(move || fetch_window(&registration, next_offset))
        });
        if stream.write_all(&current).is_err() {
            return;
        }
        let Some(next) = next else {
            return;
        };
        let handle = match next {
            Ok(handle) => handle,
            Err(error) => {
                abort_failed_window(
                    stream,
                    registration,
                    next_offset,
                    &StreamProxyError::Worker(error),
                );
                return;
            }
        };
        let bytes = match handle.join() {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(error)) => {
                abort_failed_window(stream, registration, next_offset, &error);
                return;
            }
            Err(_) => {
                abort_failed_window(
                    stream,
                    registration,
                    next_offset,
                    &StreamProxyError::Upstream,
                );
                return;
            }
        };
        current = bytes;
        offset = next_offset;
    }
}

fn abort_failed_window(
    stream: &mut TcpStream,
    registration: &Registration,
    offset: u64,
    error: &StreamProxyError,
) {
    let window_end = (offset + WINDOW_BYTES - 1).min(registration.total_len - 1);
    tracing::error!(
        offset,
        window_end,
        cause = %error,
        "local YouTube stream proxy could not fetch a later origin window"
    );
    let socket = socket2::SockRef::from(&*stream);
    if let Err(linger_error) = socket.set_linger(Some(Duration::ZERO)) {
        tracing::error!(
            %linger_error,
            "local YouTube stream proxy could not arm an abrupt client reset"
        );
    }
}

fn fetch_window(registration: &Registration, start: u64) -> Result<Vec<u8>, StreamProxyError> {
    let end = (start + WINDOW_BYTES - 1).min(registration.total_len - 1);
    let expected = usize::try_from(end - start + 1).map_err(|_| StreamProxyError::Upstream)?;
    let mut retryable_attempts = 0;
    loop {
        let version = registration.source_version.load(Ordering::Acquire);
        match fetch_once(registration, start, end, expected) {
            Ok(bytes) => return Ok(bytes),
            Err(FetchFailure::Forbidden) => refresh_source(registration, version)?,
            Err(FetchFailure::Retryable) if retryable_attempts + 1 < MAX_WINDOW_ATTEMPTS => {
                retryable_attempts += 1;
            }
            Err(FetchFailure::Retryable | FetchFailure::Fatal) => {
                return Err(StreamProxyError::Upstream);
            }
        }
    }
}

fn fetch_once(
    registration: &Registration,
    start: u64,
    end: u64,
    expected: usize,
) -> Result<Vec<u8>, FetchFailure> {
    let source_url = registration.source_url.lock().unwrap().clone();
    let separator = if source_url.contains('?') { '&' } else { '?' };
    let url = format!("{source_url}{separator}range={start}-{end}");
    let response = registration
        .agent
        .get(&url)
        .header("Accept-Encoding", "identity")
        .config()
        .timeout_global(Some(ORIGIN_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|_| FetchFailure::Retryable)?;
    let status = response.status().as_u16();
    if status == 403 {
        return Err(FetchFailure::Forbidden);
    }
    if (500..=599).contains(&status) {
        return Err(FetchFailure::Retryable);
    }
    if status != 206 {
        return Err(FetchFailure::Fatal);
    }
    let mut bytes = Vec::with_capacity(expected);
    response
        .into_body()
        .into_reader()
        .take(expected as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FetchFailure::Retryable)?;
    if bytes.len() != expected {
        return Err(FetchFailure::Retryable);
    }
    Ok(bytes)
}

fn refresh_source(
    registration: &Registration,
    observed_version: u64,
) -> Result<(), StreamProxyError> {
    let mut source_url = registration.source_url.lock().unwrap();
    if registration.source_version.load(Ordering::Acquire) != observed_version {
        return Ok(());
    }
    if registration.refresh_used.swap(true, Ordering::AcqRel) {
        return Err(StreamProxyError::Upstream);
    }
    let refresh = registration
        .refresh
        .as_ref()
        .ok_or(StreamProxyError::Upstream)?;
    let source = refresh()?;
    if source.total_len != registration.total_len {
        return Err(StreamProxyError::Refresh(
            "the resolved stream length changed".to_owned(),
        ));
    }
    *source_url = source.url;
    registration.source_version.fetch_add(1, Ordering::Release);
    Ok(())
}

fn write_empty_response(stream: &mut TcpStream, status: u16, headers: &[(&str, &str)]) {
    let mut owned = vec![
        ("Content-Length", "0".to_owned()),
        ("Connection", "close".to_owned()),
    ];
    owned.extend(
        headers
            .iter()
            .map(|(name, value)| (*name, (*value).to_owned())),
    );
    let _ = write_headers(stream, status, &owned);
}

fn write_headers(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(&str, String)],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        502 => "Bad Gateway",
        _ => "Error",
    };
    write!(stream, "HTTP/1.1 {status} {reason}\r\n")?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(stream, "\r\n")
}

#[cfg(test)]
#[path = "stream_proxy_tests.rs"]
mod tests;
