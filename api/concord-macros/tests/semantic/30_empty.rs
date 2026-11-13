concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { headers { "" = "v"; } } }
}
fn main() {}
