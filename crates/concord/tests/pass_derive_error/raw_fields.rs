#[derive(Debug, concord::derive::Error)]
enum Error {
    #[display("{r#type}")]
    Used { r#type: &'static str },
    #[display("kind {r#type}")]
    Composite { r#type: &'static str },
}

fn main() {
    assert_eq!(Error::Used { r#type: "used" }.to_string(), "used");
    assert_eq!(Error::Composite { r#type: "used" }.to_string(), "kind used");
}
