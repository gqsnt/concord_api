concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { headers { "naïve" = "v"; } } }
}
fn main() {}
