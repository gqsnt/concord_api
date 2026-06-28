impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn build_request_from_plan(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        args: &mut crate::endpoint::RequestArgs,
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
        if !headers.contains_key(CONTENT_TYPE) {
            match &plan.endpoint.body {
                BodyPlan::Encoded { content_type, .. } => {
                    if let Some(content_type) = content_type {
                        headers.insert(CONTENT_TYPE, content_type.clone());
                    }
                }
                BodyPlan::RawStream { content_type } => {
                    headers.insert(CONTENT_TYPE, content_type.clone());
                }
                BodyPlan::None => {}
            }
        }

        let body = match &plan.endpoint.body {
            BodyPlan::None => match &args.body {
                crate::transport::TransportRequestBody::Empty => {
                    crate::transport::TransportRequestBody::Empty
                }
                crate::transport::TransportRequestBody::Bytes(_)
                | crate::transport::TransportRequestBody::Stream(_) => {
                    return Err(ApiClientError::PolicyViolation {
                        ctx,
                        msg: "request body is not allowed for this endpoint",
                    });
                }
            },
            BodyPlan::Encoded { .. } => match &args.body {
                crate::transport::TransportRequestBody::Bytes(bytes) => {
                    crate::transport::TransportRequestBody::from_bytes(bytes.clone())
                }
                crate::transport::TransportRequestBody::Empty
                | crate::transport::TransportRequestBody::Stream(_) => {
                    return Err(ApiClientError::PolicyViolation {
                        ctx,
                        msg: "encoded request body plan requires buffered bytes",
                    });
                }
            },
            BodyPlan::RawStream { .. } => match std::mem::replace(
                &mut args.body,
                crate::transport::TransportRequestBody::Empty,
            ) {
                crate::transport::TransportRequestBody::Stream(stream) => {
                    crate::transport::TransportRequestBody::Stream(stream)
                }
                crate::transport::TransportRequestBody::Empty
                | crate::transport::TransportRequestBody::Bytes(_) => {
                    return Err(ApiClientError::PolicyViolation {
                        ctx,
                        msg: "raw stream body plan requires a stream request body",
                    });
                }
            },
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
