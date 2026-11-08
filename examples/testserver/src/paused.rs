//! Timeouts on a paused clock.
//!
//! A test that makes a call time out gives the client a short timeout and has the server hold its
//! answer. In real time that timeout also bounds everything else the test needs to happen first:
//! a token request made before the call, the delivery of the request itself, the response head
//! before a stalled body, the attempts and follow-up calls that should succeed. On a loaded
//! machine any of them can run out first, and the test then fails with the wrong error or the
//! wrong request count.
//!
//! Such tests run on Tokio's paused clock (`#[tokio::test(start_paused = true)]`) and drive their
//! calls with [`drive`], [`time_out`], [`time_out_in_body`] or
//! [`time_out_when`]. The runtime is kept busy while a call is driven, so virtual time never jumps
//! ahead by itself while loopback I/O is in flight; it moves only once the server has received the
//! request that is meant to time out and is holding its reply
//! ([`TestServer::held_replies`]: a delayed or gated head, or, for [`time_out_in_body`], a stalled
//! body after its head). Everything else completes while virtual time stands still, and a reply
//! the server does not hold is never timed out. The server's own delays ([`Reply::delay`](crate::Reply::delay), chunk
//! delays) run on the same virtual clock, so a reply held longer than the client's timeout stays
//! late however slowly the machine runs.
//!
//! The timeouts must be Tokio timers: reqwest's client timeouts, `tokio::time::timeout`, and SDK
//! deadlines measured with `tokio::time::Instant`. A deadline measured with `std::time::Instant`
//! keeps running in real time.

use std::future::Future;
use std::pin::{Pin, pin};
use std::task::Poll;
use std::time::{Duration, Instant};

use crate::{Recorded, TestServer};

/// Real time a driven call may take before the test reports it as stuck.
pub const STUCK: Duration = Duration::from_secs(10);

/// Timer deadlines are rounded up to the next millisecond.
const TICK: Duration = Duration::from_millis(1);

/// Real time, and polls, [`time_out_in_body`] gives the client to read a head the server has
/// written before virtual time moves.
const SETTLE: Duration = Duration::from_millis(50);
const SETTLE_POLLS: usize = 1000;

/// Polls `call` until it completes (`Some`) or `until()` holds (`None`), yielding between polls:
/// loopback I/O and the server's tasks progress while virtual time stands still. Panics if that
/// takes [`STUCK`] of real time.
pub async fn drive_until<F: Future>(
    mut call: Pin<&mut F>,
    mut until: impl FnMut() -> bool,
) -> Option<F::Output> {
    let started = Instant::now();
    loop {
        if let Poll::Ready(output) = poll_once(call.as_mut()).await {
            return Some(output);
        }
        if until() {
            return None;
        }
        assert!(
            started.elapsed() < STUCK,
            "a driven call did not finish within {STUCK:?} of real time (an attempt the server \
             holds that the test did not expect to time out?)"
        );
        tokio::task::yield_now().await;
    }
}

/// Polls `call` once.
pub async fn poll_once<F: Future>(mut call: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx))).await
}

/// Runs `call` to completion while virtual time stands still: no timeout can run out.
pub async fn drive<F: Future>(call: F) -> F::Output {
    let mut call = pin!(call);
    drive_until(call.as_mut(), || false)
        .await
        .expect("only completion ends an unconditional drive")
}

/// Runs `call` with Tokio's clock paused, then resumes it: every timer, a caller's bound included,
/// stands still while the call's loopback exchanges complete. For bounds that only guard against
/// a call waiting for something that never comes (a stalled body, a sleep): such a call is
/// reported as stuck after [`STUCK`] of real time. Needs a current-thread runtime whose clock is
/// not already paused.
pub async fn frozen<F: Future>(call: F) -> F::Output {
    tokio::time::pause();
    let output = drive(call).await;
    tokio::time::resume();
    output
}

fn received(server: &TestServer, matches: &impl Fn(&Recorded) -> bool) -> usize {
    server.requests().iter().filter(|r| matches(r)).count()
}

/// Runs `call` while virtual time stands still until `ready()` holds (what the test needs to have
/// happened before the timeout, observed on the server or the client), then moves virtual time
/// past `timeout` and runs the call to completion. A call that ends before `ready()` holds is
/// returned as it ended.
pub async fn time_out_when<F: Future>(
    ready: impl FnMut() -> bool,
    timeout: Duration,
    call: F,
) -> F::Output {
    let mut call = pin!(call);
    if let Some(ended) = drive_until(call.as_mut(), ready).await {
        return ended;
    }
    tokio::time::advance(timeout + TICK).await;
    drive(call).await
}

/// Runs `call`, which sends one request matching `matches` that the server answers later than
/// the client's `timeout` (the request is not repeated). Once the server has received that
/// request and is holding its reply, virtual time moves past `timeout`, so it times out after it
/// reached the server, never before; everything else (a token request, the answers to other
/// requests) completes while time stands still. A call that ends first (the server answered, or
/// the request never arrived) is returned as it ended.
pub async fn time_out<F: Future>(
    server: &TestServer,
    matches: impl Fn(&Recorded) -> bool,
    timeout: Duration,
    call: F,
) -> F::Output {
    let mut call = pin!(call);
    let before = received(server, &matches);
    let held_before = server.held_replies();
    let held = || received(server, &matches) > before && server.held_replies() > held_before;
    if let Some(ended) = drive_until(call.as_mut(), held).await {
        return ended;
    }
    tokio::time::advance(timeout + TICK).await;
    drive(call).await
}

/// Runs `call`, which sends one request matching `matches` whose response head the server sends
/// at once and whose body then stalls ([`Body::Stall`](crate::Body::Stall)). Once the server has
/// received the request, written the head and is holding the body, and the client has had
/// [`SETTLE`] of real time and [`SETTLE_POLLS`] polls to read the head,
/// virtual time moves past the client's `timeout`, so the timeout ends the body, never the wait
/// for the head; everything else completes while time stands still.
///
/// The settling is the one real-time allowance: the head is already in the kernel when the server
/// has written it, and only a loopback delivery delayed by more than [`SETTLE`] after that write
/// could still let the timeout fire before the client read it.
pub async fn time_out_in_body<F: Future>(
    server: &TestServer,
    matches: impl Fn(&Recorded) -> bool,
    timeout: Duration,
    call: F,
) -> F::Output {
    let mut call = pin!(call);
    let before = received(server, &matches);
    let held_before = server.held_replies();
    let mut heads_before = server.heads_sent();
    let written = drive_until(call.as_mut(), || {
        if received(server, &matches) <= before {
            heads_before = server.heads_sent();
            false
        } else {
            server.heads_sent() > heads_before && server.held_replies() > held_before
        }
    })
    .await;
    if let Some(ended) = written {
        return ended;
    }
    let settling = Instant::now();
    let mut polls = 0;
    if let Some(ended) = drive_until(call.as_mut(), || {
        polls += 1;
        polls >= SETTLE_POLLS && settling.elapsed() >= SETTLE
    })
    .await
    {
        return ended;
    }
    tokio::time::advance(timeout + TICK).await;
    drive(call).await
}
