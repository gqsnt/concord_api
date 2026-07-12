// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub(super) fn build_request_from_plan(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        body: &mut crate::io::PreparedBody,
        meta: RequestMeta,
    ) -> Result<BuiltRequest, ApiClientError> {
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let timeout = plan.overrides.timeout.or(plan.endpoint.policy.timeout);
        let retry = plan.endpoint.policy.retry.clone();
        let rate_limit = plan.endpoint.policy.rate_limit.clone();

        let base = format!(
            "{}://{}",
            plan.endpoint.route.scheme, plan.endpoint.route.host
        );
        let mut url = url::Url::parse(&base).map_err(|e| ApiClientError::BuildUrl {
            ctx: ctx.clone(),
            source: e,
        })?;
        url.set_path(&plan.endpoint.route.path);
        if !plan.endpoint.policy.query.is_empty() {
            let mut qp = url.query_pairs_mut();
            for (k, v) in &plan.endpoint.policy.query {
                qp.append_pair(k, v);
            }
        }

        let mut headers = plan.endpoint.policy.headers.clone();
        crate::io::apply_prepared_body_media_type(&mut headers, body).map_err(|()| {
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "request Content-Type conflicts with prepared body media type",
            }
        })?;
        let body = body.produce_for_attempt().map_err(|error| {
            let msg = match error.kind() {
                crate::io::BodyProductionErrorKind::AlreadyConsumed => {
                    "one-shot request body has already been consumed"
                }
                crate::io::BodyProductionErrorKind::FactoryFailure => "request body factory failed",
            };
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg,
            }
        })?;

        Ok(BuiltRequest {
            meta,
            url,
            headers,
            body,
            timeout,
            retry,
            rate_limit,
            extensions: Default::default(),
        })
    }
}
