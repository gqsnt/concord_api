use runtime::http::HttpConfigBuilder;

fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.dangerous_custom_retry(())
}

fn main() {}
