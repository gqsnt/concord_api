concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part spare() {} GET fetch(); }
}
fn main() {}
