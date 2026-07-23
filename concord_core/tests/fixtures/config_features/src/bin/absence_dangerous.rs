use concord_core::advanced::HttpConfigBuilder;

fn configure(
    builder: HttpConfigBuilder,
    retry: concord_core::dangerous::ReqwestRetryBuilder,
) -> HttpConfigBuilder {
    builder.dangerous_custom_retry(retry)
}

fn main() {}
