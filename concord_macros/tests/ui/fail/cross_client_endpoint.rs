use concord_macros::api;

mod alpha {
    use super::api;

    api! {
        client AlphaApi {
            base "https://alpha.example.test";
            scope health() {
                path ["health"];
                GET Download() -> Stream<OctetStream>;
            }
        }
    }
}

mod beta {
    use super::api;

    api! {
        client BetaApi {
            base "https://beta.example.test";
            scope health() {
                path ["health"];
                GET Download() -> Stream<OctetStream>;
            }
        }
    }
}

fn main() {
    let beta = beta::BetaApi::new().unwrap();
    let alpha_endpoint = alpha::alpha_api::endpoints::health::Download::new();
    let _ = beta.request(alpha_endpoint);
}
