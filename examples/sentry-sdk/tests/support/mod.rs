use concord_sdk_sentry::{SendPolicy, Sentry, Settings, http_client};
use concord_testserver::TestServer;
use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

pub fn settings() -> Settings {
    Settings {
        max_attempts: NonZeroUsize::new(1).unwrap(),
        timeout: Duration::from_secs(3),
        requests: NonZeroU32::new(1000).unwrap(),
        window: Duration::from_secs(1),
        initial_delay: Duration::from_millis(50),
        ..Settings::default()
    }
}
pub fn api(server: &TestServer, settings: Settings) -> Sentry {
    Sentry::with_token(
        http_client().unwrap(),
        "synthetic-bearer-secret",
        SendPolicy::new(settings).unwrap(),
    )
    .unwrap()
    .with_base_url(server.url())
    .unwrap()
}
