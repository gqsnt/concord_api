concord::api! {
    pub client Api {
        base "https://user@x.test/?q=1";
        GET list();
    }
}
fn main() {}
