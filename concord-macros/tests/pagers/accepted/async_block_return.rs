#![allow(dead_code, unused_braces, unused_imports, unused_macros, unreachable_code, clippy::all)]
// A `return` inside a nested `async` block is that block's own: the expression is still a call at
// its tail, and takes the pager path.
use concord::{by_cursor, Json};
#[derive(serde::Deserialize)]
pub struct Page { pub next: Option<String> }
concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String) -> Page { query { after; } paginate { let _later = async { return 1u8; }; by_cursor(after, |page| page.next.clone()) }; }
    }
}
fn main() {}
