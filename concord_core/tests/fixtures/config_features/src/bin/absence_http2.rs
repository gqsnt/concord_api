use concord_core::advanced::HttpConfigBuilder;

fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.http2_prior_knowledge()
}

fn main() {}
