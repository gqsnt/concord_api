use concord_macros::api;

api! {
    client ReservedMultipartUnsupportedApi { base "https://example.com" }

    POST Upload(body: Multipart<String>)
        path ["upload"]
        -> Json<()>
}

fn main() {}
