use concord_ratelimit::Clock;
use concord_sdk_arxiv::{Arxiv, ArxivConfig, Policy, http_client};
use concord_testserver::TestServer;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

pub const FIRST: &str = include_str!("../../fixtures/search-0.xml");
pub const SECOND: &str = include_str!("../../fixtures/search-1.xml");
pub const IDS: &str = include_str!("../../fixtures/ids.xml");
pub const EMPTY: &str = include_str!("../../fixtures/empty.xml");
pub const ERROR: &str = include_str!("../../fixtures/error-id.xml");

/// Deterministic virtual elapsed time; only fixture clients use this clock.
#[derive(Clone, Default)]
pub struct VirtualClock(Arc<Mutex<(Duration, Vec<Duration>)>>);
impl VirtualClock {
    pub fn waits(&self) -> Vec<Duration> {
        self.0.lock().unwrap().1.clone()
    }
}
impl Clock for VirtualClock {
    fn now(&self) -> Duration {
        self.0.lock().unwrap().0
    }
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let state = self.0.clone();
        Box::pin(async move {
            let mut state = state.lock().unwrap();
            state.0 += duration;
            state.1.push(duration);
        })
    }
}
pub fn client(server: &TestServer) -> Arxiv {
    Arxiv::new(
        http_client().unwrap(),
        ArxivConfig {
            policy: Policy::with_clock(VirtualClock::default()),
        },
    )
    .with_base_url(format!("{}/api", server.url()))
    .unwrap()
}
