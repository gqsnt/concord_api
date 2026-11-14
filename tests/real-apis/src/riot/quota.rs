use concord::reqwest::{self, header::HeaderMap};
use concord::transport::{TransportError, TryInfo};
use concord::{Middleware, Next};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{sleep_until, Instant};

/// Riot's limits: a bucket for the application on each host, and one for each method. A bucket
/// that the responses show full is held, and a request waits for its two buckets before it is sent.
#[derive(Clone, Default)]
pub struct Quota(Arc<Mutex<HashMap<String, Instant>>>);

impl Quota {
    pub fn new() -> Quota {
        Quota::default()
    }

    fn held(&self, bucket: &str) -> Option<Instant> {
        let now = Instant::now();
        let buckets = self.0.lock().unwrap_or_else(|e| e.into_inner());
        buckets.get(bucket).copied().filter(|until| *until > now)
    }

    fn hold(&self, bucket: &str, wait: Duration) {
        let until = Instant::now() + wait;
        let mut buckets = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let slot = buckets.entry(bucket.to_owned()).or_insert(until);
        *slot = (*slot).max(until);
    }
}

impl Middleware for Quota {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let app = request.url().host_str().unwrap_or_default().to_owned();  // euw1.api.riotgames.com
        let method = format!("{app} {}", info.endpoint());                   // … riot.league.entries
        while let Some(until) = self.held(&app).max(self.held(&method)) {
            sleep_until(until).await;
        }
        let answer = next.send(request, info).await?;
        let headers = answer.headers();
        for (bucket, limits, counts) in [
            (&app, "x-app-rate-limit", "x-app-rate-limit-count"),
            (&method, "x-method-rate-limit", "x-method-rate-limit-count"),
        ] {
            if let Some(window) = full_window(headers, limits, counts) {
                self.hold(bucket, window);      // a whole window: Riot does not say when it began
            }
        }
        if answer.status() == 429 {
            let wait = seconds(headers, "retry-after").unwrap_or(Duration::from_secs(1));
            match headers.get("x-rate-limit-type").and_then(|v| v.to_str().ok()) {
                Some("application") => self.hold(&app, wait),
                _ => self.hold(&method, wait),
            }
        }
        Ok(answer)
    }
}

/// The longest window whose count has reached its limit: `20:1,100:120` against `20:1,31:120`.
fn full_window(headers: &HeaderMap, limits: &str, counts: &str) -> Option<Duration> {
    let pairs = |name: &str| -> Vec<(u64, u64)> {
        let text = headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
        text.split(',').filter_map(|pair| {
            let (n, seconds) = pair.trim().split_once(':')?;
            Some((n.parse().ok()?, seconds.parse().ok()?))
        }).collect()
    };
    let counts = pairs(counts);
    pairs(limits).into_iter()
        .filter(|(limit, window)| counts.iter().any(|(n, w)| w == window && n >= limit))
        .map(|(_, window)| Duration::from_secs(window))
        .max()
}

fn seconds(headers: &HeaderMap, name: &str) -> Option<Duration> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok().map(Duration::from_secs)
}
