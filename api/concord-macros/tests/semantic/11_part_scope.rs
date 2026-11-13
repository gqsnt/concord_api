concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part route() { headers { "x-region" = "{outer.region}"; } } scope outer(region: String) { path "{region}"; GET fetch(..route); } }
}
fn main() {}
