use tokio::runtime::{Builder, Runtime};

pub fn runtime() -> Runtime {
    Builder::new_current_thread()
        .build()
        .expect("failed to build benchmark runtime")
}
