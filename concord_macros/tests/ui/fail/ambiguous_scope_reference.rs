use concord_macros::api;

api! {
    client ReferenceApi {
        base "https://reference.example.test";
    }

    scope outer(id: String) {
        scope inner(id: String) {
            GET Item() -> Bytes {
                path [scope.id];
            }
        }
    }
}

fn main() {}
