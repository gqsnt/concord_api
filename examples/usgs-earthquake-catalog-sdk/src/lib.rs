#![doc = include_str!("../README.md")]

mod csv;
mod error;
mod export;
mod input;
mod models;
mod policy;

pub use concord;
pub use concord_codecs;
pub use concord_pagination;
pub use concord_retry;
pub use csv::{CSV_BODY_LIMIT, CSV_RECORD_LIMIT, CatalogCsv, CatalogCsvProfile, CsvRows};
pub use error::{Error, HttpFailure, HttpKind};
pub use export::CsvExport;
pub use input::{EventId, Magnitude, OrderBy, PageSize, Timestamp, Window};
pub use models::{CsvRow, Feature, FeatureCollection, Metadata, Point, Properties};
pub use policy::{ERROR_BODY_LIMIT, Policy, Response, SUCCESS_BODY_LIMIT};

use concord_codecs::Json;
use concord_pagination::{Page, Pages, Resume};
use std::num::{NonZeroU64, NonZeroUsize};

/// A bounded GeoJSON traversal. Offsets are one-based and are not snapshots.
pub type CatalogPages = Pages<Feature, NonZeroU64, (), Error>;

concord::api! {
    /// Public earthquake catalog client. Always use a local base for this fixture-only capsule.
    pub client Usgs {
        base "https://earthquake.usgs.gov";
        path "fdsnws/event/1";
        query { "nodata" = "404"; }
        response Json;
        send Policy;

        /// Catalog operations; lookup does not inherit window filters.
        scope catalog {
            /// A bounded filter shared by both output formats.
            /// Prefer `catalog.query(&Window)`; `send()` validates bounds before I/O.
            scope window(
                query starttime: Timestamp,
                query endtime: Timestamp,
                query minmagnitude?: Magnitude,
                query maxmagnitude?: Magnitude,
                query limit: PageSize = PageSize::DEFAULT,
                query orderby: OrderBy = OrderBy::default(),
            ) {
                query { "eventtype" = "earthquake"; }

                /// A single GeoJSON page; generated `send()` applies the SDK policy.
                GET geojson(query offset: NonZeroU64 = NonZeroU64::MIN) -> FeatureCollection {
                    path "query";
                    query { "format" = "geojson"; }
                }
                /// The same page/filter as incremental CSV. Check each row result and completion.
                GET csv(query offset: NonZeroU64 = NonZeroU64::MIN) -> stream CsvRow {
                    path "query";
                    query { "format" = "csv"; }
                    response CatalogCsv;
                }
            }

            /// Retrieve one selected event, without time or magnitude filters.
            GET event(#[sensitive] query eventid: EventId) -> Feature {
                path "query";
                query { "format" = "geojson"; }
            }
        }
    }
}

impl UsgsCatalog {
    /// Export a window across at most `max_pages` CSV responses, yielding rows immediately.
    /// A limit or failure is an error; only exhaustion returns `Ok(None)`.
    pub fn export(&self, window: &Window, from: NonZeroU64, max_pages: NonZeroUsize) -> CsvExport {
        CsvExport::new(self.query(window), window.page_size(), from, max_pages)
    }

    /// Map one window into the shared generated scope, without I/O.
    /// Generated `send()` validates magnitude bounds, including subsequent setter changes.
    pub fn query(&self, window: &Window) -> UsgsCatalogWindow {
        self.window(window.start(), window.end())
            .maybe_minmagnitude(window.magnitude())
            .maybe_maxmagnitude(window.maximum_magnitude())
            .set_limit(window.page_size())
            .set_orderby(window.order_by())
    }

    /// Walk GeoJSON pages sequentially, with an explicit page budget and start offset.
    ///
    /// A full page needs another request to determine exhaustion. A short page or no-data
    /// status ends the traversal. Catalog updates may repeat or omit events across pages;
    /// neither this adapter nor its continuation promises a stable snapshot.
    pub fn pages(
        &self,
        window: &Window,
        from: NonZeroU64,
        max_pages: NonZeroUsize,
    ) -> CatalogPages {
        let query = self.query(window);
        let size = usize::from(window.page_size().get());
        Pages::new(Resume::At(from), move |offset: Option<NonZeroU64>| {
            let query = query.clone();
            async move {
                let offset = offset.unwrap_or(NonZeroU64::MIN);
                let result = query.geojson().set_offset(offset).send().await;
                let response = match result {
                    Err(e) if e.is_no_data() => return Ok(Page::new(Vec::new(), None)),
                    other => other?,
                };
                let features = response.data.features;
                if features.len() > size || response.data.metadata.count != features.len() {
                    return Err(Error::Protocol("inconsistent GeoJSON page count"));
                }
                let next = if features.len() == size {
                    Some(
                        offset
                            .checked_add(size as u64)
                            .ok_or(Error::Protocol("catalog offset overflow"))?,
                    )
                } else {
                    None
                };
                Ok(Page::new(features, next))
            }
        })
        .max_pages(max_pages)
    }
}
