#![allow(missing_docs)]
use concord::{Endpoint, SendWith};
use concord_ratelimit::{Clock, Limiter, Mode, Observation};
use concord_testserver::{Reply, TestServer};
use serde::Deserialize;
use std::{
    collections::HashMap,
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct ClockImpl(Instant);
impl Clock for ClockImpl {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(tokio::time::sleep(duration))
    }
}
#[derive(Clone)]
pub struct Policy {
    limiter: Limiter<String>,
    aliases: Arc<Mutex<HashMap<String, String>>>,
}
impl<T: Send> SendWith<T> for Policy {
    type Output = Result<T, String>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare().map_err(|e| e.to_string())?;
        // This SDK fixture owns the route projection and learns bucket aliases from headers.
        let route = prepared.request().url().path().to_owned();
        let key = self
            .aliases
            .lock()
            .unwrap()
            .get(&route)
            .cloned()
            .unwrap_or_else(|| route.clone());
        self.limiter
            .acquire(std::slice::from_ref(&key), Mode::Refuse)
            .await
            .map_err(|_| "gated".to_string())?;
        let received = prepared.receive().await.map_err(|e| e.to_string())?;
        if received.status().as_u16() == 429 {
            let global = received
                .head()
                .headers()
                .get("x-ratelimit-global")
                .is_some();
            let delay: f64 = received
                .head()
                .headers()
                .get("retry-after")
                .unwrap()
                .to_str()
                .unwrap()
                .parse()
                .unwrap();
            let end = self
                .limiter
                .after(Duration::try_from_secs_f64(delay).unwrap())
                .unwrap();
            if global {
                self.limiter.observe_global(end);
            } else {
                self.limiter
                    .observe(key, Observation::Cooldown(end))
                    .unwrap();
            }
            return Err("429".into());
        }
        let remaining = received
            .head()
            .headers()
            .get("x-ratelimit-remaining")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        let delay: f64 = received
            .head()
            .headers()
            .get("x-ratelimit-reset-after")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        let bucket = received
            .head()
            .headers()
            .get("x-ratelimit-bucket")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        self.aliases.lock().unwrap().insert(route, bucket.clone());
        self.limiter
            .observe(
                bucket,
                Observation::Window {
                    remaining,
                    reset_at: self
                        .limiter
                        .after(Duration::try_from_secs_f64(delay).unwrap())
                        .unwrap(),
                },
            )
            .unwrap();
        received.decode().await.map_err(|e| e.to_string())
    }
}
#[derive(Debug, Deserialize)]
pub struct Message {
    id: u64,
}
concord::api! {
    pub client Discord(policy: Policy) {
        base "http://discord.invalid";
        send policy;
        format concord::Json;
        GET messages(channel: u64) -> Message { path "channels/{channel}/messages"; }
        GET global() -> Message { path "global"; }
    }
}
#[tokio::test]
async fn routes_share_bucket_and_global_429_gates_other_bucket() {
    let server = TestServer::start(|r| {
        if r.path() == "/global" {
            Reply::json(429, "{} ")
                .header("x-ratelimit-global", "true")
                .header("retry-after", "1.25")
        } else {
            Reply::json(200, "{\"id\":7}")
                .header("x-ratelimit-bucket", "shared")
                .header(
                    "x-ratelimit-remaining",
                    if r.path().contains("/1/") { "1" } else { "0" },
                )
                .header("x-ratelimit-reset-after", "10")
        }
    })
    .await;
    let limiter = Limiter::new(NonZeroUsize::new(8).unwrap(), ClockImpl(Instant::now()));
    let policy = Policy {
        limiter,
        aliases: Arc::new(Mutex::new(HashMap::new())),
    };
    let client = Discord::new(concord::reqwest::Client::new(), DiscordConfig { policy })
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(client.messages(1).send().await.unwrap().id, 7);
    assert_eq!(client.messages(2).send().await.unwrap().id, 7);
    assert_eq!(client.messages(1).send().await.unwrap_err(), "gated");
    assert_eq!(client.global().send().await.unwrap_err(), "429");
    assert_eq!(client.messages(3).send().await.unwrap_err(), "gated");
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

concord::api! {
    pub client Streaming {
        base "http://stream.invalid";
        GET events() -> stream { path "events"; }
    }
}
#[tokio::test]
async fn lease_held_until_stream_completion() {
    let server = TestServer::start(|_| Reply::text(200, "text/plain", "events")).await;
    let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), ClockImpl(Instant::now()));
    limiter
        .set_concurrency("stream", NonZeroUsize::new(1).unwrap())
        .unwrap();
    let lease = limiter.lease("stream", Mode::Wait).await.unwrap();
    let client = Streaming::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let stream = client.events().send().await.unwrap();
    assert!(limiter.lease("stream", Mode::Refuse).await.is_err());
    assert_eq!(stream.drain().await.unwrap(), 6);
    drop(lease);
    assert!(limiter.lease("stream", Mode::Refuse).await.is_ok());
    server.shutdown().await;
}
