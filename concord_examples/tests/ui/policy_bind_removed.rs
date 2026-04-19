use concord_macros::api;

api! {
    client UiPolicyBindRemoved {
        scheme: https,
        host: "example.com",
    }

    POST Create(body: Json<()>) -> Json<()> {
        headers {
            "Idempotency-Key" as idempotency_key: String
        }
    }
}

fn main() {}
