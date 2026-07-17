use std::time::Duration;

use concord_core::advanced::{HttpConfigBuilder, ProviderRetry};

fn configure(mut builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder = builder
        .connect_timeout(Duration::from_secs(1))
        .retry_reqwest_default()
        .provider_retry(ProviderRetry::ProtocolRecovery);

    #[cfg(feature = "http2")]
    {
        builder = builder.http2_prior_knowledge();
    }
    #[cfg(feature = "default-tls")]
    {
        builder = builder
            .add_root_certificate_pem(Vec::new())
            .identity_pem(Vec::new());
    }
    #[cfg(feature = "gzip")]
    {
        builder = builder.disable_gzip();
    }
    #[cfg(feature = "brotli")]
    {
        builder = builder.disable_brotli();
    }
    #[cfg(feature = "deflate")]
    {
        builder = builder.disable_deflate();
    }
    builder
}

#[cfg(feature = "dangerous-custom-retry")]
fn dangerous(
    builder: HttpConfigBuilder,
    retry: concord_core::dangerous::ReqwestRetryBuilder,
) -> HttpConfigBuilder {
    builder.dangerous_retry(retry)
}

fn main() {
    let _: fn(HttpConfigBuilder) -> HttpConfigBuilder = configure;
    #[cfg(feature = "dangerous-custom-retry")]
    let _: fn(
        HttpConfigBuilder,
        concord_core::dangerous::ReqwestRetryBuilder,
    ) -> HttpConfigBuilder = dangerous;
    #[cfg(feature = "json")]
    let _: std::marker::PhantomData<concord_core::prelude::Json<String>> = std::marker::PhantomData;
}
