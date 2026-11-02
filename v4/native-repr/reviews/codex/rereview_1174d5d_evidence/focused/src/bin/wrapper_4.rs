#![allow(dead_code, unused_imports)]
use wire::{ToHeaderValue, reqwest::header::HeaderValue};
use std::{borrow::Cow, sync::Arc, rc::Rc};
#[derive(Clone)]
pub struct Dual(pub &'static str);
impl ToHeaderValue for Dual {
 fn to_header_value(&self)->Option<HeaderValue> { let mut h=HeaderValue::from_static(self.0); h.set_sensitive(true); Some(h) }
}
impl std::fmt::Display for Dual { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str(self.0) } }
type Alias = Dual;

wire::api! {
pub client Api { base "http://example.test"; GET go(v: Cow<'static, Dual>) { path "{v}"; query { "q" = v; } headers { "x-dual" = v; "x-copy" = "{v}"; } } }
}
fn main() {}
