concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part r#loop() {} GET fetch(..r#loop); }
}
fn main() {}
