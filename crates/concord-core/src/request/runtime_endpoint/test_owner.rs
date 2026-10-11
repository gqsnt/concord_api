//! Test-only owner projection standing in for a generated client or scope.
//!
//! Core unit tests have no `api!` expansion; this projection implements the
//! same [`OwnerProjection`] contract generated owners implement, so runtime
//! endpoints in core tests are bound exactly like downstream ones.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::{BoundRuntimeRequest, RuntimeEndpoint};
use crate::atom::{AtomError, AtomErrorKind};
use crate::auth::Auth;
use crate::client::ClientCore;
use crate::error::ApiError;
use crate::rate_limit::RateLimitDeclaration;
use crate::request::program::{PatchFamily, PatchKind, PatchOp};
use crate::request::{
    HeadBuilder, HeadEmissions, OccurrenceSink, OwnerProjection, ProgramSegment, ProgramVisitor,
};
use crate::response::ResponseRepresentation;

/// A configurable owner chain collapsed into one projection.
pub(crate) struct TestOwner {
    core: ClientCore,
    display: &'static str,
    timeout: Option<Duration>,
    host: Vec<&'static str>,
    segments: Vec<&'static str>,
    auth: Vec<Auth>,
    rates: Vec<RateLimitDeclaration>,
    ops: Vec<PatchOp<'static>>,
    values: Vec<Option<Vec<String>>>,
    calls: Vec<AtomicUsize>,
    failing: Option<u16>,
    ids: &'static [&'static str],
}

impl TestOwner {
    pub(crate) fn new(core: ClientCore) -> Self {
        Self {
            core,
            display: "TestApi",
            timeout: None,
            host: Vec::new(),
            segments: Vec::new(),
            auth: Vec::new(),
            rates: Vec::new(),
            ops: Vec::new(),
            values: Vec::new(),
            calls: Vec::new(),
            failing: None,
            ids: &[],
        }
    }

    pub(crate) fn display(mut self, display: &'static str) -> Self {
        self.display = display;
        self
    }

    pub(crate) fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn host(mut self, label: &'static str) -> Self {
        self.host.push(label);
        self
    }

    pub(crate) fn segment(mut self, segment: &'static str) -> Self {
        self.segments.push(segment);
        self
    }

    pub(crate) fn auth(mut self, auth: Auth) -> Self {
        self.auth.push(auth);
        self
    }

    pub(crate) fn rate(mut self, declaration: RateLimitDeclaration) -> Self {
        self.rates.push(declaration);
        self
    }

    pub(crate) fn generated_ids(mut self, ids: &'static [&'static str]) -> Self {
        self.ids = ids;
        self
    }

    /// Adds one keyed owner operation with an emission (`None` = absent).
    pub(crate) fn op(
        mut self,
        family: PatchFamily,
        kind: PatchKind,
        key: &'static str,
        value: Option<&[&str]>,
        sensitive: bool,
    ) -> Self {
        let emission = u16::try_from(self.values.len()).expect("bounded test owner");
        self.values
            .push(value.map(|values| values.iter().map(|value| (*value).to_owned()).collect()));
        self.calls.push(AtomicUsize::new(0));
        self.ops
            .push(PatchOp::keyed(family, kind, key, emission, sensitive));
        self
    }

    /// Adds a plain query introduction of one present scalar.
    pub(crate) fn query(self, key: &'static str, value: &str) -> Self {
        self.op(
            PatchFamily::Query,
            PatchKind::Introduce,
            key,
            Some(&[value]),
            false,
        )
    }

    /// Adds a plain header introduction of one present scalar.
    pub(crate) fn header(self, key: &'static str, value: &str) -> Self {
        self.op(
            PatchFamily::Header,
            PatchKind::Introduce,
            key,
            Some(&[value]),
            false,
        )
    }

    /// Makes the given owner emission's encoder fail if it is ever invoked.
    pub(crate) fn failing(mut self, emission: u16) -> Self {
        self.failing = Some(emission);
        self
    }

    /// Number of encoder invocations of one owner emission.
    pub(crate) fn encoded(&self, emission: u16) -> usize {
        self.calls[usize::from(emission)].load(Ordering::SeqCst)
    }

    pub(crate) fn into_projection(self) -> Arc<Self> {
        Arc::new(self)
    }
}

impl HeadEmissions for TestOwner {
    fn emission_present(&self, emission: u16) -> bool {
        self.values
            .get(usize::from(emission))
            .is_some_and(Option::is_some)
    }

    fn emit(&self, emission: u16, sink: &mut OccurrenceSink<'_>) -> Result<(), AtomError> {
        let index = usize::from(emission);
        self.calls[index].fetch_add(1, Ordering::SeqCst);
        if self.failing == Some(emission) {
            return Err(AtomError::new(AtomErrorKind::Encode));
        }
        if let Some(Some(values)) = self.values.get(index) {
            for value in values {
                sink.value(value.as_str())?;
            }
        }
        Ok(())
    }
}

impl OwnerProjection for TestOwner {
    fn core(&self) -> &ClientCore {
        &self.core
    }

    fn owner_path(&self) -> &'static str {
        self.display
    }

    fn owner_timeout(&self) -> Option<Duration> {
        self.timeout
    }

    fn push_host(&self, builder: &mut HeadBuilder) -> Result<(), ApiError> {
        for label in &self.host {
            builder.push_host_static(label)?;
        }
        Ok(())
    }

    fn push_path(&self, builder: &mut HeadBuilder) -> Result<(), ApiError> {
        for segment in &self.segments {
            builder.push_static(segment)?;
        }
        Ok(())
    }

    fn push_auth(&self, builder: &mut HeadBuilder) -> Result<(), ApiError> {
        for auth in &self.auth {
            builder.push_auth(auth.clone())?;
        }
        Ok(())
    }

    fn push_rate_limits(&self, builder: &mut HeadBuilder) -> Result<(), ApiError> {
        for declaration in &self.rates {
            builder.push_rate_limit(declaration.clone())?;
        }
        Ok(())
    }

    fn owner_has_auth(&self) -> bool {
        !self.auth.is_empty()
    }

    fn owner_has_rate_limits(&self) -> bool {
        !self.rates.is_empty()
    }

    fn visit_program(&self, visit: &mut ProgramVisitor<'_>) -> Result<(), ApiError> {
        visit(&[ProgramSegment {
            ops: &self.ops,
            emissions: self,
        }])
    }

    fn generated_endpoint_ids(&self) -> &'static [&'static str] {
        self.ids
    }
}

/// Binds `endpoint` at `owner` through the real runtime bridge.
pub(crate) fn bind<R, O>(
    owner: &Arc<TestOwner>,
    endpoint: RuntimeEndpoint<R, O>,
) -> Result<BoundRuntimeRequest<R, O>, ApiError>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    let projection: Arc<dyn OwnerProjection> = owner.clone();
    super::bind_runtime_at_owner(&projection, endpoint)
}

/// Plans `endpoint` at `owner` without binding it.
pub(crate) fn plan<R, O>(
    owner: &Arc<TestOwner>,
    endpoint: &RuntimeEndpoint<R, O>,
) -> Result<crate::endpoint::RequestPlan, ApiError>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    let projection: Arc<dyn OwnerProjection> = owner.clone();
    super::plan_runtime_at_owner(&projection, endpoint)
}

/// Binds `endpoint` at an empty root owner of `core` (no inherited
/// attachments), panicking on a binding error.
pub(crate) fn root<R, O>(
    core: &ClientCore,
    endpoint: RuntimeEndpoint<R, O>,
) -> BoundRuntimeRequest<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    bind(&TestOwner::new(core.clone()).into_projection(), endpoint)
        .expect("runtime endpoint binds at the root test owner")
}
