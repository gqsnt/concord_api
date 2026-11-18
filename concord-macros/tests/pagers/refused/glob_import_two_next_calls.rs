#![allow(dead_code, unused_braces, unused_imports, unused_macros, unreachable_code, clippy::all)]
use concord::page::{Call, Slot};
use concord::{Answer, Json};
#[derive(serde::Deserialize)]
pub struct Page { pub next: Option<String> }
fn wrap<F>(f: F) -> F { f }
fn next_page<C: Call<Value = Page>>(call: C, page: &Answer<Page>) -> Option<C> { page.value().next.as_ref().map(|_| call) }
fn author_pager<C: Call<Value = Page>>(after: impl Slot<C, Value = String>) -> impl Fn(C, &Answer<Page>) -> Option<C> + Send + 'static {
    move |call, page| page.value().next.clone().map(|next| after.set(call, next))
}
macro_rules! make_pager { ($slot:expr) => { move |call, page| page.value().next.clone().map(|next| $slot.set(call, next)) }; }
macro_rules! make_empty { () => { |_, _| None }; }
macro_rules! last_page { () => { |call, page| if page.value().next.is_none() { None } else { Some(call) } }; }
macro_rules! shipped { ($slot:expr) => { concord::by_cursor($slot, |page: &Page| page.next.clone()) }; }
use concord::page::*;
/// A trait of the author's with a method named as `Pager`'s.
trait LocalMove {
    fn next_call(&mut self, call: probe::Call<probe::List>, page: &Answer<Page>) -> Option<probe::Call<probe::List>>;
}
impl<F> LocalMove for F where F: FnMut(probe::Call<probe::List>, &Answer<Page>) -> Option<probe::Call<probe::List>> {
    fn next_call(&mut self, call: probe::Call<probe::List>, page: &Answer<Page>) -> Option<probe::Call<probe::List>> { self(call, page) }
}

concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String) -> Page { query { after; } paginate { let mut f = |call: probe::Call<probe::List>, page: &Answer<Page>| page.value().next.as_ref().map(|_| call); move |call, page| f.next_call(call, page) }; }
    }
}
fn main() {}
