concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(key: String) { path "objects/pre{*key}"; } }
}
fn main() {}
