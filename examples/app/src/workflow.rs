//! A named listing workflow: the application's tenant browser.
//!
//! The application browses one tenant's items with its own featured tags as the default, and
//! sometimes needs the same listing without any tag filter. Writing
//! `listing.list().set_tags(Vec::new())` at each call site hides that intent behind the SDK's
//! "an empty list sends no `tag`" rule, so the application names it once:
//! [`Browser::unfiltered_page`].
//!
//! The names return the generated [`CatalogTenantListingList`] operation, not a response: callers
//! still chain setters (`set_limit`, `maybe_cursor`, `set_region`), `prepare()` and `send()`, and
//! the traversal ([`crate::traversal`]) takes them as templates. Only calls that carry
//! application intent get a name; every other endpoint is used through the SDK directly
//! ([`Browser::listing`]).

use concord_example_sdk::{Catalog, CatalogTenantListing, CatalogTenantListingList};

/// The tag this application features.
pub const FEATURED_TAG: &str = "featured";

/// One tenant's listing, with the application's featured tags as its default.
#[derive(Debug, Clone)]
pub struct Browser {
    listing: CatalogTenantListing,
}

impl Browser {
    /// A browser for `tenant` on `catalog`, defaulting every list request to [`FEATURED_TAG`].
    pub fn new(catalog: &Catalog, tenant: &str) -> Self {
        Self {
            listing: catalog
                .tenant(tenant)
                .listing()
                .set_default_tags(vec![FEATURED_TAG.to_owned()]),
        }
    }

    /// The featured listing: the handle's default tags, unchanged.
    pub fn featured_page(&self) -> CatalogTenantListingList {
        self.listing.list()
    }

    /// The same listing without any tag filter: replaces the tags of **this request only** with
    /// none, so no `tag` is sent. The shared handle keeps its defaults, and sibling requests made
    /// from it still send the featured tags.
    pub fn unfiltered_page(&self) -> CatalogTenantListingList {
        self.listing.list().set_tags(Vec::new())
    }

    /// The underlying SDK handle, for ordinary calls that need no name.
    pub fn listing(&self) -> &CatalogTenantListing {
        &self.listing
    }
}
