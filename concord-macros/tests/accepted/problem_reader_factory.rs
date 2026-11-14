// macro.md 4: the problem-reader expression runs even when evidence is cut,
// although the reader's read method is then never called by the engine.
use std::sync::atomic::{AtomicUsize, Ordering};
static FACTORIES: AtomicUsize = AtomicUsize::new(0);
static READS: AtomicUsize = AtomicUsize::new(0);
struct Counting;
fn factory() -> Counting { FACTORIES.fetch_add(1, Ordering::SeqCst); Counting }
impl concord::read::Reader<u32> for Counting {
    async fn read(&self, _: concord::read::Incoming) -> Result<u32, concord::read::ReadError> {
        READS.fetch_add(1, Ordering::SeqCst); Ok(0)
    }
}
concord::api! {
    client Audit {
        base "https://audit.test";
        problem u32 read factory();
        GET get();
    }
}
fn main() {
    let call = Audit::new().get();
    let mut answer = std::pin::pin!(call.read_response(concord::reply(400, "x".repeat(65537))));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let std::task::Poll::Ready(Err(error)) = std::future::Future::poll(answer.as_mut(), &mut cx) else {
        panic!("expected the ready refusal");
    };
    assert_eq!(error.kind(), concord::error::Kind::Refused);
    assert_eq!(FACTORIES.load(Ordering::SeqCst), 1);
    assert_eq!(READS.load(Ordering::SeqCst), 0);
    println!("reader factories: 1; actual reads: 0");
}
