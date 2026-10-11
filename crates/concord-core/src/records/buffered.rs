#[cfg(test)]
use std::marker::PhantomData;

use bytes::Bytes;

#[cfg(test)]
use crate::response::{Buffered, ResponseDriver, ResponseRepresentation};
use crate::response::{ResponseBodyError, ResponseHead};

use super::driver::{RecordDriver, RecordStep};
use super::error::response_error;
use super::{RecordDecoder, RecordError};

/// A complete bounded accepted-response specification decoded into records by `D`.
///
/// Concord first acquires the complete response within the endpoint's aggregate
/// response-body limit. The shared record driver then incrementally stages and
/// decodes those bytes within `D`'s independent logical-record limit. The
/// result is all records in source order or one error; a partial vector is
/// never returned.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct BufferedRecords<D>(PhantomData<fn() -> D>);

#[cfg(test)]
impl<D> ResponseRepresentation<Vec<D::Record>> for BufferedRecords<D>
where
    D: RecordDecoder,
{
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = D::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<D>(head)
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Vec<D::Record>, ResponseBodyError> {
        decode_complete_records::<D>(body, head)
    }
}

/// Validates one bounded record decoder without exposing native response authority.
#[doc(hidden)]
pub fn validate_record_decoder<D>(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError>
where
    D: RecordDecoder,
{
    D::validate(head).map_err(response_error)
}

/// Decodes a completely acquired bounded body with one semantic record decoder.
#[doc(hidden)]
pub fn decode_complete_records<D>(
    body: Bytes,
    head: &ResponseHead<'_>,
) -> Result<Vec<D::Record>, ResponseBodyError>
where
    D: RecordDecoder,
{
    let decoder = D::start(head).map_err(response_error)?;
    collect_records(body, decoder).map_err(response_error)
}

pub(super) fn collect_records<D>(body: Bytes, decoder: D) -> Result<Vec<D::Record>, RecordError>
where
    D: RecordDecoder,
{
    let mut body = Some(body);
    let mut driver = RecordDriver::new(decoder, D::max_buffered_bytes());
    let mut records = Vec::new();

    loop {
        match driver.advance()? {
            RecordStep::Record(record) => records.push(record),
            RecordStep::NeedInput => {
                if let Some(body) = body.take() {
                    driver.supply_input(body)?;
                } else {
                    driver.finish_input()?;
                }
            }
            RecordStep::Done => return Ok(records),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bytes::{Bytes, BytesMut};

    use super::*;
    use crate::records::RecordErrorKind;

    struct LineDecoder;

    impl RecordDecoder for LineDecoder {
        type Record = Vec<u8>;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            unreachable!("the collection helper receives a started decoder")
        }

        fn decode(&mut self, staging: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            let Some(end) = staging.iter().position(|byte| *byte == b'\n') else {
                return Ok(None);
            };
            let mut record = staging.split_to(end + 1);
            record.truncate(end);
            Ok(Some(record.to_vec()))
        }

        fn decode_eof(
            &mut self,
            staging: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            if staging.is_empty() {
                Ok(None)
            } else {
                Ok(Some(staging.split_to(staging.len()).to_vec()))
            }
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(8).expect("positive test staging bound")
        }
    }

    #[test]
    fn complete_collection_is_ordered_and_empty_bodies_are_empty() {
        assert_eq!(
            collect_records(Bytes::from_static(b"first\nsecond\nlast"), LineDecoder)
                .expect("complete records"),
            [&b"first"[..], &b"second"[..], &b"last"[..]]
                .map(|record| record.to_vec())
                .to_vec()
        );
        assert!(
            collect_records(Bytes::new(), LineDecoder)
                .expect("empty response")
                .is_empty()
        );
    }

    #[test]
    fn one_large_source_chunk_is_distinct_from_bounded_record_staging() {
        let body = Bytes::from_static(b"a\nb\nc\nd\ne\nf\n");
        assert!(body.len() > LineDecoder::max_buffered_bytes().get());
        assert_eq!(
            collect_records(body, LineDecoder).expect("small logical records"),
            [b"a", b"b", b"c", b"d", b"e", b"f"]
                .map(|record| record.to_vec())
                .to_vec()
        );
    }

    struct FailingAfterOne;

    impl RecordDecoder for FailingAfterOne {
        type Record = Vec<u8>;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            unreachable!("the collection helper receives a started decoder")
        }

        fn decode(&mut self, staging: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            if staging.starts_with(b"ok\n") {
                return Ok(Some(staging.split_to(3).to_vec()));
            }
            if staging.starts_with(b"bad\n") {
                return Err(RecordError::new(RecordErrorKind::Decode));
            }
            Ok(None)
        }

        fn decode_eof(
            &mut self,
            staging: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            self.decode(staging)
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(4).expect("positive test staging bound")
        }
    }

    #[test]
    fn a_late_failure_never_returns_the_partial_vector() {
        let error = collect_records(Bytes::from_static(b"ok\nbad\n"), FailingAfterOne)
            .expect_err("second record fails");
        assert_eq!(error.kind(), RecordErrorKind::Decode);
    }
}
