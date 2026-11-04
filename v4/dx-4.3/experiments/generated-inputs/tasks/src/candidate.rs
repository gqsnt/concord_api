//! The tasks with the experiment's generated input records
//! (`CatalogTenantListingListInput`, `list_with`, `input`, `input_mut`, `into_input`).

use concord::Error;
use concord_example_sdk::{
    CatalogTenantListing, CatalogTenantListingList, CatalogTenantListingListInput,
};

/// T1: item counts of every call, in order.
pub async fn t1_saved_search(
    acme: &CatalogTenantListing,
    studio: &CatalogTenantListing,
) -> Result<Vec<usize>, Error> {
    let mut saved = CatalogTenantListingListInput::new();
    saved.q = Some("e".into());
    saved.limit = 3;
    let mut everything = saved.clone();
    everything.tags = Some(Vec::new());
    let mut furniture = saved.clone();
    furniture.tags = Some(vec!["furniture".into()]);
    let pending_everything = acme.list_with(everything.clone());

    let mut counts = Vec::new();
    for request in [
        acme.list_with(saved.clone()),
        studio.list_with(saved.clone()),
        acme.list_with(everything),
        studio.list_with(furniture),
    ] {
        counts.push(request.send().await?.items.len());
    }
    saved.limit = 2; // the user edits the saved search
    counts.push(acme.list_with(saved.clone()).send().await?.items.len());
    counts.push(pending_everything.send().await?.items.len()); // built earlier: keeps 3
    counts.push(acme.list().send().await?.items.len()); // the ordinary sibling
    Ok(counts)
}

/// T2: as `baseline::t2_clear_for_one_call`; the call's own values are read from its input,
/// the wire text still from a prepared clone.
pub async fn t2_clear_for_one_call(
    listing: &CatalogTenantListing,
) -> Result<(String, bool, usize, usize), Error> {
    let mut one = listing.list();
    one.input_mut().tags = Some(Vec::new()); // no `tag`, for this call only
    one.input_mut().region = Some(None); // no `x-region`, for this call only
    assert_eq!(one.input().tags.as_deref(), Some(&[][..]));
    // What the context adds (other headers, the base) is outside the input: prepare to see it.
    let prepared = one.clone().prepare()?;
    let query = prepared.request().url().query().unwrap_or("").to_owned();
    let region = prepared.request().headers().contains_key("x-region");
    let cleared = one.send().await?.items.len();
    let ordinary = listing.list().send().await?.items.len();
    Ok((query, region, cleared, ordinary))
}

/// T3: the `limit` of each handed-over request, read from its input, and the same inputs run
/// for `other`.
pub async fn t3_rerun_elsewhere(
    handed_over: Vec<CatalogTenantListingList>,
    other: &CatalogTenantListing,
) -> Result<(Vec<u32>, Vec<usize>), Error> {
    let limits = handed_over.iter().map(|r| r.input().limit).collect();
    let mut counts = Vec::new();
    for request in handed_over {
        counts.push(other.list_with(request.into_input()).send().await?.items.len());
    }
    Ok((limits, counts))
}
