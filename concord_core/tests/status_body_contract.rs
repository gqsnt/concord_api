use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};

const CAP: usize = 16;

#[derive(Eq, PartialEq)]
struct SecretBytes(Vec<u8>);

impl fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "<redacted:{} bytes>", self.0.len())
    }
}

#[derive(Debug, Eq, PartialEq)]
enum CaptureState {
    Complete(SecretBytes),
    OmittedKnownTooLarge,
    OmittedUnknownTooLarge,
    OmittedReadFailure,
    OmittedDeadline,
    UnreadRecoverableChallenge,
}

impl CaptureState {
    fn label(&self) -> &'static str {
        match self {
            Self::Complete(_) => "complete",
            Self::OmittedKnownTooLarge => "known-too-large",
            Self::OmittedUnknownTooLarge => "unknown-too-large",
            Self::OmittedReadFailure => "read-failure",
            Self::OmittedDeadline => "deadline",
            Self::UnreadRecoverableChallenge => "recoverable-challenge-unread",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TerminalBody {
    status: u16,
    capture: CaptureState,
}

impl TerminalBody {
    fn bytes(&self) -> Option<&[u8]> {
        match &self.capture {
            CaptureState::Complete(bytes) => Some(&bytes.0),
            _ => None,
        }
    }

    fn observation(&self) -> Observation {
        Observation {
            status: self.status,
            capture: self.capture.label(),
        }
    }
}

impl fmt::Display for TerminalBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "terminal HTTP status {} ({})",
            self.status,
            self.capture.label()
        )
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    status: u16,
    capture: &'static str,
}

#[derive(Debug)]
struct ConnectionDiagnostic {
    status: u16,
    connection_id: u64,
}

#[derive(Debug)]
struct RetryDiagnostic {
    status: u16,
    logical_attempt: u8,
    physical_attempts_known: bool,
}

#[derive(Debug)]
struct TerminalError(TerminalBody);

impl fmt::Display for TerminalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

enum ProbeChunk {
    Data(Vec<u8>),
    Failure,
    Delayed(Duration, Vec<u8>),
}

struct ProbeBody {
    known_length: Option<usize>,
    chunks: VecDeque<ProbeChunk>,
    polls: Arc<AtomicUsize>,
    consumed: Arc<AtomicUsize>,
}

impl ProbeBody {
    fn new(known_length: Option<usize>, chunks: impl IntoIterator<Item = ProbeChunk>) -> Self {
        Self {
            known_length,
            chunks: chunks.into_iter().collect(),
            polls: Arc::new(AtomicUsize::new(0)),
            consumed: Arc::new(AtomicUsize::new(0)),
        }
    }

    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, ()> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        match self.chunks.pop_front() {
            Some(ProbeChunk::Data(bytes)) => Ok(Some(bytes)),
            Some(ProbeChunk::Failure) => Err(()),
            Some(ProbeChunk::Delayed(delay, bytes)) => {
                tokio::time::sleep(delay).await;
                Ok(Some(bytes))
            }
            None => Ok(None),
        }
    }
}

async fn capture_probe(
    status: u16,
    mut source: ProbeBody,
    limit: usize,
    deadline: Option<Duration>,
) -> TerminalBody {
    if source.known_length.is_some_and(|length| length > limit) {
        return TerminalBody {
            status,
            capture: CaptureState::OmittedKnownTooLarge,
        };
    }
    let started = Instant::now();
    let mut captured = Vec::new();
    loop {
        let next = if let Some(total) = deadline {
            let Some(remaining) = total.checked_sub(started.elapsed()) else {
                return TerminalBody {
                    status,
                    capture: CaptureState::OmittedDeadline,
                };
            };
            match tokio::time::timeout(remaining, source.next_chunk()).await {
                Ok(next) => next,
                Err(_) => {
                    return TerminalBody {
                        status,
                        capture: CaptureState::OmittedDeadline,
                    };
                }
            }
        } else {
            source.next_chunk().await
        };
        match next {
            Ok(Some(bytes)) => {
                let needed = limit.saturating_add(1).saturating_sub(captured.len());
                let observed = bytes.len().min(needed);
                source.consumed.fetch_add(observed, Ordering::SeqCst);
                if bytes.len() > limit.saturating_sub(captured.len()) {
                    return TerminalBody {
                        status,
                        capture: CaptureState::OmittedUnknownTooLarge,
                    };
                }
                captured.extend_from_slice(&bytes);
            }
            Ok(None) => {
                return TerminalBody {
                    status,
                    capture: CaptureState::Complete(SecretBytes(captured)),
                };
            }
            Err(()) => {
                return TerminalBody {
                    status,
                    capture: CaptureState::OmittedReadFailure,
                };
            }
        }
    }
}

async fn capture_reqwest(
    mut response: reqwest::Response,
    limit: usize,
    deadline: Option<Duration>,
) -> TerminalBody {
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|length| usize::try_from(length).map_or(true, |length| length > limit))
    {
        return TerminalBody {
            status,
            capture: CaptureState::OmittedKnownTooLarge,
        };
    }
    let started = Instant::now();
    let mut captured = Vec::new();
    loop {
        let next = if let Some(total) = deadline {
            let Some(remaining) = total.checked_sub(started.elapsed()) else {
                return TerminalBody {
                    status,
                    capture: CaptureState::OmittedDeadline,
                };
            };
            match tokio::time::timeout(remaining, response.chunk()).await {
                Ok(next) => next,
                Err(_) => {
                    return TerminalBody {
                        status,
                        capture: CaptureState::OmittedDeadline,
                    };
                }
            }
        } else {
            response.chunk().await
        };
        match next {
            Ok(Some(bytes)) => {
                if bytes.len() > limit.saturating_sub(captured.len()) {
                    return TerminalBody {
                        status,
                        capture: CaptureState::OmittedUnknownTooLarge,
                    };
                }
                captured.extend_from_slice(&bytes);
            }
            Ok(None) => {
                return TerminalBody {
                    status,
                    capture: CaptureState::Complete(SecretBytes(captured)),
                };
            }
            Err(_) => {
                return TerminalBody {
                    status,
                    capture: CaptureState::OmittedReadFailure,
                };
            }
        }
    }
}

async fn classify_challenge(
    status: u16,
    recoverable: bool,
    source: ProbeBody,
    limit: usize,
) -> TerminalBody {
    if recoverable {
        TerminalBody {
            status,
            capture: CaptureState::UnreadRecoverableChallenge,
        }
    } else {
        capture_probe(status, source, limit, None).await
    }
}

fn client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .retry(reqwest::retry::never())
        .http1_only()
        .build()
        .expect("status-body client should build")
}

#[tokio::test]
async fn small_terminal_body_is_captured_completely() {
    let server = ScriptedServer::start([ResponseScript::status(500, b"exact-small".to_vec())])
        .expect("small-body server should start");
    let response = client()
        .get(server.url("/small"))
        .send()
        .await
        .expect("small terminal response should arrive");
    let terminal = capture_reqwest(response, CAP, None).await;
    assert_eq!(terminal.status, 500);
    assert_eq!(terminal.bytes(), Some(&b"exact-small"[..]));
}

#[tokio::test]
async fn known_oversized_body_is_omitted_without_polling() {
    let source = ProbeBody::new(
        Some(CAP + 1),
        [ProbeChunk::Data(b"must-not-be-polled".to_vec())],
    );
    let polls = Arc::clone(&source.polls);
    let terminal = capture_probe(500, source, CAP, None).await;
    assert_eq!(terminal.capture, CaptureState::OmittedKnownTooLarge);
    assert_eq!(terminal.bytes(), None);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reqwest_known_oversized_body_is_omitted() {
    let server = ScriptedServer::start([ResponseScript::status(500, vec![b'k'; CAP + 1])])
        .expect("known-oversized Reqwest server should start");
    let response = client()
        .get(server.url("/known-oversized"))
        .send()
        .await
        .expect("known-oversized response head should arrive");
    assert_eq!(response.content_length(), Some((CAP + 1) as u64));

    let terminal = capture_reqwest(response, CAP, None).await;

    assert_eq!(terminal.status, 500);
    assert_eq!(terminal.capture, CaptureState::OmittedKnownTooLarge);
    assert_eq!(terminal.bytes(), None);
}

#[tokio::test]
async fn unknown_oversized_body_returns_no_prefix() {
    let marker = b"DISTINCTIVE_BODY_MARKER".to_vec();
    let source = ProbeBody::new(
        None,
        marker
            .iter()
            .copied()
            .map(|byte| ProbeChunk::Data(vec![byte])),
    );
    let polls = Arc::clone(&source.polls);
    let consumed = Arc::clone(&source.consumed);
    let terminal = capture_probe(500, source, CAP, None).await;
    assert_eq!(terminal.capture, CaptureState::OmittedUnknownTooLarge);
    assert_eq!(terminal.bytes(), None);
    assert_eq!(consumed.load(Ordering::SeqCst), CAP + 1);
    assert_eq!(polls.load(Ordering::SeqCst), CAP + 1);
    assert!(!format!("{terminal:?}").contains("DISTINCTIVE_BODY_MARKER"));
}

#[tokio::test]
async fn reqwest_unknown_oversized_body_returns_no_prefix() {
    const PREFIX: &str = "REQWEST_PREFIX_MUST_DISAPPEAR";
    let server = ScriptedServer::start([ResponseScript::Chunked {
        status: 503,
        headers: Vec::new(),
        chunks: vec![PREFIX.as_bytes().to_vec(), vec![b'z'; CAP + 1]],
    }])
    .expect("unknown-oversized Reqwest server should start");
    let response = client()
        .get(server.url("/unknown-oversized"))
        .send()
        .await
        .expect("unknown-oversized response head should arrive");
    assert_eq!(response.content_length(), None);

    let terminal = capture_reqwest(response, CAP, None).await;

    assert_eq!(terminal.status, 503);
    assert_eq!(terminal.capture, CaptureState::OmittedUnknownTooLarge);
    assert_eq!(terminal.bytes(), None);
    assert!(!format!("{terminal:?}").contains(PREFIX));
    assert!(!terminal.to_string().contains(PREFIX));
    assert!(!format!("{:?}", terminal.observation()).contains(PREFIX));
}

#[tokio::test]
async fn read_failure_preserves_status_and_returns_no_bytes() {
    let server = ScriptedServer::start([ResponseScript::Truncated {
        status: 503,
        declared_length: 64,
        prefix: b"partial-prefix".to_vec(),
    }])
    .expect("read-failure server should start");
    let response = client()
        .get(server.url("/read-failure"))
        .send()
        .await
        .expect("status should arrive before read failure");
    let terminal = capture_reqwest(response, 128, None).await;
    assert_eq!(terminal.status, 503);
    assert_eq!(terminal.capture, CaptureState::OmittedReadFailure);
    assert_eq!(terminal.bytes(), None);
}

#[tokio::test]
async fn deadline_preserves_status_and_returns_no_bytes() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 500,
        body: b"late-body-marker".to_vec(),
        gate: gate.clone(),
    }])
    .expect("deadline-body server should start");
    let response = client()
        .get(server.url("/deadline"))
        .send()
        .await
        .expect("status should arrive before body deadline");
    let terminal = capture_reqwest(response, 128, Some(Duration::from_millis(80))).await;
    gate.release();
    assert_eq!(terminal.status, 500);
    assert_eq!(terminal.capture, CaptureState::OmittedDeadline);
    assert_eq!(terminal.bytes(), None);
}

#[tokio::test]
async fn recoverable_challenge_body_is_unread() {
    let source = ProbeBody::new(Some(12), [ProbeChunk::Data(b"challenge".to_vec())]);
    let polls = Arc::clone(&source.polls);
    let terminal = classify_challenge(401, true, source, CAP).await;
    assert_eq!(terminal.capture, CaptureState::UnreadRecoverableChallenge);
    assert_eq!(terminal.bytes(), None);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn terminal_challenge_uses_bounded_complete_only_capture() {
    let small = ProbeBody::new(Some(9), [ProbeChunk::Data(b"challenge".to_vec())]);
    let terminal_small = capture_probe(401, small, CAP, None).await;
    assert_eq!(terminal_small.status, 401);
    assert_eq!(terminal_small.bytes(), Some(&b"challenge"[..]));

    let oversized = ProbeBody::new(None, [ProbeChunk::Data(vec![b'x'; CAP + 1])]);
    let terminal_large = capture_probe(403, oversized, CAP, None).await;
    assert_eq!(terminal_large.status, 403);
    assert_eq!(terminal_large.capture, CaptureState::OmittedUnknownTooLarge);
    assert_eq!(terminal_large.bytes(), None);
}

#[tokio::test]
async fn body_bytes_are_absent_from_formatting_and_observation() {
    const MARKER: &str = "BODY_BYTES_MUST_NOT_APPEAR_7F4A";
    let source = ProbeBody::new(
        Some(MARKER.len()),
        [ProbeChunk::Data(MARKER.as_bytes().to_vec())],
    );
    let terminal = capture_probe(500, source, 128, None).await;
    assert_eq!(terminal.bytes().map(<[u8]>::len), Some(MARKER.len()));
    let debug = format!("{terminal:?}");
    let display = terminal.to_string();
    let error = TerminalError(terminal);
    let error_debug = format!("{error:?}");
    let error_display = error.to_string();
    let observation = format!("{:?}", error.0.observation());
    let connection = ConnectionDiagnostic {
        status: error.0.status,
        connection_id: 7,
    };
    let retry = RetryDiagnostic {
        status: error.0.status,
        logical_attempt: 1,
        physical_attempts_known: false,
    };
    assert_eq!(connection.status, 500);
    assert_eq!(connection.connection_id, 7);
    assert_eq!(retry.status, 500);
    assert_eq!(retry.logical_attempt, 1);
    assert!(!retry.physical_attempts_known);
    for formatted in [
        debug,
        display,
        error_debug,
        error_display,
        observation,
        format!("{connection:?}"),
        format!("{retry:?}"),
    ] {
        assert!(!formatted.contains(MARKER));
    }
}

#[tokio::test]
async fn probe_deadline_discards_partial_capture() {
    let source = ProbeBody::new(
        None,
        [
            ProbeChunk::Data(b"partial".to_vec()),
            ProbeChunk::Delayed(Duration::from_millis(200), b"late".to_vec()),
        ],
    );
    let terminal = capture_probe(500, source, CAP, Some(Duration::from_millis(50))).await;
    assert_eq!(terminal.capture, CaptureState::OmittedDeadline);
    assert_eq!(terminal.bytes(), None);
}

#[tokio::test]
async fn probe_read_failure_discards_partial_capture() {
    let source = ProbeBody::new(
        None,
        [ProbeChunk::Data(b"partial".to_vec()), ProbeChunk::Failure],
    );
    let terminal = capture_probe(500, source, CAP, None).await;
    assert_eq!(terminal.capture, CaptureState::OmittedReadFailure);
    assert_eq!(terminal.bytes(), None);
}
