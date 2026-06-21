impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
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
    ) -> Result<Option<std::time::Duration>, ApiClientError> {
        let decision = if let RetrySetting::Config(config) = config {
            if retry_count >= config.max_retries() {
                return Ok(None);
            }
            config.try_decide(ctx)?
        } else if matches!(config, RetrySetting::Inherit) {
            if retry_count >= self.runtime_state.retry_policy().max_retries() {
                return Ok(None);
            }
            self.runtime_state.retry_policy().should_retry_checked(ctx)?
        } else {
            return Ok(None);
        };

        Ok(match decision {
            RetryDecision::Stop => None,
            RetryDecision::Retry => Some(std::time::Duration::ZERO),
            RetryDecision::RetryAfter(delay) => {
                validate_retry_delay(ctx, delay, "retry policy duration overflowed")?;
                Some(delay)
            }
        })
    }
}
