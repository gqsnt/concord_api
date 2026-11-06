//! Sending a prepared operation while keeping selected response metadata: one
//! `Prepared::receive` (a single execute, any status, body unread), the head read into a value the
//! SDK owns, then the declared handling (`Received::decode`), with status errors turned into
//! Salesforce error documents.

use concord::reqwest::header::HeaderMap;
use concord::{Operation, Prepared};

use crate::error::SfError;

/// The name of the declared endpoint that produced a body (`SalesforceApi.bulk_query.job.results`),
/// for the SDK's own protocol errors about it. Generated endpoints always attach their identity;
/// the `Option` exists only for native bodies, which this SDK does not create.
pub(crate) fn operation_name(operation: Option<Operation>) -> &'static str {
    operation.map_or("<undeclared operation>", |op| op.name())
}

/// `Sforce-Limit-Info: api-usage=USED/LIMIT[; api-bursts=..]`, returned by every REST request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiUsage {
    /// API calls used today.
    pub used: u64,
    /// The org's daily limit.
    pub limit: u64,
}

impl ApiUsage {
    /// Parses the `api-usage` part of `Sforce-Limit-Info`; `None` when absent or malformed.
    pub fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let value = headers.get("sforce-limit-info")?.to_str().ok()?;
        value.split(';').find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            if key != "api-usage" {
                return None;
            }
            let (used, limit) = value.split_once('/')?;
            Some(Self {
                used: used.trim().parse().ok()?,
                limit: limit.trim().parse().ok()?,
            })
        })
    }
}

/// Sends `prepared` once and decodes it with its declared format, returning the value and the
/// response headers' [`ApiUsage`] (read from the head before the body, whatever the status). A
/// non-2xx status becomes [`SfError::Api`]; a transport failure (no response) stays
/// [`SfError::Concord`].
pub(crate) async fn send_with_usage<T>(
    prepared: Prepared<T>,
) -> Result<(T, Option<ApiUsage>), SfError> {
    // `receive` fails only when no response arrived (a transport error naming the endpoint).
    let received = prepared.receive().await.map_err(SfError::from)?;
    let usage = ApiUsage::from_headers(received.head().headers());
    match received.decode().await {
        Ok(value) => Ok((value, usage)),
        Err(error) => Err(SfError::from_concord(error).await),
    }
}

/// Sends `prepared` once (`send()`), turning a non-2xx status into [`SfError::Api`].
pub(crate) async fn send<T>(prepared: Prepared<T>) -> Result<T, SfError> {
    match prepared.send().await {
        Ok(value) => Ok(value),
        Err(error) => Err(SfError::from_concord(error).await),
    }
}

/// Builds and sends a generated operation: `prepare()` errors are kept as Concord build errors.
pub(crate) fn prepared<T>(
    result: Result<Prepared<T>, concord::Error>,
) -> Result<Prepared<T>, SfError> {
    result.map_err(SfError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord::reqwest::header::HeaderValue;

    #[test]
    fn parses_limit_info() {
        let mut h = HeaderMap::new();
        h.insert(
            "sforce-limit-info",
            HeaderValue::from_static("api-usage=10018/100000; api-bursts=1/750"),
        );
        assert_eq!(
            ApiUsage::from_headers(&h),
            Some(ApiUsage {
                used: 10018,
                limit: 100000
            })
        );
        h.insert(
            "sforce-limit-info",
            HeaderValue::from_static("api-usage=x/1"),
        );
        assert_eq!(ApiUsage::from_headers(&h), None);
    }
}
