pub(crate) mod ast;

mod credentials;
mod declaration;
pub(crate) mod diagnostics;
mod keyword;
mod pagination;
mod rate_limits;
mod request;
mod values;

pub(crate) use declaration::parse;
