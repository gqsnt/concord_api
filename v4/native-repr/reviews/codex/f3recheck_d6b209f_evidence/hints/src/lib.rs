#![allow(dead_code, unused_imports, non_camel_case_types)]
extern crate concord as c4;

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
pub mod f3_qualified_body {
use crate::codec;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via codec::Text); } }
}
pub mod f3_qualified_optional {
use crate::codec;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via codec::Text); } }
}
pub mod f3_qualified_output {
use crate::codec;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via codec::Text; } }
}
pub mod f3a_scoped_import_body {
use crate::codec::Text; mod unrelated { use concord::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
pub mod f3a_scoped_import_optional {
use crate::codec::Text; mod unrelated { use concord::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
pub mod f3a_scoped_import_output {
use crate::codec::Text; mod unrelated { use concord::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
pub mod f3a_cfg_disabled_body {
use crate::codec::Text; #[cfg(any())] use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
pub mod f3a_cfg_disabled_optional {
use crate::codec::Text; #[cfg(any())] use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
pub mod f3a_cfg_disabled_output {
use crate::codec::Text; #[cfg(any())] use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
pub mod f3a_line_comment_body {
use crate::codec::Text; // use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
pub mod f3a_line_comment_optional {
use crate::codec::Text; // use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
pub mod f3a_line_comment_output {
use crate::codec::Text; // use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
pub mod f3a_block_comment_body {
use crate::codec::Text; /* use concord::{Json, Text}; */
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
pub mod f3a_block_comment_optional {
use crate::codec::Text; /* use concord::{Json, Text}; */
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
pub mod f3a_block_comment_output {
use crate::codec::Text; /* use concord::{Json, Text}; */
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
pub mod cfg_selected_custom_body {
#[cfg(all())] use crate::codec::Text; #[cfg(any())] use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
pub mod cfg_selected_custom_optional {
#[cfg(all())] use crate::codec::Text; #[cfg(any())] use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
pub mod cfg_selected_custom_output {
#[cfg(all())] use crate::codec::Text; #[cfg(any())] use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
pub mod glob_custom_body {
use crate::codec::*; mod unrelated { use concord::*; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
pub mod glob_custom_optional {
use crate::codec::*; mod unrelated { use concord::*; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
pub mod glob_custom_output {
use crate::codec::*; mod unrelated { use concord::*; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
pub mod aliased_import_body {
use crate::codec::Text as CustomText; use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via CustomText); } }
}
pub mod aliased_import_optional {
use crate::codec::Text as CustomText; use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via CustomText); } }
}
pub mod aliased_import_output {
use crate::codec::Text as CustomText; use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via CustomText; } }
}
pub mod self_path_body {
use crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via self::Text); } }
}
pub mod self_path_optional {
use crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via self::Text); } }
}
pub mod self_path_output {
use crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via self::Text; } }
}
pub mod super_path_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via super::codec::Text); } }
}
pub mod super_path_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via super::codec::Text); } }
}
pub mod super_path_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via super::codec::Text; } }
}
pub mod crate_path_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via crate::codec::Text); } }
}
pub mod crate_path_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via crate::codec::Text); } }
}
pub mod crate_path_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via crate::codec::Text; } }
}
pub mod nested_module_body {
pub mod deeper {
use crate::codec::Text; mod unrelated { use concord::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
}
pub mod nested_module_optional {
pub mod deeper {
use crate::codec::Text; mod unrelated { use concord::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
}
pub mod nested_module_output {
pub mod deeper {
use crate::codec::Text; mod unrelated { use concord::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
}
pub mod nested_super_body {
pub type Text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via super::Text); } }
}
pub mod nested_super_optional {
pub type Text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via super::Text); } }
}
pub mod nested_super_output {
pub type Text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via super::Text; } }
}
pub mod downstream_form_body {
use crate::codec::Form;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Form); } }
}
pub mod downstream_form_optional {
use crate::codec::Form;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Form); } }
}
pub mod downstream_form_output {
use crate::codec::Form;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Form; } }
}
pub mod bare_builtin_text_body {
use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Text); } }
}
pub mod bare_builtin_text_optional {
use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Text); } }
}
pub mod bare_builtin_text_output {
use concord::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Text; } }
}
pub mod qualified_builtin_text_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via concord::Text); } }
}
pub mod qualified_builtin_text_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via concord::Text); } }
}
pub mod qualified_builtin_text_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via concord::Text; } }
}
pub mod absolute_builtin_text_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via ::concord::Text); } }
}
pub mod absolute_builtin_text_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via ::concord::Text); } }
}
pub mod absolute_builtin_text_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via ::concord::Text; } }
}
pub mod local_concord_text_body {
mod concord { pub use crate::codec::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via concord::Text); } }
}
pub mod local_concord_text_optional {
mod concord { pub use crate::codec::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via concord::Text); } }
}
pub mod local_concord_text_output {
mod concord { pub use crate::codec::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via concord::Text; } }
}
pub mod absolute_under_shadow_text_body {
mod concord { pub use crate::codec::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via ::concord::Text); } }
}
pub mod absolute_under_shadow_text_optional {
mod concord { pub use crate::codec::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via ::concord::Text); } }
}
pub mod absolute_under_shadow_text_output {
mod concord { pub use crate::codec::Text; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via ::concord::Text; } }
}
pub mod bare_builtin_json_body {
use concord::Json;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via Json); } }
}
pub mod bare_builtin_json_optional {
use concord::Json;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via Json); } }
}
pub mod bare_builtin_json_output {
use concord::Json;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via Json; } }
}
pub mod qualified_builtin_json_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via concord::Json); } }
}
pub mod qualified_builtin_json_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via concord::Json); } }
}
pub mod qualified_builtin_json_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via concord::Json; } }
}
pub mod absolute_builtin_json_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via ::concord::Json); } }
}
pub mod absolute_builtin_json_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via ::concord::Json); } }
}
pub mod absolute_builtin_json_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via ::concord::Json; } }
}
pub mod local_concord_json_body {
mod concord { pub use crate::codec::Json; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via concord::Json); } }
}
pub mod local_concord_json_optional {
mod concord { pub use crate::codec::Json; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via concord::Json); } }
}
pub mod local_concord_json_output {
mod concord { pub use crate::codec::Json; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via concord::Json; } }
}
pub mod absolute_under_shadow_json_body {
mod concord { pub use crate::codec::Json; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via ::concord::Json); } }
}
pub mod absolute_under_shadow_json_optional {
mod concord { pub use crate::codec::Json; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via ::concord::Json); } }
}
pub mod absolute_under_shadow_json_output {
mod concord { pub use crate::codec::Json; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via ::concord::Json; } }
}
pub mod bare_builtin_binary_body {
use concord::Binary;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: Vec<u8> via Binary); } }
}
pub mod bare_builtin_binary_optional {
use concord::Binary;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: Vec<u8> via Binary); } }
}
pub mod bare_builtin_binary_output {
use concord::Binary;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> Vec<u8> via Binary; } }
}
pub mod qualified_builtin_binary_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: Vec<u8> via concord::Binary); } }
}
pub mod qualified_builtin_binary_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: Vec<u8> via concord::Binary); } }
}
pub mod qualified_builtin_binary_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> Vec<u8> via concord::Binary; } }
}
pub mod absolute_builtin_binary_body {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: Vec<u8> via ::concord::Binary); } }
}
pub mod absolute_builtin_binary_optional {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: Vec<u8> via ::concord::Binary); } }
}
pub mod absolute_builtin_binary_output {

crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> Vec<u8> via ::concord::Binary; } }
}
pub mod local_concord_binary_body {
mod concord { pub use crate::codec::Binary; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: Vec<u8> via concord::Binary); } }
}
pub mod local_concord_binary_optional {
mod concord { pub use crate::codec::Binary; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: Vec<u8> via concord::Binary); } }
}
pub mod local_concord_binary_output {
mod concord { pub use crate::codec::Binary; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> Vec<u8> via concord::Binary; } }
}
pub mod absolute_under_shadow_binary_body {
mod concord { pub use crate::codec::Binary; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: Vec<u8> via ::concord::Binary); } }
}
pub mod absolute_under_shadow_binary_optional {
mod concord { pub use crate::codec::Binary; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: Vec<u8> via ::concord::Binary); } }
}
pub mod absolute_under_shadow_binary_output {
mod concord { pub use crate::codec::Binary; }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> Vec<u8> via ::concord::Binary; } }
}
pub mod alias_named_concord_body {
use crate::codec as concord;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via concord::Text); } }
}
pub mod alias_named_concord_optional {
use crate::codec as concord;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via concord::Text); } }
}
pub mod alias_named_concord_output {
use crate::codec as concord;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via concord::Text; } }
}
pub mod macro_reexport_safe_body {
mod facade { pub use crate::c4::{api, Text}; }
facade::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via facade::Text); } }
}
pub mod macro_reexport_safe_optional {
mod facade { pub use crate::c4::{api, Text}; }
facade::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via facade::Text); } }
}
pub mod macro_reexport_safe_output {
mod facade { pub use crate::c4::{api, Text}; }
facade::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via facade::Text; } }
}
pub mod macro_reexport_custom_body {
mod facade { pub use crate::c4::api; pub use crate::codec::Text; }
facade::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via facade::Text); } }
}
pub mod macro_reexport_custom_optional {
mod facade { pub use crate::c4::api; pub use crate::codec::Text; }
facade::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via facade::Text); } }
}
pub mod macro_reexport_custom_output {
mod facade { pub use crate::c4::api; pub use crate::codec::Text; }
facade::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via facade::Text; } }
}
pub mod macro_reexport_shadow_body {
mod concord { pub use crate::c4::api; pub use crate::codec::Json; }
concord::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via concord::Json); } }
}
pub mod macro_reexport_shadow_optional {
mod concord { pub use crate::c4::api; pub use crate::codec::Json; }
concord::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via concord::Json); } }
}
pub mod macro_reexport_shadow_output {
mod concord { pub use crate::c4::api; pub use crate::codec::Json; }
concord::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via concord::Json; } }
}
pub mod nested_shadow_path_body {
mod nested { pub mod concord { pub use crate::codec::Json; } }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via nested::concord::Json); } }
}
pub mod nested_shadow_path_optional {
mod nested { pub mod concord { pub use crate::codec::Json; } }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via nested::concord::Json); } }
}
pub mod nested_shadow_path_output {
mod nested { pub mod concord { pub use crate::codec::Json; } }
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via nested::concord::Json; } }
}
pub mod bare_lowercase_text_body {
type text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via text); } }
}
pub mod bare_lowercase_text_optional {
type text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via text); } }
}
pub mod bare_lowercase_text_output {
type text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via text; } }
}
pub mod bare_lowercase_json_body {
type json = crate::codec::Json;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via json); } }
}
pub mod bare_lowercase_json_optional {
type json = crate::codec::Json;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via json); } }
}
pub mod bare_lowercase_json_output {
type json = crate::codec::Json;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via json; } }
}
pub mod bare_lowercase_bytes_body {
type bytes = crate::codec::Binary;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: Vec<u8> via bytes); } }
}
pub mod bare_lowercase_bytes_optional {
type bytes = crate::codec::Binary;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: Vec<u8> via bytes); } }
}
pub mod bare_lowercase_bytes_output {
type bytes = crate::codec::Binary;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> Vec<u8> via bytes; } }
}
pub mod qualified_lowercase_body {
type text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via self::text); } }
}
pub mod qualified_lowercase_optional {
type text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via self::text); } }
}
pub mod qualified_lowercase_output {
type text = crate::codec::Text;
crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via self::text; } }
}
pub mod raw_json_body { type r#json = crate::codec::Text; crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via r#json); } } }
pub mod raw_json_optional { type r#json = crate::codec::Text; crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via r#json); } } }
pub mod raw_json_output { type r#json = crate::codec::Text; crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via r#json; } } }
pub mod raw_type_body { type r#type = crate::codec::Text; crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b: String via r#type); } } }
pub mod raw_type_optional { type r#type = crate::codec::Text; crate::c4::api! { pub client Hint { base "http://127.0.0.1"; POST p(body b?: String via r#type); } } }
pub mod raw_type_output { type r#type = crate::codec::Text; crate::c4::api! { pub client Hint { base "http://127.0.0.1"; GET p() -> String via r#type; } } }
