// §7.2 / §16.3: generated clients are concrete, non-generic types; there is
// no executor type parameter or executor-taking constructor through which a
// caller could substitute native execution.
use concord::{Empty, api};

api! {
    client DevelopmentProofApi {
        timeout 30s;
        base "http://development-proof.example.test/";

        scope resources(id: String) {
            path "resources/{id}";

            GET fetch(value: String) -> () via Empty {
                path "{value}";
            }
        }
    }
}

fn main() {
    let _: Option<DevelopmentProofApi<()>> = None;
    let _ = DevelopmentProofApi::new(());
}
