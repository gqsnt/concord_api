use std::borrow::Cow;
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RateLimitPlan {
    buckets: Vec<RateLimitBucketUse>,
}

impl RateLimitPlan {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn from_buckets(buckets: Vec<RateLimitBucketUse>) -> Self {
        Self { buckets }
    }

    #[inline]
    pub fn buckets(&self) -> &[RateLimitBucketUse] {
        &self.buckets
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    #[inline]
    pub fn push_bucket(&mut self, bucket: RateLimitBucketUse) {
        self.buckets.push(bucket);
    }

    #[inline]
    pub fn extend(&mut self, other: RateLimitPlan) {
        self.buckets.extend(other.buckets);
    }

    #[inline]
    pub fn canonicalize(&mut self) {
        let mut seen: HashSet<RateLimitBucketUse> = HashSet::with_capacity(self.buckets.len());
        self.buckets.retain(|bucket| seen.insert(bucket.clone()));
    }

    #[inline]
    pub fn canonicalized(mut self) -> Self {
        self.canonicalize();
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RateLimitSetting {
    #[default]
    Inherit,
    Add(RateLimitPlan),
    Replace(RateLimitPlan),
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitBucketUse {
    pub id: RateLimitBucketId,
    pub key: RateLimitKey,
    pub windows: Vec<RateLimitWindow>,
    pub cost: NonZeroU32,
}

impl RateLimitBucketUse {
    pub fn new(
        kind: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
        key: RateLimitKey,
    ) -> Self {
        Self {
            id: RateLimitBucketId::new(kind, name),
            key,
            windows: Vec::new(),
            cost: NonZeroU32::new(1).expect("1 is non-zero"),
        }
    }

    #[inline]
    pub fn with_window(mut self, window: RateLimitWindow) -> Self {
        self.windows.push(window);
        self
    }

    #[inline]
    pub fn with_windows(mut self, windows: Vec<RateLimitWindow>) -> Self {
        self.windows = windows;
        self
    }

    #[inline]
    pub fn with_cost(mut self, cost: NonZeroU32) -> Self {
        self.cost = cost;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitBucketId {
    pub kind: Cow<'static, str>,
    pub name: Cow<'static, str>,
}

impl RateLimitBucketId {
    #[inline]
    pub fn new(kind: impl Into<Cow<'static, str>>, name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RateLimitKey {
    parts: Vec<RateLimitKeyPart>,
}

impl RateLimitKey {
    #[inline]
    pub fn new(parts: Vec<RateLimitKeyPart>) -> Self {
        Self { parts }
    }

    #[inline]
    pub fn empty() -> Self {
        Self::default()
    }

    #[inline]
    pub fn parts(&self) -> &[RateLimitKeyPart] {
        &self.parts
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitKeyPart {
    pub name: Cow<'static, str>,
    pub value: RateLimitKeyValue,
}

impl RateLimitKeyPart {
    #[inline]
    pub fn new(name: impl Into<Cow<'static, str>>, value: RateLimitKeyValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    #[inline]
    pub fn static_value(
        name: impl Into<Cow<'static, str>>,
        value: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::new(name, RateLimitKeyValue::Static(value.into()))
    }

    #[inline]
    pub fn endpoint() -> Self {
        Self::new("endpoint", RateLimitKeyValue::Endpoint)
    }

    #[inline]
    pub fn method() -> Self {
        Self::new("method", RateLimitKeyValue::Method)
    }

    #[inline]
    pub fn url_host() -> Self {
        Self::new("route.host", RateLimitKeyValue::UrlHost)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RateLimitKeyValue {
    Static(Cow<'static, str>),
    Endpoint,
    Method,
    UrlHost,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitWindow {
    pub max: NonZeroU32,
    pub per: Duration,
}

impl RateLimitWindow {
    #[inline]
    pub fn new(max: NonZeroU32, per: Duration) -> Self {
        Self { max, per }
    }

    #[inline]
    pub fn from_u32(max: u32, per: Duration) -> Option<Self> {
        Some(Self {
            max: NonZeroU32::new(max)?,
            per: (!per.is_zero()).then_some(per)?,
        })
    }
}
