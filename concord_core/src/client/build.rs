// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub(super) struct PublicRequestHead {
    pub(super) meta: RequestMeta,
    pub(super) url: url::Url,
    pub(super) headers: http::HeaderMap,
    pub(super) timeout: Option<std::time::Duration>,
    pub(super) retry: RetrySetting,
    pub(super) rate_limit: RateLimitPlan,
    pub(super) auth_plan: crate::auth::AuthPlacementPlan,
}

impl PublicRequestHead {
    pub(super) fn apply_auth_preflight(
        &mut self,
        auth_plan: &crate::auth::AuthPlacementPlan,
        ctx: &ErrorContext,
    ) -> Result<(), ApiClientError> {
        auth_plan
            .validate_public_request(&self.headers, &self.url)
            .map_err(|source| ApiClientError::Auth {
                ctx: ctx.clone(),
                source,
            })?;
        self.auth_plan = auth_plan.clone();
        Ok(())
    }

    pub(super) fn finish(
        self,
        body: crate::io::ProducedBody,
        ctx: &ErrorContext,
    ) -> Result<BuiltRequest, ApiClientError> {
        let uri = self.url.as_str().parse::<http::Uri>().map_err(|_| {
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "resolved request URI is invalid",
            }
        })?;
        let stream_like = body.is_stream();
        let mut message = http::Request::new(body.into_dyn_body());
        *message.method_mut() = self.meta.method.clone();
        *message.uri_mut() = uri;
        *message.version_mut() = http::Version::HTTP_11;
        *message.headers_mut() = self.headers;
        message
            .extensions_mut()
            .insert(crate::transport::RequestExecutionContext {
                meta: self.meta,
                timeout: self.timeout,
            });
        message.extensions_mut().insert(self.auth_plan);
        Ok(BuiltRequest {
            message,
            retry: self.retry,
            rate_limit: self.rate_limit,
            stream_like,
        })
    }
}

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub(super) fn resolve_public_request_head(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        body: &crate::io::PreparedBody,
        meta: RequestMeta,
    ) -> Result<PublicRequestHead, ApiClientError> {
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
        Ok(PublicRequestHead {
            meta,
            url,
            headers,
            timeout,
            retry,
            rate_limit,
            auth_plan: crate::auth::AuthPlacementPlan::default(),
        })
    }

    pub(super) fn produce_attempt_body(
        &self,
        body: &mut crate::io::PreparedBody,
        ctx: &ErrorContext,
    ) -> Result<crate::io::ProducedBody, ApiClientError> {
        body.produce_for_attempt().map_err(|error| {
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_preflight_leaves_one_shot_body_unconsumed() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("public"),
        );
        let mut head = PublicRequestHead {
            meta: RequestMeta {
                endpoint: "PreflightOneShot",
                method: http::Method::POST,
                idempotent: false,
                attempt: 0,
                page_index: 0,
            },
            url: "https://example.com/items".parse().expect("url"),
            headers,
            timeout: None,
            retry: RetrySetting::Off,
            rate_limit: RateLimitPlan::new(),
            auth_plan: Default::default(),
        };
        let auth_plan = crate::auth::AuthPlacementPlan::from_auth_plan(&crate::auth::AuthPlan {
            requirements: vec![crate::auth::AuthRequirement {
                credential: crate::auth::CredentialRef {
                    id: crate::auth::CredentialId::new("test", "token"),
                },
                placement: crate::auth::AuthPlacement::Bearer,
                usage_id: crate::auth::AuthUsageId::new("preflight-use"),
                step_id: Some("preflight"),
                provenance: crate::auth::AuthProvenance::new("preflight"),
                challenge: Default::default(),
            }],
        })
        .expect("placement plan");
        let ctx = ErrorContext {
            endpoint: "PreflightOneShot",
            method: http::Method::POST,
        };
        let mut body = crate::io::PreparedBody::one_shot(crate::body::DynBody::empty(), None);

        head.apply_auth_preflight(&auth_plan, &ctx)
            .expect_err("collision must fail");
        assert!(body.produce_for_attempt().is_ok());
    }
}
