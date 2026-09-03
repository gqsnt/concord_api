use concord::api;

api! {
    client DevelopmentProofApi {
        timeout off;
        base "http://development-proof.example.test/";


    scope resources(id: String) {
        path ["resources", resources.id];
        GET fetch(value: String) -> () using NoContent {
            path [endpoint.value];
        }
    }
}}

fn main() {
    let _: Option<DevelopmentProofApi<()>> = None;
    let _ = DevelopmentProofApi::new(());
}
