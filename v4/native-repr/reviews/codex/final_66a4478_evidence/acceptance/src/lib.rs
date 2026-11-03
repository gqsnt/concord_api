//! Independent downstream acceptance probes; renamed Concord, no reqwest default features.
wire::api! {
    pub client Unused {
        base "http://127.0.0.1";
        request DoesNotExist;
        response DoesNotExistEither;
        GET live() -> text String;
    }
}

#[cfg(feature = "codecs")]
wire::api! {
    /// Fixture for visual inspection of generated endpoint rustdoc.
    pub client Docs(header("x-native") native: wire::reqwest::header::HeaderValue) {
        base "http://127.0.0.1";
        request json;
        response json;
        GET ignored();
        GET typed_unit() -> ();
        POST submit(body input: String) -> String;
        POST custom(text("application/custom") input?: String) -> text String;
        scope inherited {
            request bytes;
            POST upload(body b: Vec<u8>) -> bytes Vec<u8>;
        }
    }
}
wire::api! {
    pub client FeatureGated {
        base "http://127.0.0.1";
        request json;
        response form;
        #[cfg(any())]
        scope gone {
            POST never(body p: Unknown) -> AlsoUnknown;
        }
        GET live();
    }
}
