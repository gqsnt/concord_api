concord::api! {
    client Workshop(rate?: u16) { base "https://workshop.test"; middleware Quota(rate); GET fetch(); }
}
fn main() {}
