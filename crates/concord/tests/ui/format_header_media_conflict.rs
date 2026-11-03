// 8c: a media type declared for the body and a `content-type` header contribution surviving
// composition on the endpoint (conditional ones included) are rejected: keep one. Writing the
// endpoint's `request` statement without a media type, or a `remove "content-type"` that drops
// every inherited contribution, compiles (`tests/formats.rs`).
concord::api! {
    pub client InPlace {
        base "http://x.test";
        headers { "content-type" = "text/csv"; }
        POST save(body p: String) { request concord::Text "text/plain; charset=utf-8"; }
    }
}
concord::api! {
    pub client BodyMedia(header("content-type") ct?: concord::reqwest::header::HeaderValue) {
        base "http://x.test";
        request concord::Text;
        POST save(body p: String) { request concord::Text "text/markdown"; }
    }
}
concord::api! {
    pub client FromDefault(header("content-type") ct?: concord::reqwest::header::HeaderValue) {
        base "http://x.test";
        request concord::Json "application/vnd.api+json";
        POST save(body p: String);
        POST plain(body p: String) { request concord::Text; }
        scope clean {
            headers { remove "content-type"; }
            POST save(body p: String);
        }
    }
}
concord::api! {
    pub client Conditional(header("x-ct") ct?: String) {
        base "http://x.test";
        scope s(v?: String) {
            headers { "content-type" = "text/csv"; }
            POST save(body p: String) {
                request concord::Text "text/plain";
                headers { override "content-type" = s.v else inherit; }
            }
        }
    }
}
fn main() {}
