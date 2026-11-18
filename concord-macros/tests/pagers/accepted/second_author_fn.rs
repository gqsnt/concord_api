#![allow(dead_code, unused_imports, unused_macros)]
use concord::page::{Call, IntoNext, Slot};
use concord::{Answer, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Batch { more: Option<bool>, next: Option<String> }
fn my_pager<C: Call<Value = Batch>>(after: impl Slot<C, Value = String>)
 -> impl Fn(C, &Answer<Batch>) -> Option<C> {
    move |call, page| page.value().next.clone().map(|next| after.set(call, next))
}

concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String, before?: String) -> Batch { query { after; before; } paginate my_pager(after); }
    }
}
fn main() {  }
