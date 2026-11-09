//! Finite JSON page traversal with explicit completion and resumable partial outcomes.
use std::{num::NonZeroUsize, sync::Arc};

use concord::{Endpoint, ErrorKind, Follow, NextUrl, Prepared, ResponseMeta};
use concord_pagination::{Page, Pages, Resume};

use crate::{
    DeltaLink, GraphClient, GraphError, LinkProblem, NextLink, ProtocolProblem,
    error::ErrorBody,
    links::{Binding, Collection, LinkKind, StoredLink, validate_url},
    model::Envelope,
};

/// Metadata retained after body decoding. Only a valid final users page has a delta checkpoint.
#[derive(Clone, Debug)]
pub struct PageMeta {
    /// Full status, headers and generated operation identity; Debug hides header values.
    pub response: ResponseMeta,
    /// Final checkpoint, absent on intermediate and non-delta pages.
    pub delta_link: Option<DeltaLink>,
}

/// One validated page; an empty page with `next` is not exhaustion.
pub type GraphPage<T> = Page<T, NextLink, PageMeta>;
/// Owned traversal. Use `next_page` for delta so the final metadata is retained.
pub type GraphPages<T> = Pages<T, NextLink, PageMeta, GraphError>;

type Factory<T> = Arc<
    dyn Fn(Option<NextUrl>) -> Result<Prepared<Envelope<T>, crate::ErrorEnvelope>, concord::Error>
        + Send
        + Sync,
>;

/// A reusable request factory bound to one authorization context, collection and selection.
/// Creation performs no HTTP I/O. Every fetch uses the Concord-declared operation once.
#[derive(concord::Debug)]
#[sensitive]
pub struct Listing<T> {
    pub(crate) binding: Binding,
    prepare: Factory<T>,
    valid: fn(&T) -> bool,
    start: Option<DeltaLink>,
    success_limit: usize,
    error_limit: usize,
}

impl<T> Clone for Listing<T> {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            prepare: self.prepare.clone(),
            valid: self.valid,
            start: self.start.clone(),
            success_limit: self.success_limit,
            error_limit: self.error_limit,
        }
    }
}

impl<T: Send + 'static> Listing<T> {
    pub(crate) fn new<E>(
        client: &GraphClient,
        collection: Collection,
        select: String,
        start: Option<DeltaLink>,
        endpoint: E,
        valid: fn(&T) -> bool,
    ) -> Result<Self, GraphError>
    where
        E: Follow<Envelope<T>, crate::ErrorEnvelope> + Clone + Send + Sync + 'static,
    {
        let prepare = move |target: Option<NextUrl>| match target {
            Some(target) => endpoint.clone().follow(target).prepare(),
            None => endpoint.clone().prepare(),
        };
        // The collection path comes from the generated operation, including encoded group ids.
        let request = prepare(None)?;
        let binding = client.binding(
            collection,
            select,
            request.request().url().path().to_owned(),
        );
        Ok(Self {
            binding,
            prepare: Arc::new(prepare),
            valid,
            start,
            success_limit: client.success_limit,
            error_limit: client.error_limit,
        })
    }

    /// Restores a next-page link only for this exact listing (including its selected fields).
    pub fn restore_next(&self, json: &str) -> Result<NextLink, GraphError> {
        let link = StoredLink::parse(json)?;
        link.check(&self.binding, LinkKind::Next)?;
        Ok(NextLink(link))
    }

    /// Fetches the first page (or initial delta checkpoint) with `None`, or one next page.
    /// Empty pages retain their next link; only a delta link completes a users-delta round.
    /// Unsafe links and invalid envelopes fail before emitting the affected page's items.
    pub async fn fetch(&self, next: Option<&NextLink>) -> Result<GraphPage<T>, GraphError> {
        let link = if let Some(next) = next {
            next.0.check(&self.binding, LinkKind::Next)?;
            Some(&next.0)
        } else if let Some(start) = &self.start {
            start.0.check(&self.binding, LinkKind::Delta)?;
            Some(&start.0)
        } else {
            None
        };
        let prepared = (self.prepare)(link.map(|link| {
            NextUrl::from_url(
                concord::reqwest::Url::parse(&link.url).expect("validated stored link"),
            )
        }))?;
        let requested = prepared.request().url().clone();
        let received = prepared.receive().await?;
        if !received.status().is_success() {
            let (meta, document) = received.failure(self.error_limit).await.into_document();
            let body = match document {
                Ok(envelope) => ErrorBody::Parsed(Box::new(envelope.error)),
                Err(error) if error.kind() == ErrorKind::Decode => ErrorBody::Malformed,
                Err(error) => ErrorBody::Unavailable(error),
            };
            return Err(GraphError::Provider {
                meta: Box::new(meta),
                body,
            });
        }
        if received.status().as_u16() != 200 {
            return Err(GraphError::Protocol {
                problem: ProtocolProblem::UnexpectedStatus,
                meta: Box::new(received.meta()),
            });
        }
        let (envelope, meta) = received
            .into_success()?
            .decode_limited_with_meta(self.success_limit)
            .await?;
        let protocol = |problem| GraphError::Protocol {
            problem,
            meta: Box::new(meta.clone()),
        };
        match self.binding.collection {
            Collection::UsersDelta if envelope.next.is_some() == envelope.delta.is_some() => {
                return Err(protocol(ProtocolProblem::DeltaLinkCardinality));
            }
            Collection::Groups | Collection::Members(_) if envelope.delta.is_some() => {
                return Err(protocol(ProtocolProblem::UnexpectedDeltaLink));
            }
            _ => {}
        }
        if envelope.value.iter().any(|item| !(self.valid)(item)) {
            return Err(protocol(ProtocolProblem::InvalidObject));
        }
        let make_link = |url: String, kind: LinkKind| -> Result<StoredLink, GraphError> {
            let target =
                validate_url(&url, &self.binding).map_err(|error| error.with_meta(&meta))?;
            if kind == LinkKind::Next && target == requested {
                return Err(GraphError::link(LinkProblem::DidNotAdvance).with_meta(&meta));
            }
            Ok(StoredLink {
                version: 1,
                kind,
                binding: self.binding.clone(),
                url,
            })
        };
        let next = envelope
            .next
            .map(|url| make_link(url, LinkKind::Next).map(NextLink))
            .transpose()?;
        let delta_link = envelope
            .delta
            .map(|url| make_link(url, LinkKind::Delta).map(DeltaLink))
            .transpose()?;
        Ok(Page {
            items: envelope.value,
            next,
            meta: PageMeta {
                response: meta,
                delta_link,
            },
        })
    }

    /// Starts an owned, cancellation-safe traversal with a default 1000-page cap.
    /// Override with `.max_pages(...)`. A cap or error is never successful exhaustion.
    pub fn pages(&self) -> GraphPages<T> {
        let listing = self.clone();
        Pages::new(Resume::Start, move |key: Option<NextLink>| {
            let listing = listing.clone();
            async move { listing.fetch(key.as_ref()).await }
        })
        .max_pages(NonZeroUsize::new(1000).expect("nonzero default"))
    }
}
