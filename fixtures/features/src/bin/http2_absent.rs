use runtime::http::HttpConfigBuilder;

fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.http2_prior_knowledge()
}

fn main() {}
