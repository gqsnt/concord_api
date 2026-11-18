// Connect the independent request building and response reading of one endpoint
// to its client's generic call. Each paged endpoint implements Paged separately.
macro_rules! call_spec {
    ($client:ident, $endpoint:path, $inputs:path, $value:ty, $problem:ty, $name:literal) => {
        impl $client::__spec::Spec for $endpoint {
            type Inputs = $inputs;
            const DEBUG: &'static str = $name;

            fn request(
                call: &$client::Call<Self>,
                take: bool,
            ) -> $client::Result<concord::reqwest::Request> {
                call.__build_request(take)
            }

            fn prepare(
                call: $client::Call<Self>,
            ) -> $client::Result<concord::__private::Prepared<$value, $problem>> {
                call.__prepare_inner()
            }

            async fn read(
                call: &$client::Call<Self>,
                response: concord::reqwest::Response,
                info: concord::transport::TryInfo,
            ) -> $client::Result<concord::Answer<$value>> {
                call.__read_answer(response, info).await
            }
        }
    };
}
