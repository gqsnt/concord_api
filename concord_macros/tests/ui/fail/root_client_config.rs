use concord_macros::api;

api! {
    client RootConfigApi {
        base "https://example.test";
    }

    timeout 5s;
}

fn main() {}
