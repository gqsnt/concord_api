use concord::api;

pub struct CustomDefault;

impl Default for CustomDefault {
    fn default() -> Self {
        Self
    }
}

api! {
    client DefaultShorthandApi {
        base "http://example.test";
        vars {
            scalar: u32 = default;
            optional: Option<String> = default;
            vector: Vec<String> = default;
            custom: crate::CustomDefault = default;
        }
    }

    scope group(scope_value: crate::CustomDefault = default) {
        GET read(endpoint_value: crate::CustomDefault = default) -> Bytes;
    }
}

fn main() {
    let client = DefaultShorthandApi::builder()
        .set_scalar(1)
        .reset_scalar()
        .set_optional("selected".to_owned())
        .clear_optional()
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

    let endpoint = default_shorthand_api::group::Read::new()
        .set_scope_value(CustomDefault)
        .reset_scope_value()
        .set_endpoint_value(CustomDefault)
        .reset_endpoint_value();
    let _: default_shorthand_api::group::ReadRequest = client.request(endpoint);
}
