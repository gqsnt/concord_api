// The current v3 language in one declaration: preamble-first owners, raw
// endpoint identifiers, relative path templates (v2 path atom lists, §5.3),
// bare local references (v2 `endpoint.x`, LANG-03), a body with its own
// representation, optional/initialized/required values, a type alias that
// merely *spells* `Option` staying a required positional input (§4.1), an
// explicit endpoint ID and endpoint-local timeout.
use concord::{AtomEncoder, AtomError, Binary, BoundedAtomWriter, Bytes, Text, api};

type MaybeString = Option<String>;

/// Explicit atom for the required alias-typed value; core deliberately has
/// no `Option` atom (optionality is structural, never inferred).
pub struct PresenceText;

impl AtomEncoder<MaybeString> for PresenceText {
    fn write(value: &MaybeString, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(value.as_deref().unwrap_or("none"))
    }
}

api! {
    client CurrentApi {
        timeout 5s;
        values {
            tenant: String = crate::DEFAULT_TENANT;
        }
        base "http://current.example.test/";
        headers {
            "X-Contract" = "current";
            "X-Tenant" = tenant;
        }

        scope http_status() {
            path "status";

            GET r#type() -> Bytes via Binary;

            POST submit(
                first: String,
                body payload: String via Text,
                body_value: String,
                optional?: u32,
                header("X-Alias") alias_is_required: crate::MaybeString via crate::PresenceText,
                last: bool,
            ) -> Bytes via Binary {
                timeout 2s;
                id "current.http_status.submit";
                headers { "X-Last" = last; }
                path "submit/{first}";
                query {
                    "body" = body_value;
                    "optional" = optional;
                }
            }
        }
    }
}

const DEFAULT_TENANT: String = String::new();

fn main() {
    let client = CurrentApi::new().expect("zero-input client");
    let _: current_api::http_status::TypeRequest = client.http_status().r#type();
    let _: current_api::http_status::SubmitRequest = client.http_status().submit(
        "first",
        "body".to_owned(),
        "ordinary-parameter",
        None,
        true,
    );
    let endpoint = CurrentApi::unbound()
        .http_status()
        .submit(
            "first".to_owned(),
            "body".to_owned(),
            "ordinary-parameter".to_owned(),
            None,
            true,
        )
        .set_optional(7)
        .replace_optional(Some(7))
        .omit_optional();
    let _: current_api::http_status::SubmitRequest = client.request(endpoint);
}
