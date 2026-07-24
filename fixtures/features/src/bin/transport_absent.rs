#[cfg(feature = "fixture-absence")]
use runtime::http::HttpConfigBuilder;

#[cfg(feature = "fixture-absence")]
fn configure_tls(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.add_root_certificate_pem(Vec::new())
}

#[cfg(feature = "fixture-absence")]
fn configure_http2(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.http2_prior_knowledge()
}

#[cfg(feature = "fixture-absence")]
fn configure_gzip(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.disable_gzip()
}

#[cfg(feature = "fixture-absence")]
fn configure_brotli(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.disable_brotli()
}

#[cfg(feature = "fixture-absence")]
fn configure_deflate(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.disable_deflate()
}

fn main() {}
