use concord_macros::api;

api! {
    client ReservedStreamWrongArityApi { base "https://example.com" }

    GET Download
        path ["download"]
        -> Stream
}

fn main() {}
