use concord::api;

api! {
    client ScopeCollisionApi {
        base "https://example.test/";
        scope item_request() {
            GET item() -> () via concord::Empty;
        }
    }
}

fn main() {}
