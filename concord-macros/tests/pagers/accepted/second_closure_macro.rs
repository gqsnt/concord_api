#![allow(dead_code, unused_imports, unused_macros)]
use concord::page::{Call, IntoNext, Slot};
use concord::{Answer, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Batch { more: Option<bool>, next: Option<String> }
macro_rules! make_pager { ($slot:expr) => { move |call, page| page.value().next.clone().map(|next| $slot.set(call, next)) }; }

concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String, before?: String) -> Batch { query { after; before; } paginate make_pager!(after); }
    }
}
fn main() {  }
