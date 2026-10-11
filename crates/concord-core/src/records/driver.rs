use std::fmt;
use std::num::NonZeroUsize;

use bytes::{Bytes, BytesMut};

use super::{RecordDecoder, RecordError, RecordErrorKind};

pub(super) enum RecordStep<R> {
    Record(R),
    NeedInput,
    Done,
}

pub(super) struct RecordDriver<D>
where
    D: RecordDecoder,
{
    decoder: D,
    staging: BytesMut,
    pending: Bytes,
    max_staged_bytes: usize,
    source_finished: bool,
    eof_decoding: bool,
    input_requested: bool,
    done: bool,
}

impl<D> RecordDriver<D>
where
    D: RecordDecoder,
{
    pub(super) fn new(decoder: D, max_staged_bytes: NonZeroUsize) -> Self {
        Self {
            decoder,
            staging: BytesMut::new(),
            pending: Bytes::new(),
            max_staged_bytes: max_staged_bytes.get(),
            source_finished: false,
            eof_decoding: false,
            input_requested: false,
            done: false,
        }
    }

    pub(super) const fn needs_input(&self) -> bool {
        self.input_requested
    }

    pub(super) fn supply_input(&mut self, chunk: Bytes) -> Result<(), RecordError> {
        if self.done || self.source_finished || !self.input_requested || !self.pending.is_empty() {
            return Err(self.terminate(RecordErrorKind::Internal));
        }
        self.input_requested = false;
        self.pending = chunk;
        Ok(())
    }

    pub(super) fn finish_input(&mut self) -> Result<(), RecordError> {
        if self.done || self.source_finished || !self.input_requested || !self.pending.is_empty() {
            return Err(self.terminate(RecordErrorKind::Internal));
        }
        self.input_requested = false;
        self.source_finished = true;
        Ok(())
    }

    pub(super) fn advance(&mut self) -> Result<RecordStep<D::Record>, RecordError> {
        if self.done {
            return Ok(RecordStep::Done);
        }
        if self.input_requested {
            return Err(self.terminate(RecordErrorKind::Internal));
        }

        loop {
            let before = self.staging.len();
            let decoded = if self.eof_decoding {
                self.decoder.decode_eof(&mut self.staging)
            } else {
                self.decoder.decode(&mut self.staging)
            };
            let after = self.staging.len();

            if after > before || matches!(&decoded, Ok(Some(_)) if after >= before) {
                return Err(self.terminate(RecordErrorKind::Internal));
            }
            if after > self.max_staged_bytes {
                return Err(self.terminate(RecordErrorKind::LimitExceeded));
            }

            match decoded {
                Ok(Some(record)) => return Ok(RecordStep::Record(record)),
                Err(error) => return Err(self.terminate_with(error)),
                Ok(None) if after < before => continue,
                Ok(None) if self.eof_decoding => {
                    if self.staging.is_empty() {
                        self.done = true;
                        self.release_bytes();
                        return Ok(RecordStep::Done);
                    }
                    return Err(self.terminate(RecordErrorKind::IncompleteRecord));
                }
                Ok(None) => {}
            }

            if !self.pending.is_empty() {
                let remaining = self.max_staged_bytes - self.staging.len();
                if remaining == 0 {
                    return Err(self.terminate(RecordErrorKind::LimitExceeded));
                }
                let amount = remaining.min(self.pending.len());
                let staged = self.pending.split_to(amount);
                self.staging.extend_from_slice(&staged);
                if self.pending.is_empty() {
                    self.pending = Bytes::new();
                }
                continue;
            }

            if self.source_finished {
                self.eof_decoding = true;
                continue;
            }

            self.input_requested = true;
            return Ok(RecordStep::NeedInput);
        }
    }

    pub(super) fn terminate(&mut self, kind: RecordErrorKind) -> RecordError {
        self.terminate_with(RecordError::new(kind))
    }

    fn terminate_with(&mut self, error: RecordError) -> RecordError {
        self.done = true;
        self.input_requested = false;
        self.release_bytes();
        error
    }

    fn release_bytes(&mut self) {
        self.staging = BytesMut::new();
        self.pending = Bytes::new();
    }

    /// Ends decoding and releases staged and pending bytes synchronously, for
    /// example when the owning stream terminates for a non-record reason.
    pub(super) fn release(&mut self) {
        self.done = true;
        self.input_requested = false;
        self.release_bytes();
    }

    #[cfg(test)]
    pub(super) fn staging_is_empty(&self) -> bool {
        self.staging.is_empty() && self.pending.is_empty()
    }
}

impl<D> fmt::Debug for RecordDriver<D>
where
    D: RecordDecoder,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordDriver")
            .field("source_finished", &self.source_finished)
            .field("eof_decoding", &self.eof_decoding)
            .field("input_requested", &self.input_requested)
            .field("done", &self.done)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::response::ResponseHead;

    struct LineDecoder;

    impl RecordDecoder for LineDecoder {
        type Record = Vec<u8>;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            Ok(Self)
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
            NonZeroUsize::new(8).expect("positive line-decoder bound")
        }
    }

    fn collect_lines(
        chunks: impl IntoIterator<Item = Bytes>,
        max_staged_bytes: usize,
    ) -> Result<Vec<Vec<u8>>, RecordError> {
        let mut chunks = chunks.into_iter().collect::<VecDeque<_>>();
        let mut driver = RecordDriver::new(
            LineDecoder,
            NonZeroUsize::new(max_staged_bytes).expect("positive test staging bound"),
        );
        let mut records = Vec::new();
        loop {
            match driver.advance()? {
                RecordStep::Record(record) => records.push(record),
                RecordStep::NeedInput => {
                    if let Some(chunk) = chunks.pop_front() {
                        driver.supply_input(chunk)?;
                    } else {
                        driver.finish_input()?;
                    }
                }
                RecordStep::Done => return Ok(records),
            }
        }
    }

    #[test]
    fn large_source_chunk_with_many_small_records_is_staged_incrementally() {
        let body = Bytes::from_static(b"a\nb\nc\nd\ne\nf\n");
        assert!(body.len() > 2);
        assert_eq!(
            collect_lines([body], 2).expect("small logical records"),
            [b"a", b"b", b"c", b"d", b"e", b"f"]
                .map(|record| record.to_vec())
                .to_vec()
        );
    }

    #[test]
    fn every_chunk_boundary_and_one_byte_chunks_have_identical_results() {
        const BODY: &[u8] = b"alpha\nb\ncharlie\nlast";
        let expected = collect_lines([Bytes::from_static(BODY)], 8).expect("one source chunk");

        for split in 0..=BODY.len() {
            let chunks = [
                Bytes::copy_from_slice(&BODY[..split]),
                Bytes::copy_from_slice(&BODY[split..]),
            ];
            assert_eq!(
                collect_lines(chunks, 8).expect("two source chunks"),
                expected,
                "different records at source split {split}"
            );
        }

        let one_byte = BODY
            .iter()
            .copied()
            .map(|byte| Bytes::copy_from_slice(&[byte]))
            .collect::<Vec<_>>();
        assert_eq!(
            collect_lines(one_byte, 8).expect("one-byte source chunks"),
            expected
        );
    }

    #[test]
    fn exact_staging_limit_succeeds_and_first_excess_is_never_staged() {
        assert_eq!(
            collect_lines([Bytes::from_static(b"abcd")], 4).expect("exact EOF record"),
            [b"abcd".to_vec()]
        );

        let maximum_seen = Arc::new(AtomicUsize::new(0));
        let mut driver = RecordDriver::new(
            TrackingDecoder {
                maximum_seen: Arc::clone(&maximum_seen),
            },
            NonZeroUsize::new(4).expect("positive test staging bound"),
        );
        assert!(matches!(
            driver.advance().expect("initial progress"),
            RecordStep::NeedInput
        ));
        driver
            .supply_input(Bytes::from_static(b"abcde"))
            .expect("requested source chunk");
        let error = match driver.advance() {
            Err(error) => error,
            Ok(_) => panic!("first logical-record excess must fail"),
        };
        assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);
        assert_eq!(maximum_seen.load(Ordering::SeqCst), 4);
        assert!(driver.staging.is_empty());
        assert!(driver.pending.is_empty());
    }

    struct TrackingDecoder {
        maximum_seen: Arc<AtomicUsize>,
    }

    impl RecordDecoder for TrackingDecoder {
        type Record = Vec<u8>;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            unreachable!("tracking decoder is constructed directly")
        }

        fn decode(&mut self, staging: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            self.maximum_seen.fetch_max(staging.len(), Ordering::SeqCst);
            Ok(None)
        }

        fn decode_eof(
            &mut self,
            staging: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            self.maximum_seen.fetch_max(staging.len(), Ordering::SeqCst);
            Ok(None)
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(4).expect("positive tracking bound")
        }
    }

    #[derive(Clone, Copy)]
    enum ContractBehavior {
        SomeWithoutConsumption,
        Growth,
        ConsumeNoneThenYield,
    }

    struct ContractDecoder {
        behavior: ContractBehavior,
        consumed: bool,
    }

    impl RecordDecoder for ContractDecoder {
        type Record = Vec<u8>;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            unreachable!("contract decoder is constructed directly")
        }

        fn decode(&mut self, staging: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            match self.behavior {
                ContractBehavior::SomeWithoutConsumption if !staging.is_empty() => {
                    Ok(Some(staging.to_vec()))
                }
                ContractBehavior::Growth => {
                    staging.extend_from_slice(b"decoder-growth-secret");
                    Ok(None)
                }
                ContractBehavior::ConsumeNoneThenYield if !staging.is_empty() => {
                    if self.consumed {
                        Ok(Some(staging.split_to(1).to_vec()))
                    } else {
                        drop(staging.split_to(1));
                        self.consumed = true;
                        Ok(None)
                    }
                }
                ContractBehavior::SomeWithoutConsumption
                | ContractBehavior::ConsumeNoneThenYield => Ok(None),
            }
        }

        fn decode_eof(
            &mut self,
            _staging: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            Ok(None)
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(64).expect("positive contract bound")
        }
    }

    fn contract_driver(behavior: ContractBehavior) -> RecordDriver<ContractDecoder> {
        RecordDriver::new(
            ContractDecoder {
                behavior,
                consumed: false,
            },
            NonZeroUsize::new(64).expect("positive contract bound"),
        )
    }

    #[test]
    fn callback_invariants_and_consuming_none_are_enforced() {
        let mut no_consumption = contract_driver(ContractBehavior::SomeWithoutConsumption);
        assert!(matches!(
            no_consumption.advance().expect("initial progress"),
            RecordStep::NeedInput
        ));
        no_consumption
            .supply_input(Bytes::from_static(b"xy"))
            .expect("requested input");
        let error = match no_consumption.advance() {
            Err(error) => error,
            Ok(_) => panic!("record without consumption must fail"),
        };
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert!(no_consumption.staging.is_empty());
        assert!(no_consumption.pending.is_empty());

        let mut growth = contract_driver(ContractBehavior::Growth);
        let error = match growth.advance() {
            Err(error) => error,
            Ok(_) => panic!("decoder growth must fail"),
        };
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert!(growth.staging.is_empty());
        assert!(growth.pending.is_empty());

        let mut driver = contract_driver(ContractBehavior::ConsumeNoneThenYield);
        assert!(matches!(
            driver.advance().expect("initial progress"),
            RecordStep::NeedInput
        ));
        driver
            .supply_input(Bytes::from_static(b"xy"))
            .expect("requested input");
        let RecordStep::Record(record) = driver.advance().expect("record after consuming None")
        else {
            panic!("consuming None must continue synchronously");
        };
        assert_eq!(record, b"y");
    }

    struct EofByteDecoder;

    impl RecordDecoder for EofByteDecoder {
        type Record = u8;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            Ok(Self)
        }

        fn decode(&mut self, _staging: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            Ok(None)
        }

        fn decode_eof(
            &mut self,
            staging: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            if staging.is_empty() {
                Ok(None)
            } else {
                Ok(Some(staging.split_to(1)[0]))
            }
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(8).expect("positive EOF bound")
        }
    }

    #[test]
    fn eof_can_yield_multiple_records_and_rejects_an_incomplete_tail() {
        let mut driver = RecordDriver::new(
            EofByteDecoder,
            NonZeroUsize::new(8).expect("positive EOF bound"),
        );
        assert!(matches!(
            driver.advance().expect("initial progress"),
            RecordStep::NeedInput
        ));
        driver
            .supply_input(Bytes::from_static(b"abc"))
            .expect("requested input");
        assert!(matches!(
            driver.advance().expect("staged input"),
            RecordStep::NeedInput
        ));
        driver.finish_input().expect("requested EOF");
        for expected in b"abc" {
            let RecordStep::Record(actual) = driver.advance().expect("EOF record") else {
                panic!("EOF decoder must yield all records");
            };
            assert_eq!(actual, *expected);
        }
        assert!(matches!(
            driver.advance().expect("EOF completion"),
            RecordStep::Done
        ));

        let error = collect_incomplete_tail().expect_err("incomplete EOF tail");
        assert_eq!(error.kind(), RecordErrorKind::IncompleteRecord);
    }

    fn collect_incomplete_tail() -> Result<(), RecordError> {
        let mut driver = RecordDriver::new(
            NeverCompletesDecoder,
            NonZeroUsize::new(8).expect("positive incomplete bound"),
        );
        assert!(matches!(driver.advance()?, RecordStep::NeedInput));
        driver.supply_input(Bytes::from_static(b"tail"))?;
        assert!(matches!(driver.advance()?, RecordStep::NeedInput));
        driver.finish_input()?;
        let _ = driver.advance()?;
        Ok(())
    }

    struct NeverCompletesDecoder;

    impl RecordDecoder for NeverCompletesDecoder {
        type Record = ();

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            Ok(Self)
        }

        fn decode(&mut self, _staging: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            Ok(None)
        }

        fn decode_eof(
            &mut self,
            _staging: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            Ok(None)
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(8).expect("positive incomplete bound")
        }
    }

    #[test]
    fn misuse_is_internal_and_debug_omits_decoder_staging_and_pending_bytes() {
        let mut misuse = RecordDriver::new(
            LineDecoder,
            NonZeroUsize::new(8).expect("positive line bound"),
        );
        let error = misuse
            .supply_input(Bytes::from_static(b"unsolicited"))
            .expect_err("input was not requested");
        assert_eq!(error.kind(), RecordErrorKind::Internal);

        let mut driver = RecordDriver::new(
            SecretDebugDecoder("decoder-debug-secret"),
            NonZeroUsize::new(64).expect("positive debug bound"),
        );
        assert!(matches!(
            driver.advance().expect("initial progress"),
            RecordStep::NeedInput
        ));
        driver
            .supply_input(Bytes::from_static(b"pending-and-staging-secret"))
            .expect("requested debug input");
        let pending_debug = format!("{driver:?}");
        assert!(!pending_debug.contains("pending-and-staging-secret"));
        assert!(!pending_debug.contains("decoder-debug-secret"));
        assert!(matches!(
            driver.advance().expect("stage debug input"),
            RecordStep::NeedInput
        ));
        let staging_debug = format!("{driver:?}");
        assert!(!staging_debug.contains("pending-and-staging-secret"));
        assert!(!staging_debug.contains("decoder-debug-secret"));
    }

    struct SecretDebugDecoder(&'static str);

    impl RecordDecoder for SecretDebugDecoder {
        type Record = ();

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            unreachable!("debug decoder is constructed directly")
        }

        fn decode(&mut self, _staging: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            let _ = self.0;
            Ok(None)
        }

        fn decode_eof(
            &mut self,
            _staging: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            Ok(None)
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(64).expect("positive debug bound")
        }
    }
}
