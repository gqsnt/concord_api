use concord_core::advanced::HttpConfigBuilder;

fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.disable_deflate()
}

fn main() {}
