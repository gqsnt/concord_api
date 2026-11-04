//! Sending a prepared operation while keeping selected response metadata: Concord's documented
//! native route (`into_parts`, one `execute`, `Decoder::decode`/`finish`), with status errors
//! turned into Salesforce error documents.

use concord::Prepared;
use concord::reqwest::header::HeaderMap;

use crate::error::SfError;

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
/// response headers' [`ApiUsage`]. A non-2xx status becomes [`SfError::Api`].
pub(crate) async fn send_with_usage<T>(
    prepared: Prepared<T>,
) -> Result<(T, Option<ApiUsage>), SfError> {
    let (client, request, decoder) = prepared.into_parts();
    match client.execute(request).await {
        Err(e) => match decoder.finish(Err(e)).await {
            Ok(value) => Ok((value, None)),
            Err(error) => Err(SfError::from_concord(error).await),
        },
        Ok(response) => {
            let usage = ApiUsage::from_headers(response.headers());
            match decoder.decode(response).await {
                Ok(value) => Ok((value, usage)),
                Err(error) => Err(SfError::from_concord(error).await),
            }
        }
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
    result.map_err(SfError::Concord)
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
