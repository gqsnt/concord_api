use concord_macros::api;

mod alpha {
    use super::api;

    api! {
        client AlphaApi {
            base "https://alpha.example.test";
            scope health() {
                path ["health"];
                GET Ping() -> NoContent;
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
                GET Ping() -> NoContent;
            }
        }
    }
}

fn main() {
    let beta = beta::BetaApi::new().unwrap();
    let alpha_endpoint = alpha::alpha_api::endpoints::health::Ping::new();
    let _ = beta.request(alpha_endpoint);
}
