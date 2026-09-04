use concord::{Bytes, Text, api};

type MaybeString = Option<String>;

api! {
    client CurrentApi {
        timeout 5s;
        values {
            tenant: String = crate::DEFAULT_TENANT;
        }
        base "http://current.example.test/";
        headers { "X-Contract" = "current"; }


    scope http_status() {
        GET r#type() -> Bytes via Binary;
        path ["status"];

        POST submit(
            first: String,
            body payload: String via Text,
            body_value: String,
            optional?: u32,
            alias_is_required: crate::MaybeString,
            last: bool,
        ) -> Bytes via Binary {
            timeout 2s;
            id "v2.http_status.submit";
            headers { "X-Last" = endpoint.last; }
            path ["submit", endpoint.first];
            query { "body" = endpoint.body_value; }
        }
    }
}}

const DEFAULT_TENANT: String = String::new();

fn main() {
    let client = CurrentApi::new().expect("zero-input client");
    let _: v2_api::http_status::TypeRequest = client.http_status().r#type();
    let _: v2_api::http_status::SubmitRequest = client.http_status().submit(
        "first".to_owned(),
        "body".to_owned(),
        "ordinary-parameter".to_owned(),
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
        .set_optional(Some(7))
        .omit_optional();
    let _: v2_api::http_status::SubmitRequest = client.request(endpoint);
}
