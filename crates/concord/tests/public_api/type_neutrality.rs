use concord::codec::{CodecError, DecodeBody, DecodeContext};
use concord::{Bytes, api};

/// A downstream response specification deliberately named like an optional extra.
pub struct Json;

impl DecodeBody for Json {
    type Output = Bytes;

    const ACCEPT: Option<&'static str> = Some("application/x-local-json-name");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(body)
    }
}

api! {
    client LocalNameApi {
        timeout 30s;
        base "http://local-name.example.test/";


    GET local_name() -> Bytes via crate::Json;
}}

#[test]
fn local_type_named_json_needs_no_json_feature_or_macro_branch() {
    let client = LocalNameApi::new().expect("local response-spec client");
    let _: local_name_api::LocalNameRequest = client.local_name();
}

#[cfg(feature = "json")]
pub type RenamedJson = concord::codec::Json;

#[cfg(feature = "json")]
api! {
    client ExtrasJsonApi {
        timeout 30s;
        base "http://extras-json.example.test/";


    GET aliased() -> Vec<u32> via crate::RenamedJson;
    GET qualified() -> Vec<u32> via concord::codec::Json;
}}

#[cfg(feature = "json")]
#[test]
fn aliased_and_fully_qualified_extras_json_are_ordinary_types() {
    let client = ExtrasJsonApi::new().expect("extras JSON client");
    let _: extras_json_api::AliasedRequest = client.aliased();
    let _: extras_json_api::QualifiedRequest = client.qualified();
}
