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
        timeout off;
        base "http://local-name.example.test/";


    GET local_name() -> Bytes using crate::Json;
}}

#[test]
fn local_type_named_json_needs_no_json_feature_or_macro_branch() {
    let client = LocalNameApi::new().expect("local response-spec client");
    let _: local_name_api::LocalNameRequest = client.local_name();
}

#[cfg(feature = "json")]
pub type RenamedJson<T> = concord::codec::Json<T>;

#[cfg(feature = "json")]
api! {
    client ExtrasJsonApi {
        timeout off;
        base "http://extras-json.example.test/";


    GET aliased() -> Vec<u32> using crate::RenamedJson<Vec<u32>>;
    GET qualified() -> Vec<u32> using concord::codec::Json<Vec<u32>>;
}}

#[cfg(feature = "json")]
#[test]
fn aliased_and_fully_qualified_extras_json_are_ordinary_types() {
    let client = ExtrasJsonApi::new().expect("extras JSON client");
    let _: extras_json_api::AliasedRequest = client.aliased();
    let _: extras_json_api::QualifiedRequest = client.qualified();
}
