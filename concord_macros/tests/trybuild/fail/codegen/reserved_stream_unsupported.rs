use concord_macros::api;

api! {
    client ReservedStreamUnsupportedApi { base "https://example.com" }

    GET Download
        path ["download"]
        -> Stream<String>
}

fn main() {}
