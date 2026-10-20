//! Deterministic public request binding for generated durable traversals.

use crate::body::{PageRequestRepresentation, RequestBodyContext, RequestRepresentation};
use crate::client::ClientCore;
use crate::pagination::{CheckpointError, CheckpointErrorKind, PageableEndpoint};
use crate::request::{EndpointSpec, HeadBuilder, PreparedHead, RequestOptions};

/// Encodes the public, cursor-normalized request shape and its captured body.
/// Generated code supplies the ordinary head projection with cursor payloads
/// replaced by a fixed marker. Credentials and selected request controls are
/// outside this binding, allowing a resumed request to select current controls.
#[doc(hidden)]
pub fn checkpoint_input<E>(
    endpoint: &E,
    core: &ClientCore,
    prepare_head: impl FnOnce(HeadBuilder) -> Result<PreparedHead, crate::error::ApiError>,
) -> Result<Vec<u8>, CheckpointError>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
{
    fn field(output: &mut Vec<u8>, bytes: &[u8]) {
        output.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        output.extend_from_slice(bytes);
    }
    let invalid = || CheckpointError::new(CheckpointErrorKind::InputMismatch);
    let meta = endpoint.request_meta();
    let builder = HeadBuilder::new(
        core.base_url(),
        meta.clone(),
        &RequestOptions::default(),
        core.default_debug(),
        core.default_runtime_endpoint_timeout(),
    );
    let head = prepare_head(builder).map_err(|_| invalid())?;
    let mut output = b"CONCORD-CHECKPOINT-INPUT\x01".to_vec();
    field(&mut output, head.method().as_str().as_bytes());
    // This URL already replaces every sensitive path, host and query
    // component with a value-free marker. Never retain the logical URL.
    let mut public_url = url::Url::parse(head.observed_url()).map_err(|_| invalid())?;
    let pairs: Vec<_> = public_url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    if pairs
        .iter()
        .any(|(name, _)| crate::redaction::is_sensitive_name(name))
    {
        public_url.set_query(None);
        let public_pairs: Vec<_> = pairs
            .into_iter()
            .filter(|(name, _)| !crate::redaction::is_sensitive_name(name))
            .collect();
        if !public_pairs.is_empty() {
            public_url.query_pairs_mut().extend_pairs(public_pairs);
        }
    }
    field(&mut output, public_url.as_str().as_bytes());
    let mut headers: Vec<_> = head
        .public_headers()
        .iter()
        .filter(|(name, _)| {
            !head.pagination_sensitive_headers().contains(*name)
                && !crate::redaction::is_sensitive_name(name.as_str())
        })
        .collect();
    headers.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));
    output.extend_from_slice(&(headers.len() as u64).to_be_bytes());
    for (name, value) in headers {
        field(&mut output, name.as_str().as_bytes());
        field(&mut output, value.as_bytes());
    }
    // A page-capable representation promises that clones prepare the same
    // semantic body. Only buffered bodies can be fingerprinted synchronously;
    // no stream or factory is executed just to create or validate a checkpoint.
    let body = E::Body::prepare(
        EndpointSpec::into_body(endpoint.page_clone()),
        RequestBodyContext::new(
            meta.endpoint_id(),
            meta.method(),
            core.request_body_limit(),
            None,
        ),
    )
    .map_err(|_| invalid())?;
    if matches!(body.length_hint(), crate::body::LengthHint::Exact(length) if length > core.request_body_limit().get() as u64)
    {
        return Err(invalid());
    }
    field(&mut output, &body.checkpoint_bytes()?);
    Ok(output)
}
