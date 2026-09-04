mod extensions {
    pub(crate) mod contracts;
    mod oauth2;
    mod rate_limit_composition;
}

pub(crate) use extensions::contracts::{
    EmptyChunkRequestSpec, ExtensionBufferedSpec, ExtensionConstructionProbe, ExtensionController,
    ExtensionDiscardSpec, ExtensionPageCodec, ExtensionProvider, ExtensionRequestSpec,
    ExtensionStreamingSpec,
};
