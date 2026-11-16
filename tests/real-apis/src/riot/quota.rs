use concord::limit::{counts, Bucket, Limit};
use std::time::Duration;

/// Riot's limits: a bucket for the application on each host, and one for each method on each
/// host. Each bucket learns its windows from the responses; clones share the buckets.
pub fn quota() -> Limit {
    let application = Bucket::new(|call| Some(call.request().url().host_str()?.to_owned()))
        .learn(counts("x-app-rate-limit", "x-app-rate-limit-count"))
        .hit_when(|seen| seen.header("x-rate-limit-type") == Some("application"));
    let method = Bucket::new(|call| {
        Some(format!("{} {}", call.request().url().host_str()?, call.info().endpoint()))
    })
    .learn(counts("x-method-rate-limit", "x-method-rate-limit-count"))
    .hit_when(|seen| seen.header("x-rate-limit-type") != Some("application"));
    Limit::new()
        // Wait up to the longest window, 120 seconds: by default, a full window fails with `MaxHold`.
        .max_hold(Duration::from_secs(120))
        // A `429` of a service limit can come without `Retry-After`: hold its bucket for a second.
        .without_retry_after(Duration::from_secs(1))
        .bucket(application)
        .bucket(method)
}
