concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { path "records/{missing_code}"; } }
}
fn main() {}
