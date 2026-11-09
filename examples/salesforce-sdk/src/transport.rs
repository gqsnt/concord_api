//! Sending a prepared operation while keeping selected response metadata: one
//! `Prepared::receive` (a single execute, any status, body unread), then the declared handling
//! (`Received::decode_with_meta`), with status errors turned into Salesforce error documents.

use concord::reqwest::header::HeaderMap;
use concord::{Operation, Prepared};

use crate::error::SfError;
use crate::model::ApiErrorBody;

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
    prepared: Prepared<T, ApiErrorBody>,
) -> Result<(T, Option<ApiUsage>), SfError> {
    // `receive` fails only when no response arrived (a transport error naming the endpoint).
    let received = prepared.receive().await.map_err(SfError::from)?;
    let (value, meta) = received.decode_with_meta().await?;
    Ok((value, ApiUsage::from_headers(meta.headers())))
}

/// Sends `prepared` once (`send()`), turning a non-2xx status into [`SfError::Api`].
pub(crate) async fn send<T>(prepared: Prepared<T, ApiErrorBody>) -> Result<T, SfError> {
    Ok(prepared.send().await?)
}

/// Builds and sends a generated operation: `prepare()` errors are kept as Concord build errors.
pub(crate) fn prepared<T>(
    result: Result<Prepared<T, ApiErrorBody>, concord::Error>,
) -> Result<Prepared<T, ApiErrorBody>, SfError> {
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

    #[tokio::test]
    async fn failed_status_and_decode_keep_api_usage() {
        use crate::OrgContext;
        use crate::ids::{AccessToken, ApiVersion, InstanceUrl};
        use concord::reqwest;
        use concord_example_testserver::{Reply, Script, TestServer};

        let script = Script::new([
            Reply::text(
                400,
                "text/plain",
                r#"[{"errorCode":"INVALID_FIELD","message":"invalid field"}]"#,
            )
            .header("sforce-limit-info", "api-usage=41/1000"),
            Reply::json(200, "{").header("sforce-limit-info", "api-usage=42/1000"),
        ]);
        let server = TestServer::start(move |_| script.next()).await;
        let host = "usage.my.salesforce.test";
        let http = reqwest::Client::builder()
            .no_proxy()
            .resolve(host, server.addr())
            .build()
            .unwrap();
        let context = OrgContext::new(
            http,
            InstanceUrl::parse(&format!("http://{host}:{}", server.addr().port())).unwrap(),
            ApiVersion::PINNED,
            AccessToken::new("usage-test-token").unwrap(),
        )
        .unwrap();

        let status = send_with_usage(context.api().accounts().describe().prepare().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            status.api_usage(),
            Some(ApiUsage {
                used: 41,
                limit: 1000
            })
        );
        let api = status.api().unwrap();
        assert_eq!(api.status().as_u16(), 400);
        assert_eq!(api.api_usage(), status.api_usage());
        assert!(status.has_error_code("INVALID_FIELD"));

        let decode = send_with_usage(context.api().accounts().describe().prepare().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            decode.api_usage(),
            Some(ApiUsage {
                used: 42,
                limit: 1000
            })
        );
        assert!(
            matches!(decode, SfError::Concord(ref error) if error.kind() == concord::ErrorKind::Decode)
        );
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}
