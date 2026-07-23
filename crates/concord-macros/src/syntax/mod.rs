pub(crate) mod ast;

mod credentials;
mod declaration;
mod diagnostics;
mod keyword;
mod pagination;
mod rate_limits;
mod request;
mod values;

const MAX_FORMAT_DEPTH: usize = 64;

pub(crate) use declaration::parse;
