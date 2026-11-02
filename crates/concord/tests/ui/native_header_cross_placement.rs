// H10: a native `HeaderValue` is not text. Reusing it in a query, a path template or a header
// template is a compile error at the value's type, never an implicit (lossy) decoding.
use concord::reqwest::header::HeaderValue;

concord::api! {
    pub client Api(header("accept") accept: HeaderValue) {
        base "http://x.test";
        GET query(query("accept") a: HeaderValue);
        GET path(a: HeaderValue) { path "items/{a}"; }
        GET template(a: HeaderValue) { headers { "x-a" = "v={a}"; } }
    }
}
fn main() {}
