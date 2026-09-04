use concord::api;

api! {
    client ResponseFenceApi {
        base "https://fence.example.test/";
        timeout 30s;
        response concord::Text;

        scope fenced() {
            response explicit;
            GET missing() -> String;
        }
    }
}

fn main() {}
