//! A minimal HTTP/1.1 server on `127.0.0.1` that stands in for the real API, so the quick start
//! runs without network access or credentials.
//!
//! This is example scaffolding, not part of Concord and not a model for a real server: it reads
//! one request per connection (`Content-Length` bodies only), answers with a handler and closes
//! the connection. It records every request as received, so the tests can check what reqwest put
//! on the wire.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use concord::reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// One request as the server received it.
#[derive(Debug, Clone)]
pub struct Request {
    /// The method, e.g. `GET`.
    pub method: String,
    /// The request target, e.g. `/api/projects/demo/tasks?limit=20`.
    pub target: String,
    /// Header lines in arrival order, names lowercased.
    pub headers: Vec<(String, String)>,
    /// The body bytes.
    pub body: Vec<u8>,
}

impl Request {
    /// The first value of a header (case-insensitive name), if present.
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The target without its query string.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    /// The value of the first query parameter named `name`, exactly as sent.
    pub fn query_param(&self, name: &str) -> Option<&str> {
        let (_, query) = self.target.split_once('?')?;
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
    }
}

/// A response to send.
#[derive(Debug, Clone)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The `Content-Type`.
    pub content_type: &'static str,
    /// The body.
    pub body: String,
}

impl Response {
    /// A JSON response.
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.to_string(),
        }
    }

    /// An RFC 9457 problem document with a `title`.
    pub fn problem(status: u16, title: &str) -> Self {
        Self {
            status,
            content_type: "application/problem+json",
            body: json!({ "title": title }).to_string(),
        }
    }
}

type Handler = dyn Fn(&Request) -> Response + Send + Sync;

/// A running server. It keeps accepting connections until the Tokio runtime shuts down.
#[derive(Debug)]
pub struct LocalServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl LocalServer {
    /// Starts a server on a free loopback port that answers every request with `handler`.
    pub async fn start(
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Arc<Handler> = Arc::new(handler);
        let log = Arc::clone(&requests);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (handler, log) = (Arc::clone(&handler), Arc::clone(&log));
                // A failed connection only affects its own request.
                tokio::spawn(async move { serve(stream, &*handler, &log).await });
            }
        });
        Ok(Self { addr, requests })
    }

    /// The server's origin, `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every request received so far, in arrival order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

async fn serve(
    mut stream: TcpStream,
    handler: &Handler,
    log: &Mutex<Vec<Request>>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut start = lines.next().unwrap_or_default().split(' ');
    let method = start.next().unwrap_or_default().to_owned();
    let target = start.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let length = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[head_end..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }

    let request = Request {
        method,
        target,
        headers,
        body,
    };
    let response = handler(&request);
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(request);

    let reason = StatusCode::from_u16(response.status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("");
    let head = format!(
        "HTTP/1.1 {} {reason}\r\ncontent-type: {}\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len(),
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(response.body.as_bytes()).await?;
    stream.shutdown().await
}

/// The stand-in for the quick start's task service: `POST` and `GET` on
/// `/api/projects/{project}/tasks`, kept in memory, with a bearer-token check.
///
/// - Every request must carry `Authorization: Bearer <token>`, otherwise `401`.
/// - `POST` with an empty `title` is answered `422` with a problem document.
/// - `GET` honours `done=true|false` and `limit`.
pub fn tasks_service(token: &str) -> impl Fn(&Request) -> Response + Send + Sync + 'static {
    let expected = format!("Bearer {token}");
    let tasks: Mutex<Vec<Value>> = Mutex::new(Vec::new());
    move |request| {
        if request.header("authorization") != Some(expected.as_str()) {
            return Response::problem(401, "missing or unknown token");
        }
        let is_tasks = request
            .path()
            .strip_prefix("/api/projects/")
            .and_then(|rest| rest.strip_suffix("/tasks"))
            .is_some_and(|project| !project.is_empty() && !project.contains('/'));
        if !is_tasks {
            return Response::problem(404, "no such resource");
        }
        let mut tasks = tasks.lock().unwrap_or_else(PoisonError::into_inner);
        match request.method.as_str() {
            "POST" => {
                let Ok(new) = serde_json::from_slice::<Value>(&request.body) else {
                    return Response::problem(400, "the body is not JSON");
                };
                let title = new["title"].as_str().unwrap_or_default();
                if title.is_empty() {
                    return Response::problem(422, "a task needs a title");
                }
                let task = json!({ "id": tasks.len() + 1, "title": title, "done": false });
                tasks.push(task.clone());
                Response::json(201, task)
            }
            "GET" => {
                let done = request.query_param("done").map(|d| d == "true");
                let limit = request
                    .query_param("limit")
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(usize::MAX);
                let listed: Vec<Value> = tasks
                    .iter()
                    .filter(|t| done.is_none_or(|d| t["done"] == d))
                    .take(limit)
                    .cloned()
                    .collect();
                Response::json(200, Value::Array(listed))
            }
            _ => Response::problem(405, "method not allowed"),
        }
    }
}
