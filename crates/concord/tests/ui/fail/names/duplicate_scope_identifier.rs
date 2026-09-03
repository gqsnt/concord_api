use concord::api;

api! {
    client ReferenceApi {
        timeout off;
        base "https://reference.example.test";

        scope regional(id: String) {
            scope regional(code: String) {
                GET item() -> Bytes using Bytes;
            }
        }
    }
}

fn main() {}
