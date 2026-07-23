use runtime::http::HttpConfigBuilder;

fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder
        .add_root_certificate_pem(Vec::new())
        .identity_pem(Vec::new())
}

fn main() {}
