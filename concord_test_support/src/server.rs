use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const IO_POLL: Duration = Duration::from_millis(50);
const GATE_LIMIT: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct BodyGate(Arc<(Mutex<bool>, Condvar)>);

impl BodyGate {
    #[must_use]
    pub fn closed() -> Self {
        Self(Arc::new((Mutex::new(false), Condvar::new())))
    }

    pub fn release(&self) {
        let (open, wake) = &*self.0;
        *open.lock().expect("body gate mutex poisoned") = true;
        wake.notify_all();
    }

    fn wait(&self) -> bool {
        let (open, wake) = &*self.0;
        let guard = open.lock().expect("body gate mutex poisoned");
        let (guard, _) = wake
            .wait_timeout_while(guard, GATE_LIMIT, |open| !*open)
            .expect("body gate mutex poisoned while waiting");
        *guard
    }
}

#[derive(Clone)]
pub enum ResponseScript {
    Complete {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    Chunked {
        status: u16,
        headers: Vec<(String, String)>,
        chunks: Vec<Vec<u8>>,
    },
    Truncated {
        status: u16,
        declared_length: usize,
        prefix: Vec<u8>,
    },
    DelayBeforeHeaders {
        delay: Duration,
        status: u16,
        body: Vec<u8>,
    },
    GatedBody {
        status: u16,
        body: Vec<u8>,
        gate: BodyGate,
    },
    GatedChunked {
        status: u16,
        first_chunk: Vec<u8>,
        remaining_chunk: Vec<u8>,
        gate: BodyGate,
    },
    CloseWithoutResponse,
}

impl ResponseScript {
    #[must_use]
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self::Complete {
            status: 200,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    #[must_use]
    pub fn status(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::Complete {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    #[must_use]
    pub fn with_header(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        match self {
            Self::Complete {
                status,
                mut headers,
                body,
            } => {
                headers.push((name.into(), value.into()));
                Self::Complete {
                    status,
                    headers,
                    body,
                }
            }
            _ => panic!("headers can only be added to complete scripted responses"),
        }
    }
}

#[derive(Clone)]
pub struct RecordedRequest {
    connection_id: u64,
    method: String,
    target: String,
    headers: Vec<(String, String)>,
}

impl RecordedRequest {
    #[must_use]
    pub fn connection_id(&self) -> u64 {
        self.connection_id
    }

    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

struct Shared {
    scripts: Mutex<VecDeque<ResponseScript>>,
    requests: Mutex<Vec<RecordedRequest>>,
    changed: Condvar,
    stop: AtomicBool,
    next_connection: AtomicU64,
}

pub struct ScriptedServer {
    address: SocketAddr,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl ScriptedServer {
    pub fn start(scripts: impl IntoIterator<Item = ResponseScript>) -> io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let shared = Arc::new(Shared {
            scripts: Mutex::new(scripts.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            next_connection: AtomicU64::new(1),
        });
        let thread_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("concord-http-probe".into())
            .spawn(move || accept_loop(listener, thread_shared))?;
        Ok(Self {
            address,
            shared,
            worker: Some(worker),
        })
    }

    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    pub fn push(&self, script: ResponseScript) {
        self.shared
            .scripts
            .lock()
            .expect("script queue mutex poisoned")
            .push_back(script);
    }

    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.shared
            .requests
            .lock()
            .expect("request record mutex poisoned")
            .clone()
    }

    #[must_use]
    pub fn request_count(&self) -> usize {
        self.shared
            .requests
            .lock()
            .expect("request record mutex poisoned")
            .len()
    }

    pub fn wait_for_requests(&self, expected: usize, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        let records = self
            .shared
            .requests
            .lock()
            .expect("request record mutex poisoned");
        let (records, _) = self
            .shared
            .changed
            .wait_timeout_while(records, timeout, |records| records.len() < expected)
            .expect("request record mutex poisoned while waiting");
        if records.len() >= expected {
            Ok(())
        } else {
            Err(format!(
                "loopback server observed {} of {expected} requests after {:?}",
                records.len(),
                start.elapsed()
            ))
        }
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("loopback server thread panicked");
        }
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    let mut handlers = Vec::new();
    while !shared.stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let id = shared.next_connection.fetch_add(1, Ordering::Relaxed);
                let connection_shared = Arc::clone(&shared);
                handlers.push(
                    thread::Builder::new()
                        .name(format!("concord-http-connection-{id}"))
                        .spawn(move || handle_connection(stream, id, connection_shared))
                        .expect("failed to spawn loopback connection worker"),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("loopback accept failed: {error}"),
        }
    }
    for handler in handlers {
        handler.join().expect("loopback connection worker panicked");
    }
}

fn handle_connection(mut stream: TcpStream, connection_id: u64, shared: Arc<Shared>) {
    stream
        .set_read_timeout(Some(IO_POLL))
        .expect("failed to set loopback read timeout");
    let mut buffered = Vec::new();
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        let Some(request) = read_request(&mut stream, connection_id, &mut buffered, &shared) else {
            return;
        };
        {
            let mut requests = shared
                .requests
                .lock()
                .expect("request record mutex poisoned");
            requests.push(request);
            shared.changed.notify_all();
        }
        let script = shared
            .scripts
            .lock()
            .expect("script queue mutex poisoned")
            .pop_front()
            .unwrap_or_else(|| ResponseScript::status(500, b"unscripted request".to_vec()));
        if write_response(&mut stream, script).is_err() {
            return;
        }
    }
}

fn read_request(
    stream: &mut TcpStream,
    connection_id: u64,
    buffered: &mut Vec<u8>,
    shared: &Shared,
) -> Option<RecordedRequest> {
    loop {
        if let Some(end) = find_header_end(buffered) {
            let head = buffered[..end].to_vec();
            let text = String::from_utf8(head).ok()?;
            let mut lines = text.split("\r\n");
            let mut request_line = lines.next()?.split_whitespace();
            let method = request_line.next()?.to_owned();
            let target = request_line.next()?.to_owned();
            let headers = lines
                .filter_map(|line| line.split_once(':'))
                .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
                .collect::<Vec<_>>();
            let body_length = headers
                .iter()
                .find(|(name, _)| name == "content-length")
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(0);
            let consumed = end + 4 + body_length;
            while buffered.len() < consumed {
                if !read_more(stream, buffered, shared) {
                    return None;
                }
            }
            buffered.drain(..consumed);
            return Some(RecordedRequest {
                connection_id,
                method,
                target,
                headers,
            });
        }
        if !read_more(stream, buffered, shared) {
            return None;
        }
    }
}

fn read_more(stream: &mut TcpStream, buffered: &mut Vec<u8>, shared: &Shared) -> bool {
    let mut block = [0_u8; 2048];
    match stream.read(&mut block) {
        Ok(0) => false,
        Ok(count) => {
            buffered.extend_from_slice(&block[..count]);
            true
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            !shared.stop.load(Ordering::Acquire)
        }
        Err(_) => false,
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_response(stream: &mut TcpStream, script: ResponseScript) -> io::Result<()> {
    match script {
        ResponseScript::Complete {
            status,
            headers,
            body,
        } => {
            write_head(stream, status, &headers, Some(body.len()), false)?;
            stream.write_all(&body)?;
            stream.flush()
        }
        ResponseScript::Chunked {
            status,
            headers,
            chunks,
        } => {
            write_head(stream, status, &headers, None, true)?;
            for chunk in chunks {
                write!(stream, "{:X}\r\n", chunk.len())?;
                stream.write_all(&chunk)?;
                stream.write_all(b"\r\n")?;
            }
            stream.write_all(b"0\r\n\r\n")?;
            stream.flush()
        }
        ResponseScript::Truncated {
            status,
            declared_length,
            prefix,
        } => {
            write_head(stream, status, &[], Some(declared_length), false)?;
            stream.write_all(&prefix)?;
            stream.flush()?;
            stream.shutdown(Shutdown::Both)
        }
        ResponseScript::DelayBeforeHeaders {
            delay,
            status,
            body,
        } => {
            thread::sleep(delay);
            write_head(stream, status, &[], Some(body.len()), false)?;
            stream.write_all(&body)?;
            stream.flush()
        }
        ResponseScript::GatedBody { status, body, gate } => {
            write_head(stream, status, &[], Some(body.len()), false)?;
            stream.flush()?;
            if gate.wait() {
                stream.write_all(&body)?;
                stream.flush()
            } else {
                stream.shutdown(Shutdown::Both)
            }
        }
        ResponseScript::GatedChunked {
            status,
            first_chunk,
            remaining_chunk,
            gate,
        } => {
            write_head(stream, status, &[], None, true)?;
            write!(stream, "{:X}\r\n", first_chunk.len())?;
            stream.write_all(&first_chunk)?;
            stream.write_all(b"\r\n")?;
            stream.flush()?;
            if gate.wait() {
                write!(stream, "{:X}\r\n", remaining_chunk.len())?;
                stream.write_all(&remaining_chunk)?;
                stream.write_all(b"\r\n0\r\n\r\n")?;
                stream.flush()
            } else {
                stream.shutdown(Shutdown::Both)
            }
        }
        ResponseScript::CloseWithoutResponse => stream.shutdown(Shutdown::Both),
    }
}

fn write_head(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(String, String)],
    content_length: Option<usize>,
    chunked: bool,
) -> io::Result<()> {
    write!(stream, "HTTP/1.1 {status} {}\r\n", reason(status))?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    if let Some(length) = content_length {
        write!(stream, "Content-Length: {length}\r\n")?;
    }
    if chunked {
        stream.write_all(b"Transfer-Encoding: chunked\r\n")?;
    }
    stream.write_all(b"Connection: keep-alive\r\n\r\n")
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Probe Status",
    }
}
