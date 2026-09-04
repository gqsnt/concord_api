use concord::api;

pub struct CustomDefault;

impl Default for CustomDefault {
    fn default() -> Self {
        Self
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


    scope group(#[default] scope_value: crate::CustomDefault) {
        GET read(#[default] endpoint_value: crate::CustomDefault) -> Bytes via Binary;
    }
}}

fn main() {
    let client = DefaultShorthandApi::builder()
        .set_scalar(1)
        .reset_scalar()
        .set_optional(Some("selected".to_owned()))
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
