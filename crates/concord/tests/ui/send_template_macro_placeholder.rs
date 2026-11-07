#[derive(Default)]
struct Policy;

impl<T: Send> concord::SendWith<T> for Policy {
    type Output = Result<T, concord::Error>;
    async fn send<E: concord::Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        endpoint.prepare()?.send().await
    }
}

macro_rules! result {
    ($t:ty) => {
        Result<$t, concord::Error>
    };
}

// A macro's input is its own grammar: its `_` is not the endpoint's output.
concord::api! {
    pub client C() {
        base "https://x.test";
        send Policy -> result!(_);
        GET item() { path "i"; }
    }
}

fn main() {}
