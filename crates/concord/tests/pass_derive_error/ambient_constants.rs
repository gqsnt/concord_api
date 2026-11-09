use std::error::Error as _;

#[allow(non_upper_case_globals, dead_code)]
mod constants {
    pub const cause: u8 = 0;
    pub const __source: u8 = 0;
    pub const __formatter: u8 = 0;
    pub const __field__0: u8 = 0;
    pub const value: u8 = 0;
    pub const error: u8 = 0;
    pub const failure: u8 = 0;
}
#[allow(unused_imports)]
use constants::*;

#[derive(Debug, concord::derive::Error)]
enum Error {
    #[display("{0}")]
    Plain(#[source] std::io::Error),
    #[display("optional")]
    Optional { #[source] cause: Option<std::io::Error> },
    #[display("{0}")]
    Concord(#[source] concord::Error),
    #[display("provider")]
    #[from(concord::Error<Problem>)]
    Provider(concord::Failure<Problem>),
}

#[derive(Debug)]
struct Problem;

fn main() {
    let plain = Error::from(std::io::Error::other("plain"));
    assert_eq!(plain.to_string(), "plain");
    assert!(plain.source().is_some());
    let optional = Error::Optional { cause: Some(std::io::Error::other("cause")) };
    assert_eq!(optional.source().unwrap().to_string(), "cause");
    fn converts<T: From<concord::Error<Problem>>>() {}
    converts::<Error>();
}
