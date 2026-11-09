# NASA NeoWs SDK (bounded slice)

The client covers the NeoWs date-range feed, one browse page, and lookup by ID. It projects the object ID, name, and optional hazardous flag, the feed's date groups and count, and browse page counters. Other fields and links are ignored. It does not cover other NASA APIs or automatically traverse pages, retry, schedule requests, or validate dates and ranges. Use dates in `YYYY-MM-DD` and choose a range accepted by the service. Browse `page` and `size` are optional; their availability is supported by the independent source in PROVENANCE, not described in NASA's basic browse example. A page is a bounded unit of work; use its metadata and choose another page explicitly.

```rust,no_run
use concord_sdk_nasa_neows::{NeoWs, NeoWsConfig};
use concord::reqwest;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(15))
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
let api = NeoWs::new(http, NeoWsConfig { api_key: "DEMO_KEY".into() });
let neo = api.neows();
let feed = neo.feed("2015-09-07", "2015-09-08").send().await?;
if let Some(object) = feed.near_earth_objects.values().flatten().next() {
    let detail = neo.lookup(&object.id).send().await?;
    println!("{}: {}", detail.id, detail.name);
}
let page = neo.browse().set_page(0).set_size(20).send().await?;
if let Some(object) = page.near_earth_objects.first() {
    let detail = neo.lookup(&object.id).send().await?;
    println!("{}: {}", detail.id, detail.name);
}
# Ok(())
# }
```

Each generated `send()` names `Result<T, Error>` in its signature and executes once. Successful JSON must have an accepted JSON content type and fit within 4 MiB after decompression. Errors expose Concord's `ErrorKind` and any HTTP status, including key or quota failures as `Status`; they do not infer a cause from a particular status. Non-success bodies are not read. Build, transport, metadata, decode and limit errors are distinct. The error's `Debug` and `Display` contain no URL, key, response body, header text, or source; `Debug` shows the two parsed quota numbers. No retry or rate-limit policy is installed; callers decide how to handle limits. NASA describes DEMO_KEY as more constrained than a registered key; see its documentation for current limits.

The API key is in the request URL. Avoid logging native requests, URLs, server logs or provider responses. `prepare()` and native reqwest access bypass this SDK's send policy and its body bound and safe error wrapper; `RateLimit::from_headers` reads the quota of a response obtained that way. Supply a transport with redirects disabled (as above), timeouts and any other desired policy; callers own transport configuration. `with_base_url` is available for test servers. Fixtures are synthetic and all tests are local; live provider calls: **NOT_RUN**.

## Reported quota

api.nasa.gov returns `X-RateLimit-Limit` and `X-RateLimit-Remaining` on every response ("Web Service Rate Limits" and "How Do I See My Current Usage?", <https://api.nasa.gov/>). Every result of this SDK carries them as a `RateLimit`: a failure in `Error::rate_limit` (from `send()` and `send_metered()` alike, a key or quota refusal and failures after the head included), a success through `send_metered()`, which every endpoint has once `SendMetered` is imported (`send().await?` still returns the bare value). Each header is `Reported::Value(n)`, `Reported::Absent` (no such header, or no response arrived: `status` is `None`) or `Reported::Unparsable` (present but not a single header field of ASCII decimal digits that fits in a `u64`). No header text is kept.

```rust,no_run
use concord_sdk_nasa_neows::{NeoWs, Reported, SendMetered};

# async fn example(api: NeoWs) {
match api.neows().lookup("3542519").send_metered().await {
    Ok(detail) => println!("{}; remaining {:?}", detail.value.name, detail.rate_limit.remaining),
    Err(error) if error.rate_limit.remaining == Reported::Value(0) => println!("{error}: no quota left"),
    Err(error) => println!("{error}; remaining {:?}", error.rate_limit.remaining.value()),
}
# }
```

These are the numbers of one response. NASA documents a default of 1,000 requests per hour that may vary by service, counters that reset on a rolling hourly basis, and only these two headers, so the SDK states no default, reset time or pacing. `DEMO_KEY` is limited per IP address (30 per hour and 50 per day, as documented on 2026-11-07), not like a registered key; this SDK does not claim what the headers report for it.
