#[derive(Clone)]
pub struct UserId(pub u64);
impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}
concord::api! {
    pub client Api {
        base "http://x.test";
        GET user(user: UserId) { path "users/{user}"; }
    }
}
fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    let _ = api.user(42u64);
}
