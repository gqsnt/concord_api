//! Declared rate-limit attachments before built-in dimension resolution.

use std::num::NonZeroU32;
use std::time::Duration;

use super::canonical::{EncodedKeyPart, encode_canonical, encode_canonical_with};
use super::schema::{ENDPOINT_LABEL, ORIGIN_LABEL, RateDimensionKind};
use super::types::{KeyAccumulator, valid_custom_label, validate_window_append};
use super::{
    RateKeyEncoder, RateKeyPart, RateLimitError, RateLimitId, RateLimitPolicy, RateLimitWindow,
};
use crate::error::RateLimitErrorKind;
use crate::limits::{
    MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
};

/// One declared rate-limit attachment: explicit policy ID, cost, windows in
/// source order, and an ordered, explicitly labelled key schema with bound
/// values.
///
/// This is both the generated-code bridge (the `api!` macro lowers every
/// resolved `rate_limit` attachment form of §12.1 to one declaration) and the
/// runtime-endpoint policy API. Custom dimension values are canonically
/// encoded immediately and retained as protected bytes; the built-in
/// [`origin`](Self::origin) and [`endpoint`](Self::endpoint) dimensions are
/// resolved by core from the request head at preparation time, so a
/// declaration can never substitute a sanitized placeholder or a different
/// destination for the actual logical origin.
///
/// Every builder method latches the first structural error; later calls do
/// not encode values. The error is reported (as a safe configuration error
/// without key text) when the declaration is attached to a request head or a
/// runtime endpoint. Construction performs no limiter call, schema
/// registration or I/O. The type has no `Debug` implementation because it
/// retains protected key bytes.
///
/// Rules (RATE-01/03, §12.1, §20.1):
///
/// * ID: nonempty UTF-8, at most 256 bytes.
/// * Cost: positive, set at most once, defaults to one, at most every window
///   capacity.
/// * Windows: 1 to 16, positive capacity and duration, pairwise distinct
///   durations. Source order is provenance; schema comparison uses canonical
///   (duration, capacity) order.
/// * Key: 1 to 32 dimensions with unique labels; custom labels are nonempty,
///   at most 128 bytes and never `origin`/`endpoint`; each built-in appears at
///   most once.
///
/// ```
/// # use std::time::Duration;
/// # use concord_core::rate_limit::RateLimitDeclaration;
/// let tenant = String::from("tenant-a");
/// let declaration = RateLimitDeclaration::new("catalog.tenant.read")
///     .window(120, Duration::from_secs(60))
///     .window(5_000, Duration::from_secs(3_600))
///     .origin()
///     .part("tenant", &tenant);
/// assert!(declaration.validate().is_ok());
/// ```
#[derive(Clone)]
pub struct RateLimitDeclaration {
    state: Result<DeclarationParts, RateLimitErrorKind>,
}

#[derive(Clone)]
struct DeclarationParts {
    id: RateLimitId,
    cost: Option<NonZeroU32>,
    windows: Vec<RateLimitWindow>,
    dimensions: Vec<DeclaredDimension>,
    custom_bytes: usize,
}

#[derive(Clone)]
enum DeclaredDimension {
    Origin,
    Endpoint,
    Custom {
        label: Box<str>,
        part: EncodedKeyPart,
    },
}

impl DeclaredDimension {
    fn label(&self) -> &str {
        match self {
            Self::Origin => ORIGIN_LABEL,
            Self::Endpoint => ENDPOINT_LABEL,
            Self::Custom { label, .. } => label,
        }
    }

    fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Origin, Self::Origin) | (Self::Endpoint, Self::Endpoint) => true,
            (
                Self::Custom { label, part },
                Self::Custom {
                    label: other_label,
                    part: other_part,
                },
            ) => {
                label == other_label
                    && part.encoding_id() == other_part.encoding_id()
                    && part.as_bytes() == other_part.as_bytes()
            }
            _ => false,
        }
    }
}

impl RateLimitDeclaration {
    /// Starts a declaration with an explicit policy ID and default cost one.
    #[must_use]
    pub fn new(id: impl AsRef<str>) -> Self {
        Self::with_id(RateLimitId::new(id))
    }

    /// Generated-code bridge: starts a declaration whose policy ID is a
    /// string literal of the declaration source. Identical to [`Self::new`]
    /// except that the ID is borrowed rather than copied, so preparing a
    /// generated head does not allocate or reference-count the ID.
    #[doc(hidden)]
    #[must_use]
    pub fn new_static(id: &'static str) -> Self {
        Self::with_id(RateLimitId::from_static(id))
    }

    fn with_id(id: Result<RateLimitId, RateLimitError>) -> Self {
        Self {
            state: id
                .map(|id| DeclarationParts {
                    id,
                    cost: None,
                    windows: Vec::new(),
                    dimensions: Vec::new(),
                    custom_bytes: 0,
                })
                .map_err(|error| error.kind()),
        }
    }

    /// Sets the positive per-attempt cost charged against every window.
    ///
    /// Cost is not part of bucket identity. Zero, or setting cost twice, is a
    /// configuration error.
    #[must_use]
    pub fn cost(self, cost: u32) -> Self {
        self.update(|parts| {
            let cost = NonZeroU32::new(cost).ok_or(RateLimitErrorKind::InvalidConfiguration)?;
            if parts.cost.replace(cost).is_some() {
                return Err(RateLimitErrorKind::InvalidConfiguration);
            }
            Ok(())
        })
    }

    /// Appends one conjunctive window of `capacity` per `duration`.
    #[must_use]
    pub fn window(self, capacity: u32, duration: Duration) -> Self {
        self.update(|parts| {
            if parts.windows.len() >= MAX_RATE_LIMIT_WINDOWS_PER_POLICY {
                return Err(RateLimitErrorKind::InvalidConfiguration);
            }
            let capacity =
                NonZeroU32::new(capacity).ok_or(RateLimitErrorKind::InvalidConfiguration)?;
            let window = RateLimitWindow::new(capacity, duration).map_err(|error| error.kind())?;
            // Cost is validated against every capacity once the declaration
            // is complete, because it may be set after the windows.
            validate_window_append(&parts.windows, NonZeroU32::MIN, &window)
                .map_err(|error| error.kind())?;
            parts.windows.push(window);
            Ok(())
        })
    }

    /// Appends the built-in normalized logical-origin dimension.
    ///
    /// Core encodes `scheme://host:effective_port` of the request's actual
    /// logical destination under label `origin` with the `concord.utf8/1`
    /// contract when the declaration is attached to a head.
    #[must_use]
    pub fn origin(self) -> Self {
        self.push_dimension(DeclaredDimension::Origin)
    }

    /// Appends the built-in operational endpoint-ID dimension.
    ///
    /// Core encodes the request's operational endpoint ID under label
    /// `endpoint` with the `concord.utf8/1` contract. The declaration
    /// language requires such an endpoint to pin an explicit ID; that is a
    /// compile-time obligation of the macro.
    #[must_use]
    pub fn endpoint(self) -> Self {
        self.push_dimension(DeclaredDimension::Endpoint)
    }

    /// Appends one explicitly labelled custom dimension encoded through the
    /// value's own [`RateKeyPart`] contract.
    #[must_use]
    pub fn part<T>(self, label: &str, value: &T) -> Self
    where
        T: RateKeyPart + ?Sized,
    {
        self.push_custom(label, |limit| encode_canonical(value, limit))
    }

    /// Appends one explicitly labelled custom dimension encoded through the
    /// explicit `via` adapter `E`.
    #[must_use]
    pub fn part_with<E, T>(self, label: &str, value: &T) -> Self
    where
        E: RateKeyEncoder<T>,
        T: ?Sized,
    {
        self.push_custom(label, |limit| encode_canonical_with::<E, T>(value, limit))
    }

    /// Validates that the declaration is complete and structurally valid.
    ///
    /// # Errors
    ///
    /// Returns the first latched error, or
    /// [`RateLimitErrorKind::InvalidConfiguration`] for missing windows or a
    /// cost above a capacity and [`RateLimitErrorKind::InvalidKey`] for an
    /// empty key.
    pub fn validate(&self) -> Result<(), RateLimitError> {
        self.complete_parts().map(|_| ())
    }

    /// Returns the declared policy ID when construction has not failed.
    #[must_use]
    pub fn id(&self) -> Option<&RateLimitId> {
        self.state.as_ref().ok().map(|parts| &parts.id)
    }

    /// Returns library-retained structural bytes (ID plus custom labels and
    /// canonical values) for runtime storage accounting.
    pub(crate) fn retained_bytes(&self) -> Result<usize, RateLimitError> {
        let parts = self.complete_parts()?;
        parts
            .id
            .as_str()
            .len()
            .checked_add(parts.custom_bytes)
            .ok_or_else(|| RateLimitError::new(RateLimitErrorKind::InvalidKey))
    }

    /// Returns whether both declarations resolve to the exact same policy
    /// under one request head: identical ID, cost, canonical window schema
    /// and ordered dimensions (labels, kinds, encodings and custom bytes).
    ///
    /// Static identity of the *symbolic* bindings is the macro's obligation;
    /// core compares only what it can observe.
    pub(crate) fn same_resolution(&self, other: &Self) -> bool {
        let (Ok(left), Ok(right)) = (&self.state, &other.state) else {
            return false;
        };
        left.id == right.id
            && left.effective_cost() == right.effective_cost()
            && left.canonical_windows() == right.canonical_windows()
            && left.dimensions.len() == right.dimensions.len()
            && left
                .dimensions
                .iter()
                .zip(right.dimensions.iter())
                .all(|(left, right)| left.same_as(right))
    }

    /// Resolves this declaration for an explicit destination exactly as core
    /// does when a request head is prepared.
    ///
    /// `destination` is an absolute `http`/`https` URL whose normalized
    /// `scheme://host:effective_port` becomes the `origin` dimension, and
    /// `endpoint` supplies the `endpoint` dimension. This is intended for
    /// limiter implementations and downstream tests that need realistic
    /// resolved policies; it grants no execution authority, because core
    /// always resolves managed requests from their own head. No I/O occurs.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for a destination
    /// that is not an absolute HTTP(S) URL with a host, and otherwise the
    /// declaration's validation or key-size error.
    pub fn resolve_for(
        &self,
        destination: &str,
        endpoint: &crate::endpoint::EndpointId,
    ) -> Result<RateLimitPolicy, RateLimitError> {
        let invalid = || RateLimitError::new(RateLimitErrorKind::InvalidConfiguration);
        let url = url::Url::parse(destination).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(invalid());
        }
        let origin = super::normalized_origin(&url).ok_or_else(invalid)?;
        self.resolve(&origin, endpoint.as_str())
    }

    /// Resolves built-in dimensions for one request head.
    ///
    /// `origin` must be the normalized logical origin payload and `endpoint`
    /// the operational endpoint ID.
    pub(crate) fn resolve(
        &self,
        origin: &str,
        endpoint: &str,
    ) -> Result<RateLimitPolicy, RateLimitError> {
        let parts = self.complete_parts()?;
        let mut key = KeyAccumulator::default();
        for dimension in &parts.dimensions {
            match dimension {
                DeclaredDimension::Origin => {
                    let remaining =
                        key.remaining_value_bytes(ORIGIN_LABEL, RateDimensionKind::Origin)?;
                    let encoded = encode_canonical(origin, remaining)?;
                    key.push(ORIGIN_LABEL, RateDimensionKind::Origin, &encoded)?;
                }
                DeclaredDimension::Endpoint => {
                    let remaining =
                        key.remaining_value_bytes(ENDPOINT_LABEL, RateDimensionKind::Endpoint)?;
                    let encoded = encode_canonical(endpoint, remaining)?;
                    key.push(ENDPOINT_LABEL, RateDimensionKind::Endpoint, &encoded)?;
                }
                DeclaredDimension::Custom { label, part } => {
                    key.push(label, RateDimensionKind::Custom, part)?;
                }
            }
        }
        RateLimitPolicy::new(
            parts.id.clone(),
            key.finish()?,
            parts.effective_cost(),
            parts.windows.iter().copied(),
        )
    }

    fn complete_parts(&self) -> Result<&DeclarationParts, RateLimitError> {
        let parts = self
            .state
            .as_ref()
            .map_err(|kind| RateLimitError::new(*kind))?;
        if parts.dimensions.is_empty() {
            return Err(RateLimitError::new(RateLimitErrorKind::InvalidKey));
        }
        let cost = parts.effective_cost();
        if parts.windows.is_empty()
            || parts
                .windows
                .iter()
                .any(|window| cost.get() > window.capacity().get())
        {
            return Err(RateLimitError::new(
                RateLimitErrorKind::InvalidConfiguration,
            ));
        }
        Ok(parts)
    }

    fn push_dimension(self, dimension: DeclaredDimension) -> Self {
        self.update(|parts| {
            parts.check_label(dimension.label())?;
            parts.dimensions.push(dimension);
            Ok(())
        })
    }

    fn push_custom<F>(self, label: &str, encode: F) -> Self
    where
        F: FnOnce(usize) -> Result<EncodedKeyPart, RateLimitError>,
    {
        self.update(|parts| {
            if !valid_custom_label(label) {
                return Err(RateLimitErrorKind::InvalidKey);
            }
            parts.check_label(label)?;
            let before_value = parts
                .custom_bytes
                .checked_add(4 + 8)
                .and_then(|bytes| bytes.checked_add(label.len()))
                .ok_or(RateLimitErrorKind::InvalidKey)?;
            let remaining = MAX_RATE_LIMIT_KEY_BYTES
                .checked_sub(before_value)
                .ok_or(RateLimitErrorKind::InvalidKey)?;
            let part = encode(remaining).map_err(|error| error.kind())?;
            parts.custom_bytes = before_value
                .checked_add(part.len())
                .ok_or(RateLimitErrorKind::InvalidKey)?;
            parts.dimensions.push(DeclaredDimension::Custom {
                label: label.into(),
                part,
            });
            Ok(())
        })
    }

    fn update<F>(mut self, update: F) -> Self
    where
        F: FnOnce(&mut DeclarationParts) -> Result<(), RateLimitErrorKind>,
    {
        if let Ok(parts) = &mut self.state {
            if let Err(kind) = update(parts) {
                self.state = Err(kind);
            }
        }
        self
    }
}

impl DeclarationParts {
    fn effective_cost(&self) -> NonZeroU32 {
        self.cost.unwrap_or(NonZeroU32::MIN)
    }

    fn canonical_windows(&self) -> Vec<(Duration, NonZeroU32)> {
        let mut windows = self
            .windows
            .iter()
            .map(|window| (window.duration(), window.capacity()))
            .collect::<Vec<_>>();
        windows.sort_unstable();
        windows
    }

    fn check_label(&self, label: &str) -> Result<(), RateLimitErrorKind> {
        if self.dimensions.len() >= MAX_RATE_LIMIT_KEY_PARTS
            || self
                .dimensions
                .iter()
                .any(|dimension| dimension.label() == label)
        {
            Err(RateLimitErrorKind::InvalidKey)
        } else {
            Ok(())
        }
    }
}

/// Appends one declaration to a pending effective list under the plan
/// identity rules: an exact repetition is deduplicated, any other repetition
/// of an ID conflicts, and at most
/// [`crate::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN`] distinct
/// declarations are retained.
pub(crate) fn push_declaration(
    declarations: &mut Vec<RateLimitDeclaration>,
    declaration: RateLimitDeclaration,
) -> Result<(), RateLimitError> {
    declaration.validate()?;
    let id = declaration
        .id()
        .ok_or_else(|| RateLimitError::new(RateLimitErrorKind::InvalidConfiguration))?;
    if let Some(earlier) = declarations
        .iter()
        .find(|earlier| earlier.id().is_some_and(|earlier| earlier == id))
    {
        return if earlier.same_resolution(&declaration) {
            Ok(())
        } else {
            Err(RateLimitError::new(
                RateLimitErrorKind::InvalidConfiguration,
            ))
        };
    }
    if declarations.len() >= crate::limits::MAX_RATE_LIMIT_POLICIES_PER_PLAN {
        return Err(RateLimitError::new(
            RateLimitErrorKind::InvalidConfiguration,
        ));
    }
    declarations.push(declaration);
    Ok(())
}
