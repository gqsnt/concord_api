concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { headers { "bad key" = "v"; } } }
}
fn main() {}
