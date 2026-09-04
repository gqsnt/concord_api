use concord::api;

api! {
    client RequestFenceApi {
        base "https://fence.example.test/";
        timeout 30s;
        request concord::Text;
        response concord::Text;

        scope fenced() {
            request explicit;
            POST missing(body input: String) -> String;
        }
    }
}

fn main() {}
