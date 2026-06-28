impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn build_request_from_plan(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        args: &crate::endpoint::RequestArgs,
        meta: RequestMeta,
    ) -> Result<BuiltRequest, ApiClientError> {
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let mut policy = plan.endpoint.policy.clone();
        if let Some(timeout) = plan.overrides.timeout {
            policy.timeout = Some(timeout);
        }
        let mut rate_limit = policy.rate_limit.clone();
        rate_limit.canonicalize();

        let base = format!("{}://{}", plan.endpoint.route.scheme, plan.endpoint.route.host);
        let mut url = url::Url::parse(&base).map_err(|e| ApiClientError::BuildUrl {
            ctx: ctx.clone(),
            source: e,
        })?;
        url.set_path(&plan.endpoint.route.path);
        if !policy.query.is_empty() {
            let mut qp = url.query_pairs_mut();
            for (k, v) in policy.query.iter() {
                qp.append_pair(k, v);
            }
        }

        let mut headers = policy.headers.clone();
        if !headers.contains_key(CONTENT_TYPE)
            && let BodyPlan::Encoded { content_type, .. } = &plan.endpoint.body
            && let Some(content_type) = content_type
        {
            headers.insert(CONTENT_TYPE, content_type.clone());
        }

        let body = match &args.body {
            crate::transport::TransportRequestBody::Empty => {
                crate::transport::TransportRequestBody::Empty
            }
            crate::transport::TransportRequestBody::Bytes(bytes) => {
                crate::transport::TransportRequestBody::from_bytes(bytes.clone())
            }
            crate::transport::TransportRequestBody::Stream(_) => {
                return Err(ApiClientError::PolicyViolation {
                    ctx,
                    msg: "stream request bodies are not supported yet",
                });
            }
        };

        Ok(BuiltRequest {
            meta,
            url,
            headers,
            body,
            timeout: policy.timeout,
            retry: policy.retry,
            rate_limit,
            extensions: Default::default(),
        })
    }
}
