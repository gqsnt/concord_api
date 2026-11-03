// A media type follows the format it belongs to: a statement replaces the inherited format and
// media type together, so `request "m";` alone is an error, on the endpoint and on an owner.
concord::api! {
    pub client OnTheEndpoint {
        base "http://x.test";
        request concord::Json;
        POST a(body b: String) { request "application/merge-patch+json"; }
    }
}
concord::api! {
    pub client OnTheClient {
        base "http://x.test";
        request "application/vnd.api+json";
        POST a(body b: String);
    }
}
concord::api! {
    pub client OnAScope {
        base "http://x.test";
        request concord::Json;
        scope s {
            request "application/vnd.api+json";
            POST a(body b: String);
        }
    }
}
fn main() {}
