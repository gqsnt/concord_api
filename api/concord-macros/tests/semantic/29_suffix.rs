concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(key: String) { path "objects/{*key}.bin"; } }
}
fn main() {}
