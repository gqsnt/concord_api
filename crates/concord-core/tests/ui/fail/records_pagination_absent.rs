use std::num::NonZeroUsize;

use concord_core::advanced::{BytesMut, PageResponseSpec, ResponseHead};
use concord_core::records::{RecordDecoder, RecordError, Records};

struct Decoder;

impl RecordDecoder for Decoder {
    type Record = u32;

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, _buffer: &mut BytesMut) -> Result<Option<u32>, RecordError> {
        Ok(None)
    }

    fn decode_eof(&mut self, _buffer: &mut BytesMut) -> Result<Option<u32>, RecordError> {
        Ok(None)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::MIN
    }
}

fn requires_page_response<R: PageResponseSpec>() {}

fn main() {
    requires_page_response::<Records<Decoder>>();
}
