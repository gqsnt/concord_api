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
        base "http://local-name.example.test/";
        timeout 30s;

        GET local_name() -> Bytes via crate::Json;
    }
}

/// REP-01: representation selection is by Rust trait obligations, never by
/// the spelling `Json`; a local type named `Json` is an ordinary buffered
/// codec reached through the `DecodeBody` blanket bridge.
#[test]
fn local_type_named_json_needs_no_json_feature_or_macro_branch() {
    let client = LocalNameApi::new().expect("local response-spec client");
    let request: local_name_api::LocalNameRequest = client.local_name();
    let description = request.description();
    assert_eq!(
        description.response_accept(),
        Some("application/x-local-json-name")
    );
    let contract = LocalNameApi::contract()
        .endpoint("local_name_api.local_name")
        .expect("local name endpoint");
    assert_eq!(
        contract.result().representation().type_name(),
        "crate::Json"
    );
    assert_eq!(
        contract.result().accept(),
        Some("application/x-local-json-name")
    );
}

#[cfg(feature = "json")]
pub type RenamedJson = concord::codec::Json;

#[cfg(feature = "json")]
api! {
    client ExtrasJsonApi {
        base "http://extras-json.example.test/";
        timeout 30s;

        GET aliased() -> Vec<u32> via crate::RenamedJson;
        GET qualified() -> Vec<u32> via concord::codec::Json;
    }
}

#[cfg(feature = "json")]
#[test]
fn aliased_and_fully_qualified_extras_json_are_ordinary_types() {
    let client = ExtrasJsonApi::new().expect("extras JSON client");
    let aliased: extras_json_api::AliasedRequest = client.aliased();
    let qualified: extras_json_api::QualifiedRequest = client.qualified();
    assert_eq!(
        aliased.description().response_accept(),
        qualified.description().response_accept()
    );
    assert!(
        aliased
            .description()
            .response_accept()
            .is_some_and(|accept| accept.contains("json"))
    );
}
