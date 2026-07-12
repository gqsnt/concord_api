// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub(super) fn retry_outcome_from_error(err: &ApiClientError) -> RetryOutcome<'_> {
        match err {
            ApiClientError::Transport { source, .. } => RetryOutcome::Transport(source),
            ApiClientError::HttpStatus { status, .. } => RetryOutcome::HttpStatus(*status),
            ApiClientError::Decode { .. } => RetryOutcome::Decode,
            _ => RetryOutcome::Other,
        }
    }

    pub(super) fn rate_limit_action_from_error(
        err: &ApiClientError,
    ) -> Option<&RateLimitResponseAction> {
        match err {
            ApiClientError::HttpStatus { rate_limit, .. } => rate_limit.as_deref(),
            _ => None,
        }
    }

    pub(super) fn retry_response_headers_from_error(
        err: &ApiClientError,
    ) -> Option<&http::HeaderMap> {
        match err {
            ApiClientError::HttpStatus { headers, .. } => Some(headers.as_ref()),
            _ => None,
        }
    }

    pub(super) fn retry_delay(
        &self,
        config: &RetrySetting,
        ctx: &RetryContext<'_>,
        _retry_count: u32,
    ) -> Result<Option<std::time::Duration>, ApiClientError> {
        let (decision, respect_retry_after) = if let RetrySetting::Config(config) = config {
            (config.try_decide(ctx)?, config.respect_retry_after)
        } else if matches!(config, RetrySetting::Inherit) {
            (
                self.runtime_state
                    .retry_policy()
                    .should_retry_checked(ctx)?,
                self.runtime_state.respect_retry_after(),
            )
        } else {
            return Ok(None);
        };

        Ok(match decision {
            RetryDecision::Stop => None,
            RetryDecision::Retry if respect_retry_after => {
                let Some(headers) = ctx.response_headers else {
                    return Ok(Some(std::time::Duration::ZERO));
                };
                let Some(delay) = crate::rate_limit::parse_retry_after(headers) else {
                    return Ok(Some(std::time::Duration::ZERO));
                };
                validate_capped_retry_delay(
                    ctx,
                    delay,
                    self.runtime_state.max_rate_limit_cooldown(),
                    "retry Retry-After duration exceeds configured maximum",
                )?;
                Some(delay)
            }
            RetryDecision::Retry => Some(std::time::Duration::ZERO),
        })
    }
}
