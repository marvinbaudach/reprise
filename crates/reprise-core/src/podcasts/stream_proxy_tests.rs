use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
};

use super::{
    activate, constant_time_eq, register, register_resolved_with_refresh, register_with_refresh,
    revoke, revoke_active, StreamProxyError, StreamSource, CLIENT_WRITE_TIMEOUT,
};
use crate::podcasts::ytdlp::ResolvedAudio;

const REJECTED_RANGE_BYTES: u64 = 1_048_575;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RequestedRange {
    start: u64,
    end: u64,
}

struct FakeOrigin {
    address: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<Option<RequestedRange>>>>,
    failures: Arc<Mutex<HashMap<u64, VecDeque<u16>>>>,
    gate: Option<Arc<OriginGate>>,
}

struct OriginGate {
    start: u64,
    released: Mutex<bool>,
    changed: Condvar,
}

impl FakeOrigin {
    fn new(data: Vec<u8>) -> Self {
        Self::with_behavior(data, HashMap::new(), None)
    }

    fn with_failures(data: Vec<u8>, failures: HashMap<u64, VecDeque<u16>>) -> Self {
        Self::with_behavior(data, failures, None)
    }

    fn with_gate(data: Vec<u8>, start: u64) -> Self {
        Self::with_behavior(
            data,
            HashMap::new(),
            Some(Arc::new(OriginGate {
                start,
                released: Mutex::new(false),
                changed: Condvar::new(),
            })),
        )
    }

    fn with_behavior(
        data: Vec<u8>,
        failures: HashMap<u64, VecDeque<u16>>,
        gate: Option<Arc<OriginGate>>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let data = Arc::new(data);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let failures = Arc::new(Mutex::new(failures));
        let thread_requests = Arc::clone(&requests);
        let thread_failures = Arc::clone(&failures);
        let thread_gate = gate.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    break;
                };
                let data = Arc::clone(&data);
                let requests = Arc::clone(&thread_requests);
                let failures = Arc::clone(&thread_failures);
                let gate = thread_gate.clone();
                thread::spawn(move || {
                    serve_origin(&mut stream, &data, &requests, &failures, gate.as_deref());
                });
            }
        });
        Self {
            address,
            requests,
            failures,
            gate,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/audio?signature=fresh", self.address)
    }

    fn requests(&self) -> Vec<Option<RequestedRange>> {
        self.requests.lock().unwrap().clone()
    }

    fn failures_left(&self, start: u64) -> usize {
        self.failures
            .lock()
            .unwrap()
            .get(&start)
            .map_or(0, VecDeque::len)
    }

    fn release_gate(&self) {
        let gate = self.gate.as_ref().unwrap();
        *gate.released.lock().unwrap() = true;
        gate.changed.notify_all();
    }
}

fn serve_origin(
    stream: &mut TcpStream,
    data: &[u8],
    requests: &Mutex<Vec<Option<RequestedRange>>>,
    failures: &Mutex<HashMap<u64, VecDeque<u16>>>,
    gate: Option<&OriginGate>,
) {
    let Some(target) = read_request_target(stream) else {
        return;
    };
    let requested = requested_range(&target);
    requests.lock().unwrap().push(requested.clone());
    let Some(range) = requested else {
        write_response(stream, 403, &[], &[]);
        return;
    };
    if let Some(gate) = gate.filter(|gate| gate.start == range.start) {
        let mut released = gate.released.lock().unwrap();
        while !*released {
            released = gate.changed.wait(released).unwrap();
        }
    }
    let length = range.end.saturating_sub(range.start).saturating_add(1);
    if length >= REJECTED_RANGE_BYTES {
        write_response(stream, 403, &[], &[]);
        return;
    }
    if let Some(status) = failures
        .lock()
        .unwrap()
        .get_mut(&range.start)
        .and_then(VecDeque::pop_front)
    {
        write_response(stream, status, &[], &[]);
        return;
    }
    let Ok(start) = usize::try_from(range.start) else {
        write_response(stream, 416, &[], &[]);
        return;
    };
    let Ok(end) = usize::try_from(range.end) else {
        write_response(stream, 416, &[], &[]);
        return;
    };
    let Some(body) = data.get(start..=end.min(data.len().saturating_sub(1))) else {
        write_response(stream, 416, &[], &[]);
        return;
    };
    let content_range = format!(
        "bytes {}-{}/{}",
        range.start,
        range.start + body.len() as u64 - 1,
        data.len()
    );
    write_response(
        stream,
        206,
        &[("Content-Range", content_range.as_str())],
        body,
    );
}

fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 || bytes.len() + read > 16 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    let request = std::str::from_utf8(&bytes).ok()?;
    request
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)
        .map(str::to_owned)
}

fn requested_range(target: &str) -> Option<RequestedRange> {
    let url = url::Url::parse(&format!("http://localhost{target}")).ok()?;
    let value = url
        .query_pairs()
        .find_map(|(key, value)| (key == "range").then(|| value.into_owned()))?;
    let (start, end) = value.split_once('-')?;
    Some(RequestedRange {
        start: start.parse().ok()?,
        end: end.parse().ok()?,
    })
}

fn write_response(stream: &mut TcpStream, status: u16, headers: &[(&str, &str)], body: &[u8]) {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        403 => "Forbidden",
        404 => "Not Found",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    stream.write_all(response.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
}

struct TestResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn request(url: &str, range: Option<&str>) -> TestResponse {
    let url = url::Url::parse(url).unwrap();
    let address = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap());
    let mut stream = TcpStream::connect(address).unwrap();
    let mut request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        &url[url::Position::BeforePath..],
        url.host_str().unwrap()
    );
    if let Some(range) = range {
        request.push_str(&format!("Range: {range}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    let mut lines = headers.lines();
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    TestResponse {
        status,
        headers,
        body: bytes[header_end + 4..].to_vec(),
    }
}

fn request_until_transport_error(url: &str) -> (Vec<u8>, std::io::Error) {
    let url = url::Url::parse(url).unwrap();
    let address = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap());
    let mut stream = TcpStream::connect(address).unwrap();
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        &url[url::Position::BeforePath..],
        url.host_str().unwrap()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    let error = stream.read_to_end(&mut bytes).unwrap_err();
    (bytes, error)
}

fn fixture_bytes(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

#[test]
fn complete_stream_is_reassembled_from_bounded_origin_windows() {
    let data = fixture_bytes(2_050_007);
    let origin = FakeOrigin::new(data.clone());
    let token = register(origin.url(), data.len() as u64).unwrap();

    let response = request(token.playback_url(), None);

    assert_eq!(response.status, 200);
    assert_eq!(
        response.headers.get("content-length"),
        Some(&data.len().to_string())
    );
    assert_eq!(
        response.headers.get("accept-ranges").map(String::as_str),
        Some("bytes")
    );
    assert_eq!(response.body, data);
    let requests = origin.requests();
    assert!(requests.len() >= 3);
    assert!(requests.iter().all(|request| request
        .as_ref()
        .is_some_and(|range| { range.end - range.start + 1 < REJECTED_RANGE_BYTES })));
    revoke(&token);
}

#[test]
fn paused_client_receives_the_complete_stream_after_a_write_timeout() {
    let data = fixture_bytes(12_000_007);
    let origin = FakeOrigin::with_gate(data.clone(), 1_000_000);
    let token = register(origin.url(), data.len() as u64).unwrap();
    let url = url::Url::parse(token.playback_url()).unwrap();
    let address = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap())
        .parse::<std::net::SocketAddr>()
        .unwrap();
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();
    socket.set_recv_buffer_size(64 * 1024).unwrap();
    socket.connect(&address.into()).unwrap();
    let mut stream = TcpStream::from(socket);
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        &url[url::Position::BeforePath..],
        url.host_str().unwrap()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut received = vec![0; 4 * 1024];
    stream.read_exact(&mut received).unwrap();

    origin.release_gate();
    thread::sleep(CLIENT_WRITE_TIMEOUT * 10);
    stream.read_to_end(&mut received).unwrap();

    let header_end = received
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    assert!(
        received[header_end + 4..] == data,
        "paused client received a truncated or corrupted stream"
    );
    revoke(&token);
}

#[test]
fn open_client_range_returns_partial_headers_and_bytes_from_the_requested_offset() {
    let data = fixture_bytes(2_050_007);
    let origin = FakeOrigin::new(data.clone());
    let token = register(origin.url(), data.len() as u64).unwrap();
    let start = 1_234_567;

    let response = request(token.playback_url(), Some(&format!("bytes={start}-")));

    assert_eq!(response.status, 206);
    assert_eq!(
        response.headers.get("content-range"),
        Some(&format!("bytes {start}-{}/{}", data.len() - 1, data.len()))
    );
    assert_eq!(
        response.headers.get("content-length"),
        Some(&(data.len() - start).to_string())
    );
    assert_eq!(response.body, data[start..]);
    revoke(&token);
}

#[test]
fn closed_client_range_is_intentionally_rejected() {
    let data = fixture_bytes(1_024);
    let origin = FakeOrigin::new(data.clone());
    let token = register(origin.url(), data.len() as u64).unwrap();

    let response = request(token.playback_url(), Some("bytes=0-100"));

    assert_eq!(response.status, 416);
    assert_eq!(
        response.headers.get("content-range").map(String::as_str),
        Some("bytes */1024")
    );
    assert!(response.body.is_empty());
    revoke(&token);
}

#[test]
fn unknown_and_revoked_tokens_return_not_found() {
    let data = fixture_bytes(32);
    let origin = FakeOrigin::new(data.clone());
    let token = register(origin.url(), data.len() as u64).unwrap();
    let unknown_url = format!(
        "{}/not-registered",
        token.playback_url().rsplit_once('/').unwrap().0
    );

    assert_eq!(request(&unknown_url, None).status, 404);
    revoke(&token);
    assert_eq!(request(token.playback_url(), None).status, 404);
}

#[test]
fn transient_origin_failure_retries_the_same_window_without_a_gap() {
    let data = fixture_bytes(1_500_005);
    let origin =
        FakeOrigin::with_failures(data.clone(), HashMap::from([(0, VecDeque::from([500]))]));
    let token = register(origin.url(), data.len() as u64).unwrap();

    let response = request(token.playback_url(), None);

    assert_eq!(response.status, 200);
    assert_eq!(response.body, data);
    assert_eq!(origin.failures_left(0), 0);
    assert_eq!(
        origin
            .requests()
            .iter()
            .filter(|request| request.as_ref().is_some_and(|range| range.start == 0))
            .count(),
        2
    );
    revoke(&token);
}

#[test]
fn exhausted_later_window_retries_reset_the_client_connection() {
    let data = fixture_bytes(1_500_005);
    let origin = FakeOrigin::with_failures(
        data,
        HashMap::from([(1_000_000, VecDeque::from([500, 500, 500]))]),
    );
    let token = register(origin.url(), 1_500_005).unwrap();

    let (received, error) = request_until_transport_error(token.playback_url());

    let header_end = received
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    assert!(received.len() - header_end - 4 < 1_500_005);
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
    ));
    revoke(&token);
}

#[test]
fn forbidden_window_refreshes_once_and_continues_at_the_same_offset() {
    let data = fixture_bytes(2_100_009);
    let stale = FakeOrigin::with_failures(
        data.clone(),
        HashMap::from([(1_000_000, VecDeque::from([403, 403]))]),
    );
    let fresh = FakeOrigin::new(data.clone());
    let refreshes = Arc::new(AtomicUsize::new(0));
    let callback_refreshes = Arc::clone(&refreshes);
    let fresh_url = fresh.url();
    let total_len = data.len() as u64;
    let token = register_with_refresh(stale.url(), total_len, move || {
        callback_refreshes.fetch_add(1, Ordering::SeqCst);
        StreamSource::new(fresh_url.clone(), total_len)
    })
    .unwrap();

    let response = request(token.playback_url(), None);

    assert_eq!(response.status, 200);
    assert_eq!(response.body, data);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(
        stale
            .requests()
            .iter()
            .filter(|request| request
                .as_ref()
                .is_some_and(|range| range.start == 1_000_000))
            .count(),
        1
    );
    assert!(fresh.requests().iter().any(|request| {
        request
            .as_ref()
            .is_some_and(|range| range.start == 1_000_000)
    }));
    revoke(&token);
}

#[test]
fn activating_a_new_session_revokes_the_previous_token() {
    let data = fixture_bytes(32);
    let origin = FakeOrigin::new(data.clone());
    let first = register(origin.url(), data.len() as u64).unwrap();
    let first_url = activate(first);
    let second = register(origin.url(), data.len() as u64).unwrap();
    let second_url = activate(second);

    assert_eq!(request(&first_url, None).status, 404);
    assert_eq!(request(&second_url, None).body, data);
    revoke_active();
    assert_eq!(request(&second_url, None).status, 404);
}

#[test]
fn resolved_stream_without_a_content_length_is_rejected() {
    let audio = ResolvedAudio {
        stream_url: "https://googlevideo.test/audio".to_owned(),
        content_len: None,
        duration_secs: None,
        categories: Vec::new(),
        track: None,
        artist: None,
    };

    let error = register_resolved_with_refresh(audio, || -> Result<_, &'static str> {
        unreachable!("registration must fail before refresh is needed")
    })
    .unwrap_err();

    assert!(matches!(error, StreamProxyError::InvalidLength));
}

#[test]
fn token_comparison_covers_equal_unequal_and_different_length_values() {
    assert!(constant_time_eq(b"0123456789abcdef", b"0123456789abcdef"));
    assert!(!constant_time_eq(b"0123456789abcdef", b"0123456789abcdeg"));
    assert!(!constant_time_eq(b"0123456789abcdef", b"0123456789abcde"));
}
