use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        credentials {
            credential session = custom crate::SessionProvider;
        }


    GET protected() -> Bytes using Bytes {
        auth { authorization "Token" = session; }
    }
}}

fn main() {}
