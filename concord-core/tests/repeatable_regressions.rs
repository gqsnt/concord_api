//! Regression tests for C12.12 to C12.14: lifetimes, cancellation order, panics and cleanup.
#![allow(clippy::unwrap_used)]
use concord_core::{bytes::Bytes, Body};
use futures_core::Stream;
use futures_util::{FutureExt, StreamExt};
use std::{
    error::Error,
    future::Future,
    io,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
struct ExplosiveStream {
    poll_panics: bool,
    drop_panics: bool,
}
impl Stream for ExplosiveStream {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        assert!(!self.poll_panics, "stream poll panic");
        Poll::Ready(Some(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "original read error",
        ))))
    }
}
impl Drop for ExplosiveStream {
    fn drop(&mut self) {
        assert!(!self.drop_panics, "stream cleanup panic");
    }
}
struct ExplosiveOpen {
    poll_panics: bool,
    drop_panics: bool,
}
impl Future for ExplosiveOpen {
    type Output = Result<ExplosiveStream, io::Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        assert!(!self.poll_panics, "opener poll panic");
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "original opening error",
        )))
    }
}
impl Drop for ExplosiveOpen {
    fn drop(&mut self) {
        assert!(!self.drop_panics, "opener cleanup panic");
    }
}
fn has_io(mut error: &(dyn Error + 'static), kind: io::ErrorKind) -> bool {
    loop {
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == kind)
        {
            return true;
        }
        let Some(source) = error.source() else {
            return false;
        };
        error = source;
    }
}
async fn assert_contained(body: Body, kind: io::ErrorKind) {
    let mut stream = body.into_stream().unwrap();
    let next = AssertUnwindSafe(stream.next()).catch_unwind().await;
    assert!(
        next.is_ok(),
        "an author panic escaped instead of a body error"
    );
    let error = next.unwrap().unwrap().unwrap_err();
    assert!(has_io(&*error, kind));
    assert!(stream.next().await.is_none());
}
#[tokio::test(start_paused = true)]
async fn c12_13_returned_read_error_survives_a_panicking_stream_destructor() {
    assert_contained(
        Body::repeatable(|| async {
            Ok::<_, io::Error>(ExplosiveStream {
                poll_panics: false,
                drop_panics: true,
            })
        }),
        io::ErrorKind::NotFound,
    )
    .await;
}
#[tokio::test(start_paused = true)]
async fn c12_13_returned_open_error_survives_a_panicking_future_destructor() {
    assert_contained(
        Body::repeatable(|| ExplosiveOpen {
            poll_panics: false,
            drop_panics: true,
        }),
        io::ErrorKind::NotFound,
    )
    .await;
}
#[tokio::test(start_paused = true)]
async fn c12_13_factory_panics_become_body_errors() {
    assert_contained(
        Body::repeatable(
            || -> std::future::Ready<Result<ExplosiveStream, io::Error>> {
                panic!("factory panic")
            },
        ),
        io::ErrorKind::Other,
    )
    .await;
}
#[tokio::test(start_paused = true)]
async fn c12_13_open_future_poll_panics_become_body_errors() {
    assert_contained(
        Body::repeatable(|| ExplosiveOpen {
            poll_panics: true,
            drop_panics: false,
        }),
        io::ErrorKind::Other,
    )
    .await;
}
#[tokio::test(start_paused = true)]
async fn c12_13_stream_poll_panics_become_body_errors() {
    assert_contained(
        Body::repeatable(|| async {
            Ok::<_, io::Error>(ExplosiveStream {
                poll_panics: true,
                drop_panics: false,
            })
        }),
        io::ErrorKind::Other,
    )
    .await;
}
#[test]
fn c12_12_nested_maps_and_concatenations_make_progress_beyond_the_poll_budget() {
    let opens = Arc::new(AtomicUsize::new(0));
    let seen = opens.clone();
    let mut body = Body::repeatable(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        async {
            Ok::<_, io::Error>(futures_util::stream::iter([Ok(Bytes::from_static(
                b"leaf",
            ))]))
        }
    });
    for _ in 0..96 {
        body = Body::concat([body]).map_stream(|s| s);
    }
    let mut stream = body.into_stream().unwrap();
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut first = None;
    for _ in 0..256 {
        if let Poll::Ready(piece) = stream.as_mut().poll_next(&mut cx) {
            first = piece;
            break;
        }
    }
    assert_eq!(
        opens.load(Ordering::SeqCst),
        1,
        "every poll restarted at the same wrappers without reaching the leaf"
    );
    assert_eq!(first.unwrap().unwrap(), b"leaf"[..]);
}

struct DropTick(Arc<AtomicUsize>);
impl Drop for DropTick {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct CountedRead {
    dropped: Arc<AtomicUsize>,
    poll_panics: bool,
    drop_panics: bool,
}
impl Stream for CountedRead {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        assert!(!self.poll_panics, "read poll panic");
        Poll::Ready(Some(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "returned read error",
        ))))
    }
}
impl Drop for CountedRead {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
        assert!(!self.drop_panics, "read cleanup panic");
    }
}
struct CountedOpen {
    dropped: Arc<AtomicUsize>,
    read_dropped: Arc<AtomicUsize>,
    mode: u8,
}
impl Future for CountedOpen {
    type Output = Result<CountedRead, io::Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        assert!(self.mode != 1, "opening poll panic");
        Poll::Ready(if self.mode == 2 {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "returned opening error",
            ))
        } else {
            Ok(CountedRead {
                dropped: self.read_dropped.clone(),
                poll_panics: self.mode == 3,
                drop_panics: self.mode == 4,
            })
        })
    }
}
impl Drop for CountedOpen {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
        assert!(self.mode != 2, "opening cleanup panic");
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_panic_containment_destroys_factory_future_stream_and_payload_instead_of_leaking() {
    let factory = Arc::new(AtomicUsize::new(0));
    let payload = Arc::new(AtomicUsize::new(0));
    let held = DropTick(factory.clone());
    let thrown = payload.clone();
    let body = Body::repeatable(
        move || -> std::future::Ready<Result<CountedRead, io::Error>> {
            let _capture = &held;
            std::panic::panic_any(DropTick(thrown.clone()));
        },
    );
    assert_contained(body, io::ErrorKind::Other).await;
    assert_eq!(factory.load(Ordering::SeqCst), 1);
    assert_eq!(payload.load(Ordering::SeqCst), 1);
    for mode in 1..=4 {
        let opened = Arc::new(AtomicUsize::new(0));
        let read = Arc::new(AtomicUsize::new(0));
        let future = opened.clone();
        let stream = read.clone();
        let body = Body::repeatable(move || CountedOpen {
            dropped: future.clone(),
            read_dropped: stream.clone(),
            mode,
        });
        assert_contained(
            body,
            if mode == 2 || mode == 4 {
                io::ErrorKind::NotFound
            } else {
                io::ErrorKind::Other
            },
        )
        .await;
        assert_eq!(
            opened.load(Ordering::SeqCst),
            1,
            "opening future leaked for stage {mode}"
        );
        assert_eq!(
            read.load(Ordering::SeqCst),
            usize::from(mode >= 3),
            "stream leaked for stage {mode}"
        );
    }
}
