// An output takes no media type: Concord sends no `Accept` and does not check the response
// `Content-Type`. `format` sets both directions at once and takes none either; the request media
// type is written on `request`.
pub struct Item;
concord::api! {
    pub client OnTheEndpoint {
        base "http://x.test";
        GET item() -> Item { response json "application/vnd.api+json"; }
    }
}
concord::api! {
    pub client OnTheClient {
        base "http://x.test";
        response json "application/vnd.api+json";
        GET item() -> Item;
    }
}
concord::api! {
    pub client FormatOnTheEndpoint {
        base "http://x.test";
        POST item(body item: Item) -> Item { format json "application/vnd.api+json"; }
    }
}
concord::api! {
    pub client FormatOnTheClient {
        base "http://x.test";
        format json "application/vnd.api+json";
        POST item(body item: Item) -> Item;
    }
}
fn main() {}
