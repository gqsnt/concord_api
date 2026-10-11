use std::rc::Rc;

use concord_core::contracts::{BodyError, PreparedBody, RequestBodyContext, RequestRepresentation};

struct NonSendInputSpec;

impl RequestRepresentation<Rc<String>> for NonSendInputSpec {
    fn prepare(
        _input: Rc<String>,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        unreachable!()
    }
}

fn main() {}
