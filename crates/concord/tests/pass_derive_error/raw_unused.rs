#[derive(Debug, concord::derive::Error)]
enum Error {
    #[display("plain")]
    Unused { r#type: u8 },
}
fn main() {
    let error = Error::Unused { r#type: 7 };
    assert_eq!(error.to_string(), "plain");
    let Error::Unused { r#type: field } = error;
    assert_eq!(field, 7);
}
