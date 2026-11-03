use crate::codec;
concord::api! {
 pub client Migrated {
  base "http://127.0.0.1";
  POST p(codec::Text b: String);
 }
}
