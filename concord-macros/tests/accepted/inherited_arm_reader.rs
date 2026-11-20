use std::sync::atomic::{AtomicUsize, Ordering};

static RECEIVED: AtomicUsize = AtomicUsize::new(0);

#[allow(non_upper_case_globals)]
const zone: fn(u8) = |key| {
    RECEIVED.fetch_add(usize::from(key), Ordering::Relaxed);
};

fn select(callback: fn(u8), key: &u8) -> concord::Json {
    callback(*key);
    concord::Json
}

concord::api! {
    client Service(key: u8) {
        base "https://example.invalid";
        read select(zone, key);

        scope zone {
            GET bare() -> u8 { status { 2xx { body } => body } }
            GET braced() -> u8 { status { 2xx { body } => { body } } }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let calls = Service::new(7u8).zone();
    assert_eq!(
        calls
            .bare()
            .read_response(concord::reply(200, "21"))
            .await
            .unwrap(),
        21
    );
    assert_eq!(
        RECEIVED.swap(0, Ordering::Relaxed),
        7,
        "bare arm reader callback"
    );
    assert_eq!(
        calls
            .braced()
            .read_response(concord::reply(200, "22"))
            .await
            .unwrap(),
        22
    );
    assert_eq!(
        RECEIVED.load(Ordering::Relaxed),
        7,
        "braced arm reader callback"
    );
}
