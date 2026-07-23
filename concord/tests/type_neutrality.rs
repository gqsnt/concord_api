use concord::advanced::{CodecError, DecodeBody, DecodeContext};
use concord::{Bytes, Request, api};

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
    }

    GET LocalName() -> crate::Json;
}

#[test]
fn local_type_named_json_needs_no_json_feature_or_macro_branch() {
    let client = LocalNameApi::new().expect("local response-spec client");
    let _: Request<_> = client.local_name();
}

#[cfg(feature = "json")]
pub type RenamedJson<T> = concord::codec::Json<T>;

#[cfg(feature = "json")]
api! {
    client ExtrasJsonApi {
        base "http://extras-json.example.test/";
    }

    GET Aliased() -> crate::RenamedJson<Vec<u32>>;
    GET Qualified() -> concord::codec::Json<Vec<u32>>;
}

#[cfg(feature = "json")]
#[test]
fn aliased_and_fully_qualified_extras_json_are_ordinary_types() {
    let client = ExtrasJsonApi::new().expect("extras JSON client");
    let _: Request<_> = client.aliased();
    let _: Request<_> = client.qualified();
}
