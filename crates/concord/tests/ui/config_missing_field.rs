// A client's required values are fields of `{Client}Config`; a missing one is reported by name.
concord::api! {
    pub client Api(region: String, account: String, project: String) {
        base "http://x.test";
        query { "region" = region; "account" = account; "project" = project; }
        GET item();
    }
}
fn main() {
    let api = Api::new(
        concord::reqwest::Client::new(),
        ApiConfig { region: "eu".into(), project: "p".into() },
    );
    let _ = api.item();
}
