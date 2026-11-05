struct Filter(String);
impl std::fmt::Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
concord::api! { pub client Api { base "http://127.0.0.1";
    GET list(query filter?: Filter) { path "items"; }
} }
fn main() {
    let request = Api::new(concord::reqwest::Client::new()).list();
    let _first = request.clone().prepare().unwrap();
    let _second = request.prepare().unwrap();
}
