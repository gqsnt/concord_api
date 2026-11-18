#![allow(dead_code, unused_imports, unused_macros)]
use concord::page::{Call, IntoNext, Slot};
use concord::{Answer, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Batch { more: Option<bool>, next: Option<String> }

concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String, before?: String) -> Batch { query { after; before; } paginate const { |call, page| page.value().next.as_ref().map(|_| call) }; }
    }
}
fn main() {  }
