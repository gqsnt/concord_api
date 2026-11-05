use concord_core::reqwest;
use http_body::Body;
use std::{
    future::Future,
    pin::{Pin, pin},
    task::{Context, Poll, Waker},
};
pub fn run<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..1_000_000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("future failed to complete");
}
pub fn once<F: Future>(f: F) -> Poll<F::Output> {
    pin!(f)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}
pub fn drain(mut b: reqwest::Body) -> Result<Vec<u8>, reqwest::Error> {
    let mut bytes = Vec::new();
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..1_000_000 {
        match Pin::new(&mut b).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(f))) => {
                if let Ok(d) = f.into_data() {
                    bytes.extend(d);
                }
            }
            Poll::Ready(Some(Err(e))) => return Err(e),
            Poll::Ready(None) => return Ok(bytes),
            Poll::Pending => (),
        }
    }
    panic!("body failed to complete");
}
