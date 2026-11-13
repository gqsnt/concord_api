//! C10.2 compiler assertions for each public shipped reader, builder and adapter.
use concord_extras::read::{Binary, Stream, Text};
fn clone_shared<T: Clone + Send + Sync + Unpin + 'static>() {}
#[test]
fn c10_2_shipped_reader_types_and_futures_are_send() {
    clone_shared::<Binary>();
    clone_shared::<Text>();
    clone_shared::<Stream>();
    #[cfg(feature = "json")]
    clone_shared::<concord_extras::read::Json>();
    use concord_core::Reader as _;
    fn future<T: std::future::Future + Send>(_: T) {}
    future(Binary.read(concord_core::reply(200, "").into()));
    future(Text.read(concord_core::reply(200, "").into()));
    future(Stream.read(concord_core::reply(200, "").into()));
    #[cfg(feature = "json")]
    future(<concord_extras::read::Json as concord_core::Reader<
        serde_json::Value,
    >>::read(
        &concord_extras::read::Json,
        concord_core::reply(200, "null").into(),
    ));
}
#[cfg(feature = "multipart")]
#[test]
fn c2_16_multipart_types_hold_their_move_contracts() {
    fn shared<T: Send + Sync + Unpin + 'static>() {}
    shared::<concord_extras::Multipart>();
    shared::<concord_extras::MultipartError>();
}
#[cfg(feature = "tower")]
#[test]
fn c10_2_tower_is_send_when_its_service_is_send() {
    // Tower's public documentation requires its service to be Send, not Sync or Clone.
    fn send<T: Send + Unpin + 'static>() {}
    send::<concord_extras::tower::Tower<()>>();
}
