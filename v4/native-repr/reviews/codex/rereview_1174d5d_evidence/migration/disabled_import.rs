use crate::codec::Text;
#[cfg(any())] use concord::Text;
use crate::codec;
concord::api! {
 pub client Migrated {
  base "http://127.0.0.1";
  POST p(text b: String);
 }
}
