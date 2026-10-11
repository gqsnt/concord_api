//! The complete downstream cursor-envelope controller of the v3
//! specification (§17.2).
//!
//! Required feature: `json`. Mode: construction and deterministic local
//! execution through the canonical Catalog `feed` endpoint
//! ([`crate::contracts::catalog_v3`]). Documentation owners:
//! `docs/pagination.md` and `docs/extensions.md`.
//! Proving tests: `tests/catalog_v3.rs` and `tests/application.rs`.
//!
//! The controller uses only public façade contracts. Its public
//! initialization type ([`CursorStart`]) is what the declaration's `init`
//! clause constructs; its state ([`EnvelopePages`]) is private, so the field
//! can be renamed without a DSL edit. `Json` decodes the whole
//! [`Envelope`]; the envelope does not implement `IntoIterator` — the
//! controller owns item extraction.

// ANCHOR: envelope_controller
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind,
};
use serde::Deserialize;

/// One provider page: a batch of items and an optional continuation cursor.
#[derive(Deserialize)]
pub struct Envelope<T> {
    /// The page's items.
    pub data: Vec<T>,
    /// The next cursor; absent means the provider proved exhaustion.
    pub next: Option<String>,
}

/// Public initialization data built by `init CursorStart { after: after }`.
pub struct CursorStart {
    /// Cursor of the first request; `None` requests the first page.
    pub after: Option<String>,
}

/// Private controller state. `Cursor = Option<String>` maps onto the whole
/// endpoint `after?: String` slot.
pub struct EnvelopePages {
    after: Option<String>,
}

impl<T: Send + 'static> PaginationController<Envelope<T>> for EnvelopePages {
    type Item = T;
    type Init = CursorStart;
    type Cursor = Option<String>;
    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self { after: init.after })
    }

    fn cursor(&self) -> Self::Cursor {
        self.after.clone()
    }

    fn advance(
        self,
        response: PageResponse<Envelope<T>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, T>, PaginationError> {
        let Envelope { data, next } = response.into_page();
        if data.len() > limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        match next {
            None => Ok(PageStep::Done { items: data }),
            Some(cursor) => {
                // This fixture's domain contract rejects an empty or an
                // immediately repeated provider cursor; core independently
                // catches longer cycles and bounds.
                if cursor.is_empty() || self.after.as_ref() == Some(&cursor) {
                    return Err(PaginationError::new(
                        PaginationErrorKind::InvalidContinuation,
                    ));
                }
                Ok(PageStep::Continue {
                    next: Self {
                        after: Some(cursor),
                    },
                    items: data,
                })
            }
        }
    }
}
// ANCHOR_END: envelope_controller
