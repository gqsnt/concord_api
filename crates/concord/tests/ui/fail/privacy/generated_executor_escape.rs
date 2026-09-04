use concord::api;

api! {
    client DevelopmentProofApi {
        timeout 30s;
        base "http://development-proof.example.test/";


    scope resources(id: String) {
        path ["resources", resources.id];
        GET fetch(value: String) -> () via Empty {
            path [endpoint.value];
        }
    }
}}

fn main() {
    let _: Option<DevelopmentProofApi<()>> = None;
    let _ = DevelopmentProofApi::new(());
}
