use std::collections::VecDeque;

use super::identity::{CanonicalIdentity, EncodingLimits, HeaderField, IdentityInput};
use super::limits::{CycleDetector, PaginationLimits};
use super::{Boundary, PaginationError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AuthenticationMetadata {
    marker: u64,
}

impl AuthenticationMetadata {
    pub(super) fn inert(marker: u64) -> Self {
        Self { marker }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RequestModel {
    pub(super) endpoint_discriminator: Vec<u8>,
    pub(super) method_token: Vec<u8>,
    pub(super) canonical_logical_url: Vec<u8>,
    pub(super) public_headers: Vec<HeaderField>,
    pub(super) authentication: AuthenticationMetadata,
}

impl RequestModel {
    pub(super) fn identity_input(&self) -> IdentityInput<'_> {
        IdentityInput {
            endpoint_discriminator: &self.endpoint_discriminator,
            method_token: &self.method_token,
            canonical_logical_url: &self.canonical_logical_url,
            public_headers: &self.public_headers,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Page {
    pub(super) items: Vec<u8>,
}

pub(super) trait Controller {
    fn next(&mut self, page: &Page) -> Result<Option<RequestModel>, PaginationError>;
}

pub(super) enum ControllerModel<C> {
    Custom(C),
    BuiltIn {
        inner: C,
        requested_page_size: usize,
    },
}

impl<C: Controller> ControllerModel<C> {
    fn stops_on_short_page(&self, page: &Page) -> bool {
        match self {
            Self::Custom(_) => false,
            Self::BuiltIn {
                requested_page_size,
                ..
            } => page.items.len() < *requested_page_size,
        }
    }

    fn next(&mut self, page: &Page) -> Result<Option<RequestModel>, PaginationError> {
        match self {
            Self::Custom(controller)
            | Self::BuiltIn {
                inner: controller, ..
            } => controller.next(page),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct SideEffectCounters {
    pub(super) hook: usize,
    pub(super) credential: usize,
    pub(super) limiter: usize,
    pub(super) transport: usize,
    pub(super) controller: usize,
}

impl SideEffectCounters {
    fn increment(&mut self, boundary: Boundary) -> Result<(), PaginationError> {
        let counter = match boundary {
            Boundary::Hook => &mut self.hook,
            Boundary::Credential => &mut self.credential,
            Boundary::Limiter => &mut self.limiter,
            Boundary::Transport => &mut self.transport,
            Boundary::Controller => &mut self.controller,
        };
        *counter = counter
            .checked_add(1)
            .ok_or(PaginationError::BoundaryCountOverflow(boundary))?;
        Ok(())
    }
}

pub(super) struct PaginationEngine {
    limits: PaginationLimits,
    encoding_limits: EncodingLimits,
    scripted_pages: VecDeque<Page>,
    seen: CycleDetector,
    aggregate: Vec<u8>,
    item_count: usize,
    page_count: usize,
    pub(super) counters: SideEffectCounters,
}

impl PaginationEngine {
    pub(super) fn new(limits: PaginationLimits, scripted_pages: Vec<Page>) -> Self {
        Self {
            limits,
            encoding_limits: EncodingLimits::default(),
            scripted_pages: scripted_pages.into(),
            seen: CycleDetector::default(),
            aggregate: Vec::new(),
            item_count: 0,
            page_count: 0,
            counters: SideEffectCounters::default(),
        }
    }

    pub(super) fn run<C: Controller>(
        &mut self,
        mut request: RequestModel,
        mut controller: ControllerModel<C>,
    ) -> Result<Vec<u8>, PaginationError> {
        loop {
            let identity = CanonicalIdentity::encode_with_limits(
                request.identity_input(),
                self.encoding_limits,
            )?;
            let (digest, retained_bytes) = self
                .seen
                .preflight(&identity, self.limits.max_retained_identity_bytes)?;

            let accepted_page_count = self
                .page_count
                .checked_add(1)
                .ok_or(PaginationError::PageCountOverflow)?;
            if accepted_page_count > self.limits.max_pages {
                return Err(PaginationError::PageLimit);
            }

            self.seen.retain(identity, digest, retained_bytes)?;
            self.counters.increment(Boundary::Hook)?;
            self.counters.increment(Boundary::Credential)?;
            self.counters.increment(Boundary::Limiter)?;
            self.counters.increment(Boundary::Transport)?;

            let page = self
                .scripted_pages
                .pop_front()
                .ok_or(PaginationError::TransportScriptExhausted)?;
            let accepted_item_count = self
                .item_count
                .checked_add(page.items.len())
                .ok_or(PaginationError::ItemCountOverflow)?;
            if accepted_item_count > self.limits.max_items {
                return Err(PaginationError::ItemLimit);
            }

            self.aggregate.extend_from_slice(&page.items);
            self.item_count = accepted_item_count;
            self.page_count = accepted_page_count;

            if page.items.is_empty() || controller.stops_on_short_page(&page) {
                return Ok(std::mem::take(&mut self.aggregate));
            }

            if self.page_count >= self.limits.max_pages {
                return Err(PaginationError::PageLimit);
            }

            self.counters.increment(Boundary::Controller)?;
            match controller.next(&page)? {
                Some(next_request) => request = next_request,
                None => return Ok(std::mem::take(&mut self.aggregate)),
            }
        }
    }

    pub(super) fn retained_identity_count(&self) -> usize {
        self.seen.retained_identity_count()
    }

    pub(super) fn retained_identity_bytes(&self) -> usize {
        self.seen.retained_identity_bytes()
    }

    pub(super) fn aggregate(&self) -> &[u8] {
        &self.aggregate
    }

    pub(super) fn item_count(&self) -> usize {
        self.item_count
    }

    pub(super) fn page_count(&self) -> usize {
        self.page_count
    }

    #[cfg(test)]
    pub(super) fn set_encoding_limits(&mut self, limits: EncodingLimits) {
        self.encoding_limits = limits;
    }

    #[cfg(test)]
    pub(super) fn set_page_count(&mut self, count: usize) {
        self.page_count = count;
    }

    #[cfg(test)]
    pub(super) fn set_item_count(&mut self, count: usize) {
        self.item_count = count;
    }

    #[cfg(test)]
    pub(super) fn set_retained_identity_bytes(&mut self, bytes: usize) {
        self.seen.set_retained_identity_bytes(bytes);
    }

    #[cfg(test)]
    pub(super) fn preload_identity(
        &mut self,
        request: &RequestModel,
    ) -> Result<(), PaginationError> {
        let identity = CanonicalIdentity::encode(request.identity_input())?;
        self.seen
            .check_and_retain(identity, self.limits.max_retained_identity_bytes)
    }
}
