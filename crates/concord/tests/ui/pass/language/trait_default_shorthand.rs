// §3.1 / §3.5: `#[default]` is the explicit route for a payload's `Default`
// implementation on client, scope and endpoint values (v2 `= default`). The
// initialized values get `set_x`/`reset_x`; the initialized optional value also
// gets `replace_x`/`omit_x` (VALUE-03). Every value is live (VALUE-06).
use concord::{AtomError, Binary, BoundedAtomWriter, Bytes, RequestAtom, api};

pub struct CustomDefault;

impl Default for CustomDefault {
    fn default() -> Self {
        Self
    }
}

impl RequestAtom for CustomDefault {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str("custom")
    }
}

api! {
    client DefaultShorthandApi {
        timeout 30s;
        base "http://example.test";
        values {
            #[default]
            scalar: u32;
            #[default]
            optional?: String;
            #[default]
            vector: Vec<String>;
            #[default]
            custom: crate::CustomDefault;
        }
        query {
            "scalar" = scalar;
            "optional" = optional;
            "vector" = each vector;
            "custom" = custom;
        }

        scope group(#[default] query scope_value: crate::CustomDefault) {
            GET read(#[default] query endpoint_value: crate::CustomDefault) -> Bytes via Binary;
        }
    }
}

fn main() {
    let client = DefaultShorthandApi::builder()
        .set_scalar(1)
        .reset_scalar()
        .set_optional("selected")
        .replace_optional(Some("selected".to_owned()))
        .omit_optional()
        .reset_optional()
        .set_vector(vec!["selected".to_owned()])
        .reset_vector()
        .set_custom(CustomDefault)
        .reset_custom()
        .build()
        .expect("trait defaults");

    let _: default_shorthand_api::group::ReadRequest = client
        .group()
        .set_scope_value(CustomDefault)
        .reset_scope_value()
        .read()
        .set_endpoint_value(CustomDefault)
        .reset_endpoint_value();

    let endpoint = DefaultShorthandApi::unbound()
        .group()
        .set_scope_value(CustomDefault)
        .reset_scope_value()
        .read()
        .set_endpoint_value(CustomDefault)
        .reset_endpoint_value();
    let _: default_shorthand_api::group::ReadRequest = client.request(endpoint);
}
