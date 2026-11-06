#![doc = include_str!("../README.md")]

mod error;
mod models;
mod policy;

/// Concord's public types, including the native transport and error classification.
pub use concord;
pub use error::{Error, HttpError, RecordKind};
pub use models::{
    AuthorHit, AuthorId, AuthorSearch, AuthorWorks, Work, WorkHit, WorkId, WorkSearch, WorkSort,
};
pub use policy::{ERROR_BODY_LIMIT, RESPONSE_BODY_LIMIT, ReadPolicy};

use concord::Json;
use std::num::NonZeroU32;

fn search_limit() -> NonZeroU32 {
    NonZeroU32::new(20).expect("20 is nonzero")
}

concord::api! {
    /// Open Library's read-only search and work slice. No credentials are required.
    pub client OpenLibrary {
        base "https://openlibrary.org";
        response Json;
        send ReadPolicy;

        /// Public search operations; pagination is specific to each endpoint.
        scope search {
            /// Searches works, with a one-based page and explicit page size (SDK default: 20).
            /// Optional sort uses the documented values; omission keeps relevance order.
            GET works(
                #[sensitive] query q: String,
                query page: NonZeroU32 = NonZeroU32::MIN,
                query limit: NonZeroU32 = search_limit(),
                query sort?: WorkSort,
            ) -> WorkSearch { path "search.json"; }

            /// Searches authors by text. This slice declares only the documented `q` input.
            GET authors(#[sensitive] query q: String) -> AuthorSearch {
                path "search/authors.json";
            }
        }

        /// Work records addressed by validated Open Library work IDs.
        scope works {
            path "works";
            /// Retrieves a work, without pagination parameters or a human-readable slug.
            GET get(id: WorkId) -> Work { path "{id}.json"; }
        }

        /// An author's work collection, addressed by a validated author ID.
        scope author(id: AuthorId) {
            path "authors/{id}";
            /// Retrieves one batch; omitted limit/offset use the provider's defaults.
            GET works(query limit?: NonZeroU32, query offset?: u64) -> AuthorWorks {
                path "works.json";
            }
        }
    }
}
