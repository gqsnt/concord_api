use runtime::http::HttpConfigBuilder;

fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.disable_brotli()
}

fn main() {}
