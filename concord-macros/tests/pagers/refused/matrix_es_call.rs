#![allow(dead_code, unused_imports, unused_macros)]
use concord::page::{Call, IntoNext, Slot};
use concord::{Answer, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Batch { more: Option<bool>, next: Option<String> }
fn wrapping<C: Call<Value = Vec<String>>>(from: impl Slot<C, Value = u32>, size: impl Slot<C, Value = u32>)
 -> impl Fn(C, &Answer<Vec<String>>) -> Result<Option<C>, concord::Fail<C::Problem>> {
    let pager = concord::by_offset_limit(from, size);
    move |call, page| pager(call, page)
}

concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String, before?: String) -> Batch { query { after; before; } paginate |call, page| page.value().next.as_ref().map(|_| call); }
    }
}
fn main() {  }
