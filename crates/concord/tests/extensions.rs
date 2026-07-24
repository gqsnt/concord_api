mod extensions {
    pub(crate) mod contracts;
    mod oauth2;
    mod rate_limit_composition;
}

pub(crate) use extensions::contracts::{
    ExtensionBufferedSpec, ExtensionConstructionProbe, ExtensionController, ExtensionDiscardSpec,
    ExtensionPageCodec, ExtensionProvider, ExtensionRecordDecoder, ExtensionRequestSpec,
    ExtensionStreamingSpec,
};
