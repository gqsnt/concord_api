//! A linked, unoptimized consumer for scripts/check-tracing-footprint.py.

concord::api! {
    client Footprint {
        base "http://footprint.invalid/C9ROUTEBASE";
        GET item(id: u32) { path "C9ROUTESEGMENT/{id}"; }
    }
}

fn main() {
    #[cfg(feature = "tracing")]
    assert_eq!(
        std::mem::size_of::<concord_core::__private::Route>(),
        std::mem::size_of::<&str>()
    );
    #[cfg(not(feature = "tracing"))]
    assert_eq!(std::mem::size_of::<concord_core::__private::Route>(), 0);
    let api = Footprint::new(concord::reqwest::Client::new());
    let prepared = api.item(std::hint::black_box(7)).prepare().unwrap();
    // Retain request metadata and execution code, without making a network request.
    let _future = std::hint::black_box(prepared.receive());
}
