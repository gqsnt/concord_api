use concord_core::advanced::HttpConfigBuilder;

fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.disable_brotli()
}

fn main() {}
