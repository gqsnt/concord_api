use concord::api;

api! {
    client Api {
        base "https://example.test";
        credentials {
            credential session = custom crate::SessionProvider;
        }
    }

    GET protected() -> Bytes {
        auth authorization "Token" = session;
    }
}

fn main() {}
