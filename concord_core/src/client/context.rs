pub trait ClientContext: Sized + Send + Sync + 'static {
    type Vars: Clone + Send + Sync + 'static;
    type AuthVars: Clone + Send + Sync + 'static;
    type AuthState: Clone + Send + Sync + 'static;
    const SCHEME: Scheme;
    const DOMAIN: &'static str;

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState;

    fn apply_internal_auth<'a>(
        _requirement: &'a AuthRequirementId,
        _request: &'a mut BuiltRequest,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn AuthHttpExecutor,
    ) -> crate::auth::AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async {
            Err(AuthError::new(
                AuthErrorKind::UnsupportedScheme,
                "internal auth requirement is not supported by this client context",
            ))
        })
    }

    fn prepare_auth_requirement<'a>(
        _requirement: &'a crate::auth::AuthRequirement,
        _request: &'a mut BuiltRequest,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> crate::auth::AuthFuture<'a, Result<crate::auth::AuthAppliedCredential, AuthError>> {
        Box::pin(async {
            Err(AuthError::new(
                AuthErrorKind::UnsupportedScheme,
                "auth requirement is not supported by this client context",
            ))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_auth_response<'a>(
        _requirement: &'a crate::auth::AuthRequirement,
        _applied: &'a crate::auth::AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn AuthHttpExecutor,
        _meta: &'a RequestMeta,
        _status: http::StatusCode,
        _headers: &'a http::HeaderMap,
    ) -> crate::auth::AuthFuture<'a, Result<crate::auth::AuthDecision, AuthError>> {
        Box::pin(async { Ok(crate::auth::AuthDecision::Continue) })
    }

    fn base_route(_vars: &Self::Vars, _auth: &Self::AuthVars) -> RouteBuilder {
        RouteBuilder::new()
    }

    fn base_policy(
        _vars: &Self::Vars,
        _auth: &Self::AuthVars,
        _ctx: &ErrorContext,
    ) -> Result<Policy, ApiClientError> {
        Ok(Policy::new())
    }
}

#[derive(Clone, Copy)]
struct SendClassifyCtx<'a> {
    dbg: DebugLevel,
    dbg_verbose: bool,
    dbg_vv: bool,
    url_str: &'a str,
    error_ctx: &'a ErrorContext,
}

enum CacheBeforeOutcome {
    Hit(BuiltResponse),
    Continue(Option<CacheRevalidation>),
}

struct CacheAfterOutcome {
    response: BuiltResponse,
    needs_revalidation_refetch: bool,
}
