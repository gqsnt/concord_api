use concord::api;

api! {
    client UnsupportedEndpointClauseApi {
        timeout 30s;
        base "https://unsupported.example.test";

        GET item() -> concord::Bytes via concord::Binary {
            unsupported;
        }
    }
}

fn main() {}
