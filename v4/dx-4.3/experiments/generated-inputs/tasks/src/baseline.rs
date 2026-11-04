//! The tasks with the V4.2 interface: generated requests are built with setters and cannot be
//! read back, so the application keeps its own copy of the values it wants to store or reuse.

use concord::Error;
use concord_example_sdk::{CatalogTenantListing, CatalogTenantListingList};

/// The application's own record of a saved search. It mirrors the endpoint's values the
/// application uses; `None` leaves the SDK's own starting value (the inherited tags, the
/// declared `limit = 20`).
#[derive(Clone, Debug, Default)]
pub struct SavedSearch {
    pub q: Option<String>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<u32>,
}

impl SavedSearch {
    /// The request for this search under `listing`.
    pub fn on(&self, listing: &CatalogTenantListing) -> CatalogTenantListingList {
        let mut request = listing
            .list()
            .maybe_q(self.q.clone())
            .maybe_tags(self.tags.clone());
        if let Some(limit) = self.limit {
            request = request.set_limit(limit);
        }
        request
    }
}

/// T1: item counts of every call, in order.
pub async fn t1_saved_search(
    acme: &CatalogTenantListing,
    studio: &CatalogTenantListing,
) -> Result<Vec<usize>, Error> {
    let mut saved = SavedSearch {
        q: Some("e".into()),
        limit: Some(3),
        ..SavedSearch::default()
    };
    let mut everything = saved.clone();
    everything.tags = Some(Vec::new());
    let mut furniture = saved.clone();
    furniture.tags = Some(vec!["furniture".into()]);
    let pending_everything = everything.on(acme);

    let mut counts = Vec::new();
    for request in [
        saved.on(acme),
        saved.on(studio),
        everything.on(acme),
        furniture.on(studio),
    ] {
        counts.push(request.send().await?.items.len());
    }
    saved.limit = Some(2); // the user edits the saved search
    counts.push(saved.on(acme).send().await?.items.len());
    counts.push(pending_everything.send().await?.items.len()); // built earlier: keeps 3
    counts.push(acme.list().send().await?.items.len()); // the ordinary sibling
    Ok(counts)
}

/// T2: what one call without the inherited tags and region will send, read before sending
/// (the query string and whether `x-region` is set), and the item counts of that call and of the
/// ordinary call after it.
pub async fn t2_clear_for_one_call(
    listing: &CatalogTenantListing,
) -> Result<(String, bool, usize, usize), Error> {
    let one = listing.list().set_tags(Vec::new()).set_region(None);
    // The request cannot be read back: build the native request from a clone to look.
    let prepared = one.clone().prepare()?;
    let query = prepared.request().url().query().unwrap_or("").to_owned();
    let region = prepared.request().headers().contains_key("x-region");
    let cleared = one.send().await?.items.len();
    let ordinary = listing.list().send().await?.items.len();
    Ok((query, region, cleared, ordinary))
}

/// T3: the `limit` of each handed-over request, and the same searches run for `other`.
///
/// The requests cannot be read or moved to another tenant: the limit is read back from the
/// prepared URL's text, and re-running for `other` needs the code that built them (here
/// `crate::lamp_searches`), not the values handed over.
pub async fn t3_rerun_elsewhere(
    handed_over: Vec<CatalogTenantListingList>,
    other: &CatalogTenantListing,
) -> Result<(Vec<u32>, Vec<usize>), Error> {
    let mut limits = Vec::new();
    for request in &handed_over {
        let prepared = request.clone().prepare()?;
        let limit = prepared
            .request()
            .url()
            .query_pairs()
            .find(|(k, _)| k == "limit")
            .and_then(|(_, v)| v.parse().ok())
            .expect("the declaration always sends limit");
        limits.push(limit);
    }
    let mut counts = Vec::new();
    for request in crate::lamp_searches(other) {
        counts.push(request.send().await?.items.len());
    }
    Ok((limits, counts))
}
