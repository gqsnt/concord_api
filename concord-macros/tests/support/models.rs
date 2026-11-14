// These compiler fixtures test DSL mistakes, not JSON or provider data. Recovery
// now emits their sound siblings too. Supply the pages' external macro names and
// the one scalar id needed by the cross-base gate's early trait selection. The
// complete provider models are compiled separately by real-apis.
#[allow(unused_macros, unused_imports)]
mod __real_fixture {
    macro_rules! inert_json { ($($tokens:tt)*) => { () }; }
    pub(crate) use inert_json;
}
#[allow(unused_imports)]
use __real_fixture::{inert_json as json, inert_json as j};
#[allow(unused_imports)]
mod serde_json {
    pub(crate) use super::__real_fixture::inert_json as json;
}
#[allow(dead_code)]
type AccountId = String;
