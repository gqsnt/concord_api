concord::api! {
    pub client Api {
        base "http://x.test";
        GET one(#[serde(rename = "x")] query x: String);
    }
}
fn main() {}
