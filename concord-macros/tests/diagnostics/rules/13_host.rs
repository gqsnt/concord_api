concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(zone?: String) { base "https://{zone}.workshop.test"; } }
}
fn main() {}
