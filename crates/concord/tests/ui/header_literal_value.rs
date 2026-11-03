// Literal header values and template fragments are checked with `http::HeaderValue::from_bytes`
// at expansion (after Rust escapes and `{{ }}` are decoded). UTF-8 beyond ASCII is valid, as for
// the native constructor (`tests/native_keys.rs`); control bytes are not, and a hole beside an
// invalid fragment cannot repair it.
concord::api! {
    pub client Api(v: String) {
        base "http://x.test";
        GET ctl() { headers { "x-ctl" = "a\nb"; } }
        GET nul() { headers { "x-nul" = "a\0b"; } }
        GET del() { headers { "x-del" = "a\u{7f}"; } }
        GET fragment() { headers { "x-tpl" = "Bearer\r{client.v}"; } }
        GET fine() { headers { "x-utf8" = "café"; "x-raw" = r"back\slash {{literal}} {client.v}"; } }
    }
}
fn main() {}
