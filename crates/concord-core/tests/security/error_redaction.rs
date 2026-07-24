use std::error::Error;

use concord_core::contracts::CodecError;

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
fn application_codec_text_is_not_an_ordinary_diagnostic_channel() {
    let error = CodecError::new(format!("decoder observed {CODEC_SENTINEL}"));
    assert_recursive_chain_omits(&error, CODEC_SENTINEL);
}
