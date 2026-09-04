use std::rc::Rc;

use concord_core::contracts::{
    BodyError, RequestBody, RequestBodyContext, RequestBodyMetadata, RequestRepresentation,
};

struct NonSendInputSpec;

impl RequestRepresentation<Rc<String>> for NonSendInputSpec {
    fn metadata(
        _input: &Rc<String>,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        unreachable!()
    }

    fn prepare(
        _input: Rc<String>,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        unreachable!()
    }
}

fn main() {}
