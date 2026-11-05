struct Wire;
impl<T> concord::Encode<T> for Wire {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: T) -> Result<concord::reqwest::Body, concord::CodecError> {
        Ok("callback".into())
    }
}
concord::api! { pub client CallbackApi {
    base "http://127.0.0.1";
    POST send_callback(body callback: for<'__concord> fn(&'__concord str)) {
        path "callback"; request Wire;
    }
} }
fn main() {
    let callback: for<'x> fn(&'x str) = |_| {};
    let request = CallbackApi::new(concord::reqwest::Client::new()).send_callback(callback);
    let _ = request.clone().prepare().unwrap();
}
