use concord::api;

api! {
    client FirstApi {
        base "https://one.example.test";
    }

    client SecondApi {
        base "https://two.example.test";
    }
}

fn main() {}
