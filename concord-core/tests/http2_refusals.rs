//! C4.1, C4.4 and C7.1: what `concord::Http` puts on the wire when an HTTP/2 server refuses a
//! request, read from the frames that a local server received.
//!
//! `Http::new()` negotiates HTTP/2 only through TLS, so these tests build the client with
//! `Http::with(|b| b.http2_prior_knowledge())`: the setting under test is applied after the
//! closure, in `new()` and `with()` alike.
#![cfg(feature = "http2")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use concord_core::__private::{Core, Draft, Idempotent, Options, Run};
use concord_core::error::{Class, Kind};
use concord_core::transport::{TransportError, TryInfo};
use concord_core::{http, reqwest, Answer, Body, Error, Http, Retry, Transport};
use futures_util::FutureExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A plain-text HTTP/2 server written frame by frame, so that a test controls exactly which
/// frame refuses a request and reads exactly which frames arrived.
mod peer {
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// The first byte of a header block that starts with `:method` from the HPACK static
    /// table: index 2 is `GET`, index 3 is `POST`.
    pub const GET: u8 = 0x82;
    pub const POST: u8 = 0x83;

    const PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
    const DATA: u8 = 0x0;
    const HEADERS: u8 = 0x1;
    const RST_STREAM: u8 = 0x3;
    const SETTINGS: u8 = 0x4;
    const PING: u8 = 0x6;
    const GOAWAY: u8 = 0x7;
    const END_STREAM: u8 = 0x1;
    const ACK: u8 = 0x1;
    const END_HEADERS: u8 = 0x4;
    const PADDED: u8 = 0x8;
    const PRIORITY: u8 = 0x20;
    const REFUSED_STREAM: u32 = 0x7;
    const NO_ERROR: u32 = 0x0;

    /// How the server refuses a request.
    #[derive(Clone, Copy, Debug)]
    pub enum Refusal {
        /// `RST_STREAM` with `REFUSED_STREAM` on the request's stream.
        Stream,
        /// `GOAWAY` with `NO_ERROR` and a last stream below the request's stream.
        GoAway,
    }

    /// One request as it arrived: its connection, its stream, the first byte of its header
    /// block, and the bytes of its DATA frames.
    #[derive(Debug)]
    pub struct Arrival {
        pub connection: usize,
        pub stream: u32,
        pub method: u8,
        pub body: Vec<u8>,
    }

    #[derive(Default)]
    struct Seen {
        arrivals: Vec<Arrival>,
        requests: usize,
    }

    pub struct Peer {
        addr: SocketAddr,
        seen: Arc<Mutex<Seen>>,
        accept: JoinHandle<()>,
    }

    impl Peer {
        /// Refuses the first `refuse` requests, counted over every connection, and answers
        /// `200` with an empty body to the others once their request has ended.
        pub fn start(refusal: Refusal, refuse: usize) -> Peer {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let seen = Arc::new(Mutex::new(Seen::default()));
            let shared = seen.clone();
            let accept = std::thread::spawn(move || {
                let mut connections = Vec::new();
                for (index, stream) in listener.incoming().enumerate() {
                    let mut stream = stream.unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut preface = [0; 24];
                    // The connection that `finish` opens sends nothing: stop accepting.
                    if stream.read_exact(&mut preface).is_err() {
                        break;
                    }
                    assert_eq!(&preface, PREFACE, "an HTTP/2 client preface");
                    let seen = shared.clone();
                    connections.push(std::thread::spawn(move || {
                        serve(stream, index, refusal, refuse, &seen)
                    }));
                }
                for connection in connections {
                    connection.join().unwrap();
                }
            });
            Peer { addr, seen, accept }
        }

        pub fn url(&self) -> &'static str {
            Box::leak(format!("http://{}/token", self.addr).into_boxed_str())
        }

        /// Stops accepting, waits until every connection has closed, and returns what arrived.
        /// Drop every client first: their pooled connections stay open until then.
        pub fn finish(self) -> Vec<Arrival> {
            drop(TcpStream::connect(self.addr).unwrap());
            self.accept.join().unwrap();
            std::mem::take(&mut self.seen.lock().unwrap().arrivals)
        }
    }

    fn frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
        let length = u32::try_from(payload.len()).unwrap().to_be_bytes();
        let mut frame = vec![length[1], length[2], length[3], kind, flags];
        frame.extend_from_slice(&stream.to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[derive(Clone, Copy)]
    enum Reply {
        Ok,
        Refuse(Refusal),
    }

    impl Reply {
        fn send(self, stream: &mut TcpStream, id: u32) {
            let _ = match self {
                // `:status: 200`, HPACK static index 8, and no body.
                Reply::Ok => {
                    stream.write_all(&frame(HEADERS, END_STREAM | END_HEADERS, id, &[0x88]))
                }
                Reply::Refuse(Refusal::Stream) => {
                    stream.write_all(&frame(RST_STREAM, 0, id, &REFUSED_STREAM.to_be_bytes()))
                }
                Reply::Refuse(Refusal::GoAway) => {
                    // The last stream that the server processed: none below this one.
                    let mut payload = id.saturating_sub(2).to_be_bytes().to_vec();
                    payload.extend_from_slice(&NO_ERROR.to_be_bytes());
                    stream.write_all(&frame(GOAWAY, 0, 0, &payload))
                }
            };
        }
    }

    fn serve(
        mut stream: TcpStream,
        index: usize,
        refusal: Refusal,
        refuse: usize,
        seen: &Mutex<Seen>,
    ) {
        stream.write_all(&frame(SETTINGS, 0, 0, &[])).unwrap();
        // Streams whose request has not ended yet, and whether the server answers them.
        let mut open: HashMap<u32, (usize, Reply)> = HashMap::new();
        loop {
            let mut head = [0u8; 9];
            if stream.read_exact(&mut head).is_err() {
                return;
            }
            let length = u32::from_be_bytes([0, head[0], head[1], head[2]]) as usize;
            let (kind, flags) = (head[3], head[4]);
            let id = u32::from_be_bytes([head[5], head[6], head[7], head[8]]) & 0x7fff_ffff;
            let mut payload = vec![0; length];
            if stream.read_exact(&mut payload).is_err() {
                return;
            }
            match kind {
                SETTINGS if flags & ACK == 0 => {
                    stream.write_all(&frame(SETTINGS, ACK, 0, &[])).unwrap();
                }
                PING if flags & ACK == 0 => {
                    stream.write_all(&frame(PING, ACK, 0, &payload)).unwrap();
                }
                HEADERS => {
                    assert_eq!(flags & (PADDED | PRIORITY), 0, "a plain header block");
                    assert_ne!(flags & END_HEADERS, 0, "a header block in one frame");
                    let mut seen = seen.lock().unwrap();
                    let at = seen.arrivals.len();
                    seen.arrivals.push(Arrival {
                        connection: index,
                        stream: id,
                        method: payload[0],
                        body: Vec::new(),
                    });
                    seen.requests += 1;
                    let refused = seen.requests <= refuse;
                    drop(seen);
                    // Each request gets its answer or its refusal once it has ended, so that a
                    // client that stops sending on a refusal cannot cut its body short.
                    let reply = if refused {
                        Reply::Refuse(refusal)
                    } else {
                        Reply::Ok
                    };
                    if flags & END_STREAM != 0 {
                        reply.send(&mut stream, id);
                    } else {
                        open.insert(id, (at, reply));
                    }
                }
                DATA => {
                    let Some(&(at, reply)) = open.get(&id) else {
                        continue;
                    };
                    assert_eq!(flags & PADDED, 0, "unpadded DATA");
                    seen.lock().unwrap().arrivals[at]
                        .body
                        .extend_from_slice(&payload);
                    if flags & END_STREAM != 0 {
                        open.remove(&id);
                        reply.send(&mut stream, id);
                    }
                }
                _ => {}
            }
        }
    }
}

use peer::{Arrival, Peer, Refusal, GET, POST};

const FORM: &str = "grant_type=refresh_token&refresh_token=first";

/// Runs the work on paused time, polling it without letting the clock run: the server is a
/// thread, and the clock moves only past the engine's wait after a failed attempt.
fn run<F: std::future::Future>(work: impl FnOnce(Arc<AtomicBool>) -> F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            let failed = Arc::new(AtomicBool::new(false));
            let mut work = std::pin::pin!(work(failed.clone()));
            loop {
                if let Some(output) = work.as_mut().now_or_never() {
                    break output;
                }
                if failed.swap(false, Ordering::SeqCst) {
                    // The longest wait of the standard table before a third attempt.
                    tokio::time::advance(Duration::from_secs(2)).await;
                } else {
                    tokio::task::yield_now().await;
                }
            }
        })
}

/// Concord's HTTP client, and a flag raised when an attempt fails.
struct Watched {
    http: Http,
    failed: Arc<AtomicBool>,
}

impl Transport for Watched {
    async fn send(
        &self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let result = self.http.send(request, info).await;
        if result.is_err() {
            self.failed.store(true, Ordering::SeqCst);
        }
        result
    }
}

fn watched(http: Http, failed: Arc<AtomicBool>) -> Watched {
    Watched { http, failed }
}

fn http() -> Http {
    Http::with(|builder| builder.no_proxy().http2_prior_knowledge()).unwrap()
}

/// Repeats at once, as often as `tries` allows, whatever the failure; C4.1 still decides
/// whether the request may be repeated.
fn at_once(tries: u32) -> Retry<()> {
    Retry::none().tries(tries).when(|_| Some(Duration::ZERO))
}

async fn call(
    core: &Core<()>,
    method: http::Method,
    url: &'static str,
    idempotent: Idempotent,
) -> Result<http::StatusCode, Error> {
    let options = Options::default();
    let run = Run::start(core, &options, "tokens.exchange");
    let mut draft = Draft::from_literal(method.clone(), url);
    if method == http::Method::POST {
        draft.body(Body::new(FORM, "application/x-www-form-urlencoded"));
    }
    let answer = draft
        .prepare(
            run,
            core.clone(),
            options,
            idempotent,
            |response, _| async move {
                Ok(Answer::new(
                    response.status(),
                    response.headers().clone().into(),
                    (),
                ))
            },
        )?
        .send()
        .await?;
    Ok(answer.status())
}

fn assert_requests(arrivals: &[Arrival], method: u8, count: usize) {
    assert_eq!(arrivals.len(), count, "requests on the wire: {arrivals:?}");
    for arrival in arrivals {
        assert_eq!(arrival.method, method, "{arrival:?}");
        if method == POST {
            assert_eq!(arrival.body, FORM.as_bytes(), "{arrival:?}");
        }
    }
}

fn assert_transport_failure(error: &Error, tries: u32) {
    assert_eq!(error.kind(), Kind::Transport, "{error}");
    assert_eq!(error.class(), Some(Class::Connect), "{error}");
    assert_eq!(error.tries(), tries, "{error}");
    // The server received the request's headers: nothing says that it was not sent.
    assert!(!error.request_not_sent(), "{error}");
}

#[test]
fn c4_1_reqwest_resends_a_refused_post_and_http_does_not() {
    for refusal in [Refusal::Stream, Refusal::GoAway] {
        // reqwest's default client puts the same POST on the wire again, on its own.
        let peer = Peer::start(refusal, 1);
        let url = peer.url();
        let status = run(|_| async move {
            let client = reqwest::Client::builder()
                .no_proxy()
                .http2_prior_knowledge()
                .build()
                .unwrap();
            client.post(url).body(FORM).send().await.unwrap().status()
        });
        assert_eq!(status, 200);
        let arrivals = peer.finish();
        assert_requests(&arrivals, POST, 2);
        let (first, again) = (&arrivals[0], &arrivals[1]);
        match refusal {
            // The same connection, on the next stream.
            Refusal::Stream => assert_eq!(
                (again.connection, again.stream),
                (first.connection, first.stream + 2)
            ),
            // A new connection.
            Refusal::GoAway => assert_ne!(again.connection, first.connection),
        }

        // Concord's client sends it once, and the call fails; with no policy and with one,
        // because the engine does not repeat a POST that is not marked idempotent.
        for retry in [Retry::none(), at_once(3)] {
            let peer = Peer::start(refusal, 1);
            let url = peer.url();
            let result = run(|failed| async move {
                let core = Core::<()>::default()
                    .with_transport(watched(http(), failed))
                    .with_retry(retry);
                call(&core, http::Method::POST, url, Idempotent::ByMethod).await
            });
            assert_requests(&peer.finish(), POST, 1);
            assert_transport_failure(&result.unwrap_err(), 1);
        }
    }
}

#[test]
fn c4_4_a_refused_get_is_repeated_only_by_the_engine_and_counted() {
    for refusal in [Refusal::Stream, Refusal::GoAway] {
        // reqwest's default client sends the GET again, below any count of attempts.
        let peer = Peer::start(refusal, 1);
        let url = peer.url();
        let status = run(|_| async move {
            let client = reqwest::Client::builder()
                .no_proxy()
                .http2_prior_knowledge()
                .build()
                .unwrap();
            client.get(url).send().await.unwrap().status()
        });
        assert_eq!(status, 200);
        assert_requests(&peer.finish(), GET, 2);

        // Retries are off: the transport does not send the GET again either.
        let peer = Peer::start(refusal, 1);
        let url = peer.url();
        let result = run(|failed| async move {
            let core = Core::<()>::default().with_transport(watched(http(), failed));
            call(&core, http::Method::GET, url, Idempotent::ByMethod).await
        });
        assert_requests(&peer.finish(), GET, 1);
        assert_transport_failure(&result.unwrap_err(), 1);

        // With a policy, the engine repeats it: each request on the wire is one attempt.
        let peer = Peer::start(refusal, 3);
        let url = peer.url();
        let result = run(|failed| async move {
            let core = Core::<()>::default()
                .with_transport(watched(http(), failed))
                .with_retry(at_once(3));
            call(&core, http::Method::GET, url, Idempotent::ByMethod).await
        });
        assert_requests(&peer.finish(), GET, 3);
        assert_transport_failure(&result.unwrap_err(), 3);

        let peer = Peer::start(refusal, 1);
        let url = peer.url();
        let status = run(|failed| async move {
            let core = Core::<()>::default()
                .with_transport(watched(http(), failed))
                .with_retry(at_once(3));
            call(&core, http::Method::GET, url, Idempotent::ByMethod).await
        });
        assert_eq!(status.unwrap(), 200);
        assert_requests(&peer.finish(), GET, 2);
    }
}

#[test]
fn c4_1_an_idempotent_post_is_repeated_only_by_the_engine() {
    for refusal in [Refusal::Stream, Refusal::GoAway] {
        let peer = Peer::start(refusal, 1);
        let url = peer.url();
        let result = run(|failed| async move {
            let core = Core::<()>::default().with_transport(watched(http(), failed));
            call(&core, http::Method::POST, url, Idempotent::Yes).await
        });
        assert_requests(&peer.finish(), POST, 1);
        assert_transport_failure(&result.unwrap_err(), 1);

        let peer = Peer::start(refusal, 1);
        let url = peer.url();
        let status = run(|failed| async move {
            let core = Core::<()>::default()
                .with_transport(watched(http(), failed))
                .with_retry(at_once(2));
            call(&core, http::Method::POST, url, Idempotent::Yes).await
        });
        assert_eq!(status.unwrap(), 200);
        assert_requests(&peer.finish(), POST, 2);
    }
}

#[test]
fn c7_1_a_retry_policy_set_in_with_is_replaced() {
    // `Http::with` applies Concord's rule after the closure, as it does for redirects.
    let peer = Peer::start(Refusal::Stream, 1);
    let url = peer.url();
    let result = run(|failed| async move {
        let http = Http::with(|builder| {
            builder.no_proxy().http2_prior_knowledge().retry(
                reqwest::retry::for_host("127.0.0.1").classify_fn(|attempt| {
                    if attempt.error().is_some() {
                        attempt.retryable()
                    } else {
                        attempt.success()
                    }
                }),
            )
        })
        .unwrap();
        let core = Core::<()>::default().with_transport(watched(http, failed));
        call(&core, http::Method::POST, url, Idempotent::ByMethod).await
    });
    assert_requests(&peer.finish(), POST, 1);
    assert_transport_failure(&result.unwrap_err(), 1);
}
