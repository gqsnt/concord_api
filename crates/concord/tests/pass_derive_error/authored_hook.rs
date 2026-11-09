use std::fmt;

#[allow(non_upper_case_globals)]
const __formatter: u8 = 0;

#[derive(Debug, concord::derive::Error)]
#[display(with = error)]
enum Error {
    Message(String),
}

fn error(value: &Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match value {
        Error::Message(text) => write!(f, "authored: {text}"),
    }
}

mod generated_spelling {
    #[allow(non_upper_case_globals)]
    const __formatter_: u8 = 0;
    #[derive(Debug, concord::derive::Error)]
    #[display(with = __formatter)]
    pub enum Error { Message(String) }
    fn __formatter(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match value { Error::Message(text) => write!(f, "generated spelling: {text}") }
    }
}

fn main() {
    assert_eq!(generated_spelling::Error::from("hook".to_owned()).to_string(), "generated spelling: hook");
    assert_eq!(Error::from("test".to_owned()).to_string(), "authored: test");
}
