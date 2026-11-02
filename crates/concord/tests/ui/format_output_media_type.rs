// An output takes no media type: Concord sends no `Accept` and does not check the response
// `Content-Type`. A tuple after a format is a payload, not a media type (`-> json ()`).
pub struct Item;
concord::api! {
    pub client InPlace {
        base "http://x.test";
        GET item() -> json("application/vnd.api+json") Item;
    }
}
concord::api! {
    pub client Default {
        base "http://x.test";
        response json("application/vnd.api+json");
        GET item() -> Item;
    }
}
fn main() {}
