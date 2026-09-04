use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        credentials {
            credential session = custom crate::SessionProvider;
        }


    GET protected() -> Bytes via Binary {
        auth { authorization "Token" = session; }
    }
}}

fn main() {}
