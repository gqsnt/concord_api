use concord_macros::api;

api! {
    client NoContentApi { base "https://example.com" }
    GET Ping
        path ["ping"]
        -> NoContent
}

async fn uses_no_content() {
    let api = no_content_api::NoContentApi::new();
    let _: () = api.ping().execute().await.unwrap();
}

fn main() {}
