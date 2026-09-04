use concord::api;

api! {
    client ReferenceApi {
        timeout 30s;
        base "https://reference.example.test";

        scope regional(id: String) {
            scope regional(code: String) {
                GET item() -> Bytes via Binary;
            }
        }
    }
}

fn main() {}
