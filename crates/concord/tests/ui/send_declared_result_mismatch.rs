#[derive(Default)]
struct Policy;

impl<T: Send> concord::SendWith<T> for Policy {
    type Output = Result<T, concord::Error>;
    async fn send<E: concord::Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        endpoint.prepare()?.send().await
    }
}

concord::api! {
    pub client C() {
        base "https://x.test";
        send Policy -> Option<_>;
        GET item() -> String { path "i"; response concord::Text; }
    }
}

fn main() {}
