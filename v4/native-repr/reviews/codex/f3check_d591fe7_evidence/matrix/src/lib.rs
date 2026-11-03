#![allow(dead_code, unused_imports, non_camel_case_types)]
extern crate concord as c4;
mod wire;
pub mod codec {
    macro_rules! custom {
        ($name:ident) => {
            pub struct $name;
            impl crate::c4::Encode<String> for $name {
                const CONTENT_TYPE: Option<&'static str> = Some("application/x-custom");
                fn encode(v: String) -> Result<crate::c4::reqwest::Body, crate::c4::BodyError> { Ok(format!("CUSTOM:{v}").into()) }
            }
            impl crate::c4::Decode<String> for $name {
                fn decode(v: crate::c4::bytes::Bytes) -> Result<String, crate::c4::BodyError> { Ok(format!("CUSTOM:{}", String::from_utf8(v.to_vec()).unwrap())) }
            }
            impl crate::c4::Encode<Vec<u8>> for $name {
                const CONTENT_TYPE: Option<&'static str> = Some("application/x-custom");
                fn encode(v: Vec<u8>) -> Result<crate::c4::reqwest::Body, crate::c4::BodyError> { Ok([b"CUSTOM:".as_slice(), &v].concat().into()) }
            }
            impl crate::c4::Decode<Vec<u8>> for $name {
                fn decode(v: crate::c4::bytes::Bytes) -> Result<Vec<u8>, crate::c4::BodyError> { Ok([b"CUSTOM:".as_slice(), v.as_ref()].concat()) }
            }
        }
    }
    custom!(Text); custom!(Json); custom!(Binary); custom!(Form);
}

pub type Text = codec::Text;
pub mod f3_qualified;
pub mod f3a_scoped_import;
pub mod f3a_cfg_disabled;
pub mod f3a_line_comment;
pub mod f3a_block_comment;
pub mod cfg_selected_custom;
pub mod glob_custom;
pub mod aliased_import;
pub mod self_path;
pub mod super_path;
pub mod crate_path;
pub mod nested_module;
pub mod nested_super;
pub mod downstream_form;
pub mod bare_builtin_text;
pub mod qualified_builtin_text;
pub mod absolute_builtin_text;
pub mod local_concord_text;
pub mod absolute_under_shadow_text;
pub mod bare_builtin_json;
pub mod qualified_builtin_json;
pub mod absolute_builtin_json;
pub mod local_concord_json;
pub mod absolute_under_shadow_json;
pub mod bare_builtin_binary;
pub mod qualified_builtin_binary;
pub mod absolute_builtin_binary;
pub mod local_concord_binary;
pub mod absolute_under_shadow_binary;
pub mod alias_named_concord;
pub mod macro_reexport_safe;
pub mod macro_reexport_custom;
pub mod macro_reexport_shadow;
pub mod nested_shadow_path;
pub mod bare_lowercase_text;
pub mod bare_lowercase_json;
pub mod bare_lowercase_bytes;
pub mod qualified_lowercase;

#[cfg(test)] mod tests {
use super::*;
#[tokio::test]
async fn f3_qualified() {
    let expected = wire::exchange(|url| f3_qualified::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| f3_qualified::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("f3_qualified: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = f3_qualified::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn f3a_scoped_import() {
    let expected = wire::exchange(|url| f3a_scoped_import::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| f3a_scoped_import::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("f3a_scoped_import: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = f3a_scoped_import::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn f3a_cfg_disabled() {
    let expected = wire::exchange(|url| f3a_cfg_disabled::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| f3a_cfg_disabled::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("f3a_cfg_disabled: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = f3a_cfg_disabled::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn f3a_line_comment() {
    let expected = wire::exchange(|url| f3a_line_comment::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| f3a_line_comment::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("f3a_line_comment: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = f3a_line_comment::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn f3a_block_comment() {
    let expected = wire::exchange(|url| f3a_block_comment::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| f3a_block_comment::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("f3a_block_comment: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = f3a_block_comment::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn cfg_selected_custom() {
    let expected = wire::exchange(|url| cfg_selected_custom::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| cfg_selected_custom::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("cfg_selected_custom: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = cfg_selected_custom::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn glob_custom() {
    let expected = wire::exchange(|url| glob_custom::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| glob_custom::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("glob_custom: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = glob_custom::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn aliased_import() {
    let expected = wire::exchange(|url| aliased_import::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| aliased_import::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("aliased_import: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = aliased_import::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn self_path() {
    let expected = wire::exchange(|url| self_path::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| self_path::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("self_path: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = self_path::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn super_path() {
    let expected = wire::exchange(|url| super_path::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| super_path::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("super_path: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = super_path::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn crate_path() {
    let expected = wire::exchange(|url| crate_path::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| crate_path::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("crate_path: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = crate_path::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn nested_module() {
    let expected = wire::exchange(|url| nested_module::deeper::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| nested_module::deeper::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("nested_module: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = nested_module::deeper::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn nested_super() {
    let expected = wire::exchange(|url| nested_super::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| nested_super::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("nested_super: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = nested_super::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn downstream_form() {
    let expected = wire::exchange(|url| downstream_form::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| downstream_form::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("downstream_form: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = downstream_form::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn bare_builtin_text() {
    let expected = wire::exchange(|url| bare_builtin_text::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| bare_builtin_text::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("bare_builtin_text: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = bare_builtin_text::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn qualified_builtin_text() {
    let expected = wire::exchange(|url| qualified_builtin_text::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| qualified_builtin_text::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("qualified_builtin_text: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = qualified_builtin_text::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn absolute_builtin_text() {
    let expected = wire::exchange(|url| absolute_builtin_text::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| absolute_builtin_text::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("absolute_builtin_text: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = absolute_builtin_text::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn local_concord_text() {
    let expected = wire::exchange(|url| local_concord_text::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| local_concord_text::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("local_concord_text: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = local_concord_text::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn absolute_under_shadow_text() {
    let expected = wire::exchange(|url| absolute_under_shadow_text::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| absolute_under_shadow_text::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("absolute_under_shadow_text: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = absolute_under_shadow_text::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn bare_builtin_json() {
    let expected = wire::exchange(|url| bare_builtin_json::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    let migrated = wire::exchange(|url| bare_builtin_json::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    println!("bare_builtin_json: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = bare_builtin_json::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn qualified_builtin_json() {
    let expected = wire::exchange(|url| qualified_builtin_json::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    let migrated = wire::exchange(|url| qualified_builtin_json::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    println!("qualified_builtin_json: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = qualified_builtin_json::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn absolute_builtin_json() {
    let expected = wire::exchange(|url| absolute_builtin_json::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    let migrated = wire::exchange(|url| absolute_builtin_json::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    println!("absolute_builtin_json: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = absolute_builtin_json::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn local_concord_json() {
    let expected = wire::exchange(|url| local_concord_json::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    let migrated = wire::exchange(|url| local_concord_json::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    println!("local_concord_json: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = local_concord_json::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn absolute_under_shadow_json() {
    let expected = wire::exchange(|url| absolute_under_shadow_json::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    let migrated = wire::exchange(|url| absolute_under_shadow_json::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    println!("absolute_under_shadow_json: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = absolute_under_shadow_json::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn bare_builtin_binary() {
    let expected = wire::exchange(|url| bare_builtin_binary::Expected::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| bare_builtin_binary::Migrated::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    println!("bare_builtin_binary: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = bare_builtin_binary::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn qualified_builtin_binary() {
    let expected = wire::exchange(|url| qualified_builtin_binary::Expected::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| qualified_builtin_binary::Migrated::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    println!("qualified_builtin_binary: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = qualified_builtin_binary::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn absolute_builtin_binary() {
    let expected = wire::exchange(|url| absolute_builtin_binary::Expected::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| absolute_builtin_binary::Migrated::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    println!("absolute_builtin_binary: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = absolute_builtin_binary::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn local_concord_binary() {
    let expected = wire::exchange(|url| local_concord_binary::Expected::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| local_concord_binary::Migrated::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    println!("local_concord_binary: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = local_concord_binary::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn absolute_under_shadow_binary() {
    let expected = wire::exchange(|url| absolute_under_shadow_binary::Expected::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| absolute_under_shadow_binary::Migrated::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    println!("absolute_under_shadow_binary: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = absolute_under_shadow_binary::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn alias_named_concord() {
    let expected = wire::exchange(|url| alias_named_concord::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| alias_named_concord::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("alias_named_concord: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = alias_named_concord::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn macro_reexport_safe() {
    let expected = wire::exchange(|url| macro_reexport_safe::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| macro_reexport_safe::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("macro_reexport_safe: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = macro_reexport_safe::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn macro_reexport_custom() {
    let expected = wire::exchange(|url| macro_reexport_custom::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| macro_reexport_custom::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("macro_reexport_custom: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = macro_reexport_custom::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn macro_reexport_shadow() {
    let expected = wire::exchange(|url| macro_reexport_shadow::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    let migrated = wire::exchange(|url| macro_reexport_shadow::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    println!("macro_reexport_shadow: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = macro_reexport_shadow::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn nested_shadow_path() {
    let expected = wire::exchange(|url| nested_shadow_path::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| nested_shadow_path::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("nested_shadow_path: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = nested_shadow_path::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn bare_lowercase_text() {
    let expected = wire::exchange(|url| bare_lowercase_text::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| bare_lowercase_text::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("bare_lowercase_text: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = bare_lowercase_text::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn bare_lowercase_json() {
    let expected = wire::exchange(|url| bare_lowercase_json::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    let migrated = wire::exchange(|url| bare_lowercase_json::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), br#""reply""#).await;
    println!("bare_lowercase_json: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = bare_lowercase_json::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn bare_lowercase_bytes() {
    let expected = wire::exchange(|url| bare_lowercase_bytes::Expected::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| bare_lowercase_bytes::Migrated::new(wire::http()).with_base_url(url).unwrap().p(b"value".to_vec()).prepare().unwrap(), b"reply").await;
    println!("bare_lowercase_bytes: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = bare_lowercase_bytes::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
#[tokio::test]
async fn qualified_lowercase() {
    let expected = wire::exchange(|url| qualified_lowercase::Expected::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    let migrated = wire::exchange(|url| qualified_lowercase::Migrated::new(wire::http()).with_base_url(url).unwrap().p("value".to_owned()).prepare().unwrap(), b"reply").await;
    println!("qualified_lowercase: expected {expected:?}; migrated {migrated:?}");
    assert!(expected.decoded.is_ok(), "control failed: {expected:?}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {migrated:?}");
    let absent = qualified_lowercase::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}
}
