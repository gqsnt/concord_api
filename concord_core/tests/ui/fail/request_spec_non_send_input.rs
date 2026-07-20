use std::rc::Rc;

use concord_core::advanced::{
    BodyError, RequestBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
};

struct NonSendInputSpec;

impl RequestBodySpec for NonSendInputSpec {
    type Input = Rc<String>;

    fn metadata(
        _input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        unreachable!()
    }

    fn prepare(
        _input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        unreachable!()
    }
}

fn main() {}
