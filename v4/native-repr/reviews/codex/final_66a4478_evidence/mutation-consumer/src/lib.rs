//! Independent downstream acceptance probes; renamed Concord, no reqwest default features.
wire::api! {
    pub client Unused {
        base "http://127.0.0.1";
        request DoesNotExist;
        response DoesNotExistEither;
        GET live() -> text String;
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
