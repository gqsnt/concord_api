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

Each generated `send()` executes once. Successful JSON must have an accepted JSON content type and fit within 4 MiB after decompression. Errors expose Concord's `ErrorKind` and any HTTP status, including key or quota failures as `Status`; they do not infer a cause from a particular status. Non-success bodies are not read. Build, transport, metadata, decode and limit errors are distinct. The error's `Debug` and `Display` contain no URL, key, response body, headers, or source. No retry or rate-limit policy is installed; callers decide how to handle limits. NASA describes DEMO_KEY as more constrained than a registered key; see its documentation for current limits.

The API key is in the request URL. Avoid logging native requests, URLs, server logs or provider responses. `prepare()` and native reqwest access bypass this SDK's send policy and its body bound and safe error wrapper. Supply a transport with redirects disabled (as above), timeouts and any other desired policy; callers own transport configuration. `with_base_url` is available for test servers. Fixtures are synthetic and all tests are local; live provider calls: **NOT_RUN**.
