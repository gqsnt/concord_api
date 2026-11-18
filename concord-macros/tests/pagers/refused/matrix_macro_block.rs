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

type C = probe::Call<probe::List>;
type Ptr = fn(C, &Answer<Page>) -> Option<C>;
const FP: Ptr = next_page;
static STATIC_FP: Ptr = next_page;
fn get_ptr() -> Ptr { FP }
fn tok(p: &Page) -> Option<String> { p.next.clone() }
fn create<C: Call>(f: impl Fn(C, &Answer<C::Value>) -> Option<C> + Send + 'static)
 -> impl Fn(C, &Answer<C::Value>) -> Option<C> + Send + 'static { f }
macro_rules! nested { ($e:expr) => { $e }; }
macro_rules! meth { ($slot:expr) => { concord::by_cursor($slot, tok).has_more(|_| true) }; }
macro_rules! identity_method { ($slot:expr) => { concord::by_cursor($slot, tok).same() }; }
trait Same { fn same(self) -> Self; }
impl<T> Same for T { fn same(self) -> Self { self } }


concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String) -> Page { query { after; } paginate { shipped!(after) }; }
    }
}
fn main() {}
