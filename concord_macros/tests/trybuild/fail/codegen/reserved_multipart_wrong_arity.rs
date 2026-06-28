use concord_macros::api;

api! {
    client ReservedMultipartWrongArityApi { base "https://example.com" }

    POST Upload(body: Multipart<String, String, String>)
        path ["upload"]
        -> Json<()>
}

fn main() {}
