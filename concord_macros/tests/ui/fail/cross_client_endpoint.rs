use concord_macros::api;

api! {
    client AlphaApi {
        base "https://alpha.example.test";
    }

    scope health() {
        path ["health"];
        GET download() -> Stream<OctetStream>;
    }
}

api! {
    client BetaApi {
        base "https://beta.example.test";
    }

    scope health() {
        path ["health"];
        GET download() -> Stream<OctetStream>;
    }
}

fn main() {
    let beta = BetaApi::new().unwrap();
    let alpha_endpoint = alpha_api::health::Download::new();
    let _ = beta.request(alpha_endpoint);
}
