mod extensions {
    pub(crate) mod contracts;
    mod oauth2;
    mod rate_limit_composition;
}

#[cfg(feature = "records")]
pub(crate) use extensions::contracts::ExtensionRecordDecoder;
pub(crate) use extensions::contracts::{
    ExtensionBufferedSpec, ExtensionConstructionProbe, ExtensionController, ExtensionDiscardSpec,
    ExtensionPageCodec, ExtensionProvider, ExtensionRequestSpec, ExtensionStreamingSpec,
};
