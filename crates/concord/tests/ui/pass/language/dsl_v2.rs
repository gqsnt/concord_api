use concord::{Bytes, Text, api};

type MaybeString = Option<String>;

api! {
    client V2Api {
        timeout 5s;
        vars {
            tenant: String = crate::DEFAULT_TENANT;
        }
        base "http://v2.example.test/";
        headers { "X-Contract" = "v2"; }
    }

    scope http_status() {
        GET r#type() -> Bytes;
        path ["status"];

        POST submit(
            first: String,
            #[body] payload: Text,
            body: String,
            optional: std::option::Option<u32>,
            alias_is_required: crate::MaybeString,
            last: bool,
        ) -> Bytes {
            timeout 2s;
            id "v2.http_status.submit";
            headers { "X-Last" = ep.last; }
            path ["submit", ep.first];
            query { "body" = ep.body; }
        }
    }
}

const DEFAULT_TENANT: String = String::new();

fn main() {
    let client = V2Api::new().expect("zero-input client");
    let _: v2_api::http_status::TypeRequest = client.http_status().r#type();
    let _: v2_api::http_status::SubmitRequest = client.http_status().submit(
        "first".to_owned(),
        "body".to_owned(),
        "ordinary-parameter".to_owned(),
        None,
        true,
    );
    let endpoint = v2_api::http_status::Submit::new(
        "first".to_owned(),
        "body".to_owned(),
        "ordinary-parameter".to_owned(),
        None,
        true,
    )
    .set_optional(7)
    .clear_optional();
    let _: v2_api::http_status::SubmitRequest = client.request(endpoint);
}
