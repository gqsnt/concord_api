use concord_macros::api;

api! {
    client MultipartDisabledApi {
        base "http://multipart-disabled.example.test";
    }

    scope uploads() {
        path ["uploads"];
        POST create(#[body] body: Multipart) -> NoContent;
    }
}
