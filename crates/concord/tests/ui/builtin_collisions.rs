concord::api! {
    pub client Api {
        base "http://x.test";
        GET with_base_url();
        GET clone();
        scope s { GET prepare() { path "p"; } GET send(); }
        GET e(query send?: String) { path "e"; }
    }
}
fn main() {}
