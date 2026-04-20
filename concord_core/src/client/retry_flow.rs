impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn decode_built_response<E>(
        resp: BuiltResponse,
        ctx: ErrorContext,
    ) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError>
    where
        E: Endpoint<Cx>,
    {
        // Enforce the documented constraints:
        // - HEAD must map to a NoContent decoder (body is empty by definition).
        if resp.meta.method == http::Method::HEAD && !E::response_is_no_content() {
            return Err(ApiClientError::HeadRequiresNoContent { ctx });
        }

        // - 204/205 are "no content" success statuses. If the endpoint expects content, fail early with a clear error.
        if matches!(
            resp.status,
            StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT
        ) && !E::response_is_no_content()
        {
            return Err(ApiClientError::NoContentStatusRequiresNoContent {
                ctx: ctx.clone(),
                status: resp.status,
            });
        }

        let decoded = <<E::Response as ResponseSpec>::Dec as Decodes<
            <E::Response as ResponseSpec>::Decoded,
        >>::decode(&resp.body)
        .map_err(|e| ApiClientError::Decode {
            ctx: ctx.clone(),
            source: e.into(),
        })?;

        let decoded_resp = DecodedResponse {
            meta: resp.meta,
            url: resp.url,
            status: resp.status,
            headers: resp.headers,
            value: decoded,
        };
        <E::Response as ResponseSpec>::map_response(decoded_resp)
            .map_err(|e| ApiClientError::Transform { ctx, source: e })
    }

    fn retry_outcome_from_error(err: &ApiClientError) -> RetryOutcome<'_> {
        match err {
            ApiClientError::Transport { source, .. } => RetryOutcome::Transport(source),
            ApiClientError::HttpStatus { status, .. } => RetryOutcome::HttpStatus(*status),
            ApiClientError::Decode { .. } => RetryOutcome::Decode,
            ApiClientError::Transform { .. } => RetryOutcome::Transform,
            _ => RetryOutcome::Other,
        }
    }

    fn rate_limit_action_from_error(err: &ApiClientError) -> Option<&RateLimitResponseAction> {
        match err {
            ApiClientError::HttpStatus { rate_limit, .. } => rate_limit.as_deref(),
            _ => None,
        }
    }

    fn retry_response_headers_from_error(err: &ApiClientError) -> Option<&http::HeaderMap> {
        match err {
            ApiClientError::HttpStatus { headers, .. } => Some(headers.as_ref()),
            _ => None,
        }
    }

    fn retry_delay(
        &self,
        config: &RetrySetting,
        ctx: &RetryContext<'_>,
        retry_count: u32,
    ) -> Option<std::time::Duration> {
        let decision = if let RetrySetting::Config(config) = config {
            if retry_count >= config.max_retries() {
                return None;
            }
            config.decide(ctx)
        } else if matches!(config, RetrySetting::Inherit) {
            if retry_count >= self.runtime_state.retry_policy().max_retries() {
                return None;
            }
            self.runtime_state.retry_policy().should_retry(ctx)
        } else {
            return None;
        };

        match decision {
            RetryDecision::Stop => None,
            RetryDecision::Retry => Some(std::time::Duration::ZERO),
            RetryDecision::RetryAfter(delay) => Some(delay),
        }
    }
}

