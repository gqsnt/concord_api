pub struct Nonce(pub String);
impl std::fmt::Display for Nonce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
concord::api! {
    pub client Api {
        base "http://x.test";
        GET once(query nonce: Nonce) { path "once"; }
    }
}
fn main() {
    let request = Api::new(concord::reqwest::Client::new()).once(Nonce("n".into()));
    let _ = request.clone();
}
