//! Each module is a description AFTER the help of the macro was applied, as typed. All are expected to compile.
#![allow(dead_code, unused_imports, non_upper_case_globals, missing_docs)]
pub mod t {
    pub use concord::{by_link, by_offset, json, Base, Json, Secret};
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct Task;
    #[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
    pub struct E;
    pub fn unpack(v: Option<u64>) -> u64 {
        v.unwrap_or(0)
    }
    pub struct Limit;
    impl Limit {
        pub fn new(_: &u32) -> Self {
            Limit
        }
    }
    impl concord::Middleware for Limit {
        async fn handle(
            &self,
            request: concord::reqwest::Request,
            info: concord::transport::TryInfo,
            next: concord::middleware::Next<'_>,
        ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
            next.send(request, info).await
        }
    }
}
// 02: rename this child to `fetch_get`
pub mod a02 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; scope things(get?: u32) { query { get; } GET fetch_get(); } } }
}
// 06: write `client Tasks`
pub mod a06 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; GET get(); } }
}
// 08: rename `one_2` to `one_2_other`
pub mod a08 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; GET one2(); GET one_2_other(); } }
}
// 10: write `id: String` ; write `ids: Vec<String>` ; write `f: Option<String>`
pub mod a10 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; GET get(id: String, ids: Vec<String>, f: Option<String>) { path "{id}"; query { ids; f; } } } }
}
// 12: write `{client.tenant}` ; write `"x" = client.x;` ; write `base client.url;`
pub mod a12 {
    use super::t::*;
    concord::api! { client Tasks(tenant: String, x: String, url: Base) { scope s { base client.url; GET get() { path "{client.tenant}"; query { "x" = client.x; } } } } }
}
// 13: declare `id: String`, without `?` ; declare `url: Base`, without `?`
pub mod a13 {
    use super::t::*;
    concord::api! { client Tasks(url: Base) { base url; GET get(id: String) { path "{id}"; } } }
}
// 15: write `GET list();` ; write `GET status() -> u64 { path "s"; }`
pub mod a15 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; read Json; GET list(); GET status() -> u64 { path "s"; } } }
}
// 16: move `read Json;` before the first child
pub mod a16 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; read Json; GET get(); } }
}
// 19: write `METHOD "PROPFIND" find();` ; with a block
pub mod a19 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; METHOD "PROPFIND" find(); METHOD "PROPFIND" find2(d: String) { path "{d}"; } } }
}
// 21: write `override "accept" = "c/d";` ; write `override 404 => 0`
pub mod a21 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; read Json; headers { "accept" = "a/b"; } status { 4xx { body } => { let e: E = body; let _ = e; 7 } } GET get() -> u64 { headers { override "accept" = "c/d"; } status { override 404 => 0 } } } }
}
// 22: write `"accept" = "c/d";` ; write `404 => None`
pub mod a22 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; GET get() -> Option<u64> { read Json; headers { "accept" = "c/d"; } status { 404 => None } } } }
}
// 23: delete `remove 404`
pub mod a23 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; GET get() { status { } } } }
}
// 24: write `path "items";` ; write `headers { override "a" = "2"; "b" = "3"; }`
pub mod a24 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; headers { "a" = "1"; } GET get() { path "items"; headers { override "a" = "2"; "b" = "3"; } } } }
}
// 28: write `path "items"; query { "count" = "5"; }`
pub mod a28 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; GET get() { path "items"; query { "count" = "5"; } } } }
}
// 29: write `path "files/{key}.json";` ; write `"https://{h}.example"` ; write `"v{k}"`
pub mod a29 {
    use super::t::*;
    concord::api! { client Tasks(h: String) { base "https://{h}.example"; GET get(key: String, k: String) { path "files/{key}.json"; headers { "x-k" = "v{k}"; } } } }
}
// 34: write `200 { body: bodi } => bodi`
pub mod a34 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; read Json; GET get() -> Task { status { 200 { body: bodi } => bodi } } } }
}
// 35: did you mean `..page`?
pub mod a35 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; part page(limit?: u32) { query { limit; } } GET list(..page); GET other(..page); } }
}
// 36: add `..keyed` to a signature
pub mod a36 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; part keyed(k: String) { headers { "idempotency-key" = k; } } GET list(..keyed); } }
}
// 40: declare `per_second: u32`, without `?`
pub mod a40 {
    use super::t::*;
    concord::api! { client Tasks(per_second: u32) { base "https://a.example"; middleware Limit::new(per_second); GET get(); } }
}
// 41: write `"user-agent" = user_agent;`
pub mod a41 {
    use super::t::*;
    concord::api! { client Tasks(user_agent: String) { base "https://a.example"; headers { "user-agent" = user_agent; } GET get(); } }
}
// 44: also write `override 200 { body } => body`, and `override 201 => 1` ; or remove both
pub mod a44 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; read Json; status { 200 { body } => body, 201 => 1 } GET get() -> u64 { status { override 2xx => 1, override 200 { body } => body, override 201 => 1 } } GET other() -> u64 { status { override 2xx => 1, remove 200, remove 201 } } } }
}
// 46: add `status { override 2xx { body } => Some(unpack(body)) }` to this endpoint ; in this `status` block, add `override 200 { body, status: s } => Ok({ let _ = s; unpack(body) })`
pub mod a46 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; read Json; status { 2xx { body } => unpack(body) } GET find() -> Option<u64> { status { override 2xx { body } => Some(unpack(body)) } } } }
}
pub mod a46b {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; read Json; status { 200 { body, status: s } => { let _ = s; unpack(body) } } GET find() -> Result<u64, E> { status { 409 { body } => Err(body), override 200 { body, status: s } => Ok({ let _ = s; unpack(body) }) } } } }
}
// 14 (part): put `#[cfg(test)]` on a scope or endpoint
pub mod a14 {
    use super::t::*;
    concord::api! { client Tasks { base "https://a.example"; part p(a?: u32) { query { a; } } #[cfg(test)] GET get(..p); } }
}
// 20 (middleware): move `middleware Limit::new(per_second);` into the client block
pub mod a20 {
    use super::t::*;
    concord::api! { client Tasks(per_second: u32) { base "https://a.example"; middleware Limit::new(per_second); scope s { GET get(); } } }
}

#[test]
fn c1_18_a70_fable_reader_literal_help_repairs_compile_through_rustc() {
    // Every module above is the reader's literal application of a help. The
    // compiler checks their method bodies even though this test sends nothing.
    assert_eq!(
        include_str!("fable_helps.rs")
            .lines()
            .filter(|l| l.starts_with("pub mod a"))
            .count(),
        25
    );
}
