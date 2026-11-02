#![allow(dead_code,unused_imports)]
pub mod codec {
 pub struct Text;
 impl concord::Encode<String> for Text {
  const CONTENT_TYPE:Option<&'static str>=Some("application/x-custom");
  fn encode(v:String)->Result<concord::reqwest::Body,concord::BodyError> {Ok(format!("CUSTOM:{v}").into())}
 }
}
concord::api! {
 pub client Expected { base "http://127.0.0.1"; POST p(codec::Text b: String); }
}
mod qualified { include!("../qualified.rs"); }
mod scoped_import { include!("../scoped_import.rs"); }
mod disabled_import { include!("../disabled_import.rs"); }
mod commented_import { include!("../commented_import.rs"); }
#[cfg(test)] mod tests {
 use super::*;
 fn http()->concord::reqwest::Client { concord::reqwest::Client::builder().no_proxy().build().unwrap() }
 #[test] fn qualified() {
  let h=http(); let expected=Expected::new(h.clone()).p("value").prepare().unwrap();
  let migrated=qualified::Migrated::new(h).p("value").prepare().unwrap();
  let old=expected.request().body().unwrap().as_bytes(); let new=migrated.request().body().unwrap().as_bytes();
  println!("qualified: expected {old:?}; migrated {new:?}");
  assert_eq!(old,Some(b"CUSTOM:value".as_slice())); assert_eq!(new,old);
 }
 #[test] fn scoped_import() {
  let h=http(); let expected=Expected::new(h.clone()).p("value").prepare().unwrap();
  let migrated=scoped_import::Migrated::new(h).p("value").prepare().unwrap();
  let old=expected.request().body().unwrap().as_bytes(); let new=migrated.request().body().unwrap().as_bytes();
  println!("scoped_import: expected {old:?}; migrated {new:?}");
  assert_eq!(old,Some(b"CUSTOM:value".as_slice())); assert_eq!(new,old);
 }
 #[test] fn disabled_import() {
  let h=http(); let expected=Expected::new(h.clone()).p("value").prepare().unwrap();
  let migrated=disabled_import::Migrated::new(h).p("value").prepare().unwrap();
  let old=expected.request().body().unwrap().as_bytes(); let new=migrated.request().body().unwrap().as_bytes();
  println!("disabled_import: expected {old:?}; migrated {new:?}");
  assert_eq!(old,Some(b"CUSTOM:value".as_slice())); assert_eq!(new,old);
 }
 #[test] fn commented_import() {
  let h=http(); let expected=Expected::new(h.clone()).p("value").prepare().unwrap();
  let migrated=commented_import::Migrated::new(h).p("value").prepare().unwrap();
  let old=expected.request().body().unwrap().as_bytes(); let new=migrated.request().body().unwrap().as_bytes();
  println!("commented_import: expected {old:?}; migrated {new:?}");
  assert_eq!(old,Some(b"CUSTOM:value".as_slice())); assert_eq!(new,old);
 }
}
