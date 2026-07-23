use concord_core::advanced::ApplicationHttp;
use concord_core::advanced::HttpConfigBuilder;
use concord_core::advanced::ProviderHttpInner;

fn raw_client_escape(builder: HttpConfigBuilder) {
    let _ = builder.client();
}

fn raw_builder_escape(builder: HttpConfigBuilder) {
    let _ = builder.reqwest_builder();
}

fn name_private_types(_: ApplicationHttp, _: ProviderHttpInner) {}

fn main() {}
