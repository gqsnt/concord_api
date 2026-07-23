use concord_macros::api;

api! {
    client DevelopmentProofApi {
        base "http://development-proof.example.test/";
    }

    scope resources(id: String) {
        path ["resources", scope.id];
        GET fetch(value: String) -> NoContent {
            path [ep.value];
        }
    }
}

fn main() {
    let _: Option<DevelopmentProofApi<()>> = None;
    let _ = DevelopmentProofApi::new(());
}
