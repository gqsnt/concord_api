#[derive(Default)]
struct Policy;

impl<T: Send + 'static> concord::SendWith<T> for Policy {
    type Output = Box<[T; 1]>;
    async fn send<E: concord::Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        Box::new([endpoint.prepare().unwrap().send().await.unwrap()])
    }
}

// `Box<[T; 1]>` coerces to `Box<[T]>`, but the declared result must be the policy's output.
concord::api! {
    pub client C() {
        base "https://x.test";
        send Policy -> Box<[_]>;
        GET item() -> String { path "i"; response concord::Text; }
    }
}

fn main() {}
