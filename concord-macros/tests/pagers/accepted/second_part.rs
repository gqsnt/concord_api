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
        part pager(after?: String, before?: String) { query { after; before; } paginate move |call, page| page.value().next.clone().map(|next| after.set(call, next)); }
        GET list(..pager) -> Batch {}
    }
}
fn main() {  }
