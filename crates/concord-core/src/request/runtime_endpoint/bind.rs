//! Runtime binding: owner-level validation, the bound endpoint specification
//! and the [`BoundRuntimeRequest`] terminal wrapper (RUNTIME-01, BRIDGE-01).

use std::fmt;
use std::future::IntoFuture;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;

use super::{AuthMode, RuntimeEndpoint, RuntimeOp, RuntimeProgram, RuntimeRouteMode, route_shape};
use crate::body::{PreparedBodySpec, PreparedRequestBody};
use crate::controls::{
    CallOptions, CancellationToken, ControlError, ControlSummary, RequestControls,
};
use crate::endpoint::{EndpointDescription, RequestPlan};
use crate::error::{ApiError, ConfigurationErrorKind};
use crate::hooks::BoxFuture;
use crate::request::program::{PatchFamily, PatchKind, PatchOp};
use crate::request::{
    AcceptedBytesRequest, EndpointSpec, HeadBuilder, OwnerProjection, PreparedHead, ProgramSegment,
    Request, RequestMeta, RequestOptions, ResponseRequest, apply_head_program,
    visit_extended_program,
};
use crate::response::{AcceptedBytesDriver, ResponseRepresentation};

/// Symbolic potential ownership of the owner chain plus the runtime program
/// (PATCH-01/02): independent of runtime presence, no encoder is invoked.
#[derive(Default)]
struct Ownership {
    query: Vec<String>,
    header: Vec<String>,
}

impl Ownership {
    fn family(&mut self, family: PatchFamily) -> &mut Vec<String> {
        match family {
            PatchFamily::Query => &mut self.query,
            PatchFamily::Header => &mut self.header,
        }
    }

    fn position(&mut self, family: PatchFamily, key: &str) -> Option<usize> {
        self.family(family).iter().position(|writer| match family {
            PatchFamily::Query => writer == key,
            PatchFamily::Header => writer.eq_ignore_ascii_case(key),
        })
    }

    /// Applies one owner operation. The owner's program was validated when
    /// its declaration compiled, so this only tracks ownership.
    fn inherit(&mut self, op: &PatchOp<'_>) {
        let existing = self.position(op.family, op.key);
        let set = self.family(op.family);
        match op.kind {
            PatchKind::Clear => set.clear(),
            PatchKind::Remove => {
                if let Some(index) = existing {
                    set.remove(index);
                }
            }
            PatchKind::Introduce
            | PatchKind::Replace
            | PatchKind::WhenPresent
            | PatchKind::Append => {
                if existing.is_none() {
                    set.push(op.key.to_owned());
                }
            }
        }
    }

    /// Applies one runtime operation with the static structural rules.
    fn apply_runtime(&mut self, op: &RuntimeOp) -> Result<(), ()> {
        let existing = self.position(op.family, &op.key);
        let set = self.family(op.family);
        match op.kind {
            PatchKind::Clear => set.clear(),
            PatchKind::Introduce => {
                if existing.is_some() {
                    return Err(());
                }
                set.push(op.key.to_string());
            }
            PatchKind::Replace | PatchKind::WhenPresent => {
                if existing.is_none() {
                    return Err(());
                }
            }
            PatchKind::Remove => match existing {
                Some(index) => {
                    set.remove(index);
                }
                None => return Err(()),
            },
            PatchKind::Append => {
                if existing.is_none() {
                    set.push(op.key.to_string());
                }
            }
        }
        Ok(())
    }
}

fn scratch_head(projection: &dyn OwnerProjection, program: &RuntimeProgram) -> HeadBuilder {
    let core = projection.core();
    HeadBuilder::new(
        core.base_url(),
        RequestMeta::from_description(&program.description),
        &RequestOptions::new(),
        core.default_debug(),
        core.default_runtime_endpoint_timeout(),
    )
}

/// Validates one runtime program at an owner without encoding any value and
/// returns the bound description. No body, credential, provider, limiter,
/// observer or transport work happens.
fn validate_binding(
    projection: &dyn OwnerProjection,
    program: &RuntimeProgram,
) -> Result<EndpointDescription, ApiError> {
    let core = projection.core();
    let id = &program.id;
    let method = &program.method;
    let error = |kind| ApiError::configuration(id.clone(), method.clone(), kind);

    // IDENTITY-01: a runtime ID may not collide with a generated endpoint ID
    // of the same client.
    if projection
        .generated_endpoint_ids()
        .iter()
        .any(|generated| *generated == id.as_str())
    {
        return Err(error(ConfigurationErrorKind::InvalidValue));
    }

    // PATCH-02: introduce conflicts with the owner's potential keys and
    // override targets without an earlier potential writer are rejected
    // before presence is known.
    let mut ownership = Ownership::default();
    projection.visit_program(&mut |segments| {
        for segment in segments {
            for op in segment.ops {
                ownership.inherit(op);
            }
        }
        Ok(())
    })?;
    for op in &program.ops {
        ownership.apply_runtime(op).map_err(|()| {
            error(match op.family {
                PatchFamily::Query => ConfigurationErrorKind::InvalidValue,
                PatchFamily::Header => ConfigurationErrorKind::InvalidHeader,
            })
        })?;
    }

    // RUNTIME-02 / AUTH-01: inherited placements, explicit removals by safe
    // target identity; no runtime placement can be created.
    let mut scratch = scratch_head(projection, program);
    let inherited_auth = if program.auth == AuthMode::Inherit {
        projection.push_auth(&mut scratch)?;
        let inherited = scratch.auth_targets().len();
        for target in &program.auth_removals {
            if !scratch.remove_auth_target(target)? {
                return Err(error(ConfigurationErrorKind::InvalidAuthentication));
            }
        }
        inherited
    } else {
        let mut probe = scratch_head(projection, program);
        projection.push_auth(&mut probe)?;
        probe.auth_targets().len()
    };
    let targets = scratch.auth_targets();
    // Placements must belong to the owner's own client build.
    scratch.validate_auth_context(core.credential_context())?;

    // HEAD-01: a runtime public writer may not target an effective private
    // authentication target, even when its optional source is absent.
    for op in &program.ops {
        let claimed = targets.iter().any(|target| match op.family {
            PatchFamily::Query => target.claims_query(&op.key),
            PatchFamily::Header => target.claims_header(&op.key),
        });
        if claimed {
            return Err(error(ConfigurationErrorKind::InvalidAuthentication));
        }
    }

    // §10.1: an explicit accepted set may not overlap an effective challenge.
    if let Some(status) = &program.status {
        for code in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            if status.contains(code) && scratch.auth_classifies_challenge(code) {
                return Err(error(ConfigurationErrorKind::InvalidAuthentication));
            }
        }
    }

    // RATE-01: every inherited policy remains; runtime policies deduplicate
    // exactly or conflict. A client without a limiter selection cannot admit
    // any policy, so binding fails instead of reaching the fail-closed
    // fallback (or bypassing enforcement).
    projection.push_rate_limits(&mut scratch)?;
    scratch.append_rate_limit_declarations(&program.rate_limits)?;
    let effective_rates = scratch.rate_limit_declaration_count();
    if effective_rates > 0 && !core.rate_limiter_selected() {
        return Err(error(ConfigurationErrorKind::InvalidRateLimit));
    }

    // Fixed-origin transport preflight: credentials only over HTTPS.
    if targets.is_empty() {
        core.preflight_url(id, method, core.base_url())?;
    } else {
        core.preflight_protected_url(id, method, core.base_url())?;
    }

    let owner = projection.owner_path();
    let mut facts = program.facts.clone();
    facts.owner = Some(owner);
    facts.inherited_auth_placements = inherited_auth;
    facts.effective_auth_placements = targets.len();
    facts.effective_rate_policies = effective_rates;
    Ok(program.description.bound_runtime(
        Arc::from(format!("{owner}::<runtime>")),
        route_shape(Some(owner), program.route, program.path.len()),
        projection.owner_timeout(),
        !targets.is_empty(),
        effective_rates > 0,
        facts,
    ))
}

/// Composes the complete public head at plan/execution time: owner host and
/// path, runtime path atoms, one symbolic program (owner segments followed by
/// the runtime segment), inherited authentication minus removals, and every
/// inherited plus runtime rate policy.
fn compose_head(
    owner: &dyn OwnerProjection,
    program: &RuntimeProgram,
    mut builder: HeadBuilder,
) -> Result<PreparedHead, ApiError> {
    owner.push_host(&mut builder)?;
    if program.route == RuntimeRouteMode::Inherit {
        owner.push_path(&mut builder)?;
    }
    for segment in &program.path {
        builder.push_path_atoms(segment.sensitive, |writer| segment.atom.write(writer))?;
    }
    let ops: Vec<PatchOp<'_>> = program.ops.iter().map(RuntimeOp::as_patch).collect();
    let local = ProgramSegment {
        ops: &ops,
        emissions: &program.table,
    };
    owner.visit_program(&mut |inherited| {
        visit_extended_program(inherited, local, |segments| {
            apply_head_program(&mut builder, segments)
        })
    })?;
    if program.auth == AuthMode::Inherit {
        owner.push_auth(&mut builder)?;
        for target in &program.auth_removals {
            if !builder.remove_auth_target(target)? {
                return Err(ApiError::configuration(
                    program.id.clone(),
                    program.method.clone(),
                    ConfigurationErrorKind::InvalidAuthentication,
                ));
            }
        }
    }
    owner.push_rate_limits(&mut builder)?;
    builder.append_rate_limit_declarations(&program.rate_limits)?;
    builder.finish()
}

/// The core endpoint specification of one runtime endpoint bound at one
/// generated owner. It always holds its owner projection.
pub(crate) struct BoundRuntimeEndpoint<R, O> {
    owner: Arc<dyn OwnerProjection>,
    program: Arc<RuntimeProgram>,
    description: EndpointDescription,
    body: PreparedRequestBody,
    response: PhantomData<fn() -> (R, O)>,
}

impl<R, O> EndpointSpec for BoundRuntimeEndpoint<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    type Input = PreparedRequestBody;
    type Body = PreparedBodySpec;
    type Output = O;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::from_description(&self.description)
    }

    fn prepare_head(&self, builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        compose_head(&*self.owner, &self.program, builder)
    }

    fn into_body(self) -> Self::Input {
        self.body
    }
}

/// Binds one runtime endpoint at a generated owner projection.
pub(crate) fn bind_runtime_at_owner<R, O>(
    projection: &Arc<dyn OwnerProjection>,
    endpoint: RuntimeEndpoint<R, O>,
) -> Result<BoundRuntimeRequest<R, O>, ApiError>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    let description = validate_binding(&**projection, &endpoint.program)?;
    let RuntimeEndpoint { program, body, .. } = endpoint;
    Ok(BoundRuntimeRequest {
        inner: Request::new(
            projection.core().clone(),
            BoundRuntimeEndpoint {
                owner: Arc::clone(projection),
                program,
                description,
                body,
                response: PhantomData,
            },
        ),
    })
}

/// Builds the inert bound plan of one runtime endpoint at an owner without
/// binding it: the same validation as binding, then real public-head
/// composition. No body is prepared.
pub(crate) fn plan_runtime_at_owner<R, O>(
    projection: &Arc<dyn OwnerProjection>,
    endpoint: &RuntimeEndpoint<R, O>,
) -> Result<RequestPlan, ApiError>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    let description = validate_binding(&**projection, &endpoint.program)?;
    crate::__private::plan_unbound_request(projection.core(), &description, R::ACCEPT, |builder| {
        compose_head(&**projection, &endpoint.program, builder)
    })
}

/// A runtime endpoint bound at one generated owner (client root or scope).
///
/// It implements the same ordinary, response and control interfaces as a
/// generated request: [`IntoFuture`] resolving to `Result<O, ApiError>`,
/// [`response`](Self::response), [`plan`](Self::plan),
/// [`description`](Self::description), [`timeout`](Self::timeout),
/// [`reset_timeout`](Self::reset_timeout), [`cancel_with`](Self::cancel_with),
/// [`with_options`](Self::with_options),
/// [`control_summary`](Self::control_summary) and [`RequestControls`]. It is
/// inert until first polled and enters the same managed executor as every
/// generated request. It is not `Clone` because it owns its body input.
pub struct BoundRuntimeRequest<R, O> {
    inner: Request<BoundRuntimeEndpoint<R, O>>,
}

impl<R, O> BoundRuntimeRequest<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    /// Returns the bound, value-free description: the owner's effective
    /// timeout (unless selected locally), effective authentication and rate
    /// capabilities, and the runtime binding facts (including any removed
    /// inherited authentication, displayed prominently).
    #[must_use]
    pub fn description(&self) -> &EndpointDescription {
        &self.inner.endpoint.description
    }

    /// Builds the inert bound plan through real public-head composition with
    /// this request's controls. Planning evaluates surviving public atom
    /// encoders but never prepares a body, acquires credentials, reserves
    /// quota or sends.
    ///
    /// # Errors
    ///
    /// A preparation-stage [`ApiError`] (never dispatched) when an encoder
    /// fails or a public head limit is exceeded.
    pub fn plan(&self) -> Result<RequestPlan, ApiError> {
        crate::__private::request_plan(&self.inner, &self.inner.endpoint.description)
    }

    /// Selects universal accepted response metadata; every control is
    /// carried into the returned terminal.
    #[must_use]
    pub fn response(self) -> ResponseRequest<O> {
        self.inner.response()
    }

    /// Replaces the selected finite timeout preference; caps are retained.
    ///
    /// # Errors
    ///
    /// [`ControlError::ZeroDuration`] or
    /// [`ControlError::UnrepresentableDuration`].
    pub fn timeout(self, duration: Duration) -> Result<Self, ControlError> {
        Ok(Self {
            inner: self.inner.timeout(duration)?,
        })
    }

    /// Restores the effective declared timeout; caps are retained.
    #[must_use]
    pub fn reset_timeout(self) -> Self {
        Self {
            inner: self.inner.reset_timeout(),
        }
    }

    /// Attaches one more cancellation signal (at most eight distinct).
    ///
    /// # Errors
    ///
    /// [`ControlError::TooManyCancellationSources`] for a ninth signal.
    pub fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Ok(Self {
            inner: self.inner.cancel_with(token)?,
        })
    }

    /// Intersects additional caps componentwise; caps only narrow execution.
    #[must_use]
    pub fn with_options(self, options: &CallOptions) -> Self {
        Self {
            inner: self.inner.with_options(options),
        }
    }

    /// Returns the value-free effective control summary.
    #[must_use]
    pub fn control_summary(&self) -> ControlSummary {
        self.inner.control_summary()
    }

    /// Unwraps the core request for crate-internal executor tests.
    #[cfg(test)]
    pub(crate) fn into_request(self) -> Request<BoundRuntimeEndpoint<R, O>> {
        self.inner
    }
}

impl<R, O> BoundRuntimeRequest<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
    R::Driver: AcceptedBytesDriver,
{
    /// Selects the explicitly exposed accepted-bytes terminal (§10.4).
    ///
    /// Available only for buffered/discard response drivers.
    ///
    /// # Errors
    ///
    /// A preparation-stage [`ConfigurationErrorKind::InvalidValue`] error
    /// when the descriptor did not call `expose_accepted_bytes()`; nothing is
    /// executed.
    pub fn accepted_bytes(self) -> Result<AcceptedBytesRequest, ApiError> {
        if !self.inner.endpoint.program.accepted_bytes {
            return Err(ApiError::configuration(
                self.inner.endpoint.program.id.clone(),
                self.inner.endpoint.program.method.clone(),
                ConfigurationErrorKind::InvalidValue,
            )
            .before_operation());
        }
        Ok(crate::execute::accepted_bytes_request(self.inner))
    }
}

impl<R, O> RequestControls for BoundRuntimeRequest<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    fn with_options(self, options: &CallOptions) -> Self {
        Self::with_options(self, options)
    }

    fn control_summary(&self) -> ControlSummary {
        Self::control_summary(self)
    }

    fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Self::cancel_with(self, token)
    }
}

impl<R, O> IntoFuture for BoundRuntimeRequest<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    type Output = Result<O, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.into_future()
    }
}

impl<R, O> fmt::Debug for BoundRuntimeRequest<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundRuntimeRequest")
            .field("endpoint", &self.description().endpoint_id())
            .field("owner", &self.inner.endpoint.owner.owner_path())
            .field("controls", &self.control_summary())
            .finish_non_exhaustive()
    }
}
