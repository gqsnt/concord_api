use std::error::Error;

use concord_core::contracts::{CodecError, CodecErrorKind};

const CODEC_SENTINEL: &str = "codec-body-sentinel-298b23";

fn assert_recursive_chain_omits(error: &(dyn Error + 'static), sentinel: &str) {
    let mut current = Some(error);
    while let Some(error) = current {
        assert!(!error.to_string().contains(sentinel));
        assert!(!format!("{error:?}").contains(sentinel));
        current = error.source();
    }
}

#[test]
fn codec_errors_are_finite_categories_without_a_message_channel() {
    let error = CodecError::new(CodecErrorKind::Decode);
    assert_eq!(error.kind(), CodecErrorKind::Decode);
    assert_recursive_chain_omits(&error, CODEC_SENTINEL);
}
