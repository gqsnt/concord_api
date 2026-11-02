use crate::codec::Text;
mod unrelated { #[allow(unused_imports)] use concord::Text; }
use crate::codec;
concord::api! {
 pub client Migrated {
  base "http://127.0.0.1";
  POST p(text b: String);
 }
}
