use crate::client::{ApiClient, ClientContext};
use crate::codec::{Encodes, NoContentEncoding};
use crate::debug::DebugLevel;
use crate::error::ApiClientError;
use crate::pagination::PaginationPart;
use crate::policy::Policy;
use crate::types::RouteParts;
use http::Method;
use std::marker::PhantomData;

/// RoutePart modifie `RouteParts` (host + path).
pub trait RoutePart<Cx: ClientContext, E>: Send + Sync + 'static {
    fn apply(ep: &E, client: &ApiClient<Cx>, route: &mut RouteParts) -> Result<(), ApiClientError>;
}

/// PolicyPart modifie `Policy` (headers + query).
pub trait PolicyPart<Cx: ClientContext, E>: Send + Sync + 'static {
    fn apply(ep: &E, client: &ApiClient<Cx>, policy: &mut Policy) -> Result<(), ApiClientError>;
}

pub struct NoRoute;
impl<Cx: ClientContext, E> RoutePart<Cx, E> for NoRoute {
    fn apply(_: &E, _: &ApiClient<Cx>, _: &mut RouteParts) -> Result<(), ApiClientError> {
        Ok(())
    }
}

pub struct NoPolicy;
impl<Cx: ClientContext, E> PolicyPart<Cx, E> for NoPolicy {
    fn apply(_: &E, _: &ApiClient<Cx>, _: &mut Policy) -> Result<(), ApiClientError> {
        Ok(())
    }
}

/// Composition A puis B.
pub struct Chain<A, B>(PhantomData<(A, B)>);

impl<A, B> Default for Chain<A, B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, B> Chain<A, B> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<Cx: ClientContext, E, A, B> RoutePart<Cx, E> for Chain<A, B>
where
    A: RoutePart<Cx, E>,
    B: RoutePart<Cx, E>,
{
    fn apply(ep: &E, client: &ApiClient<Cx>, route: &mut RouteParts) -> Result<(), ApiClientError> {
        A::apply(ep, client, route)?;
        B::apply(ep, client, route)?;
        Ok(())
    }
}

impl<Cx: ClientContext, E, A, B> PolicyPart<Cx, E> for Chain<A, B>
where
    A: PolicyPart<Cx, E>,
    B: PolicyPart<Cx, E>,
{
    fn apply(ep: &E, client: &ApiClient<Cx>, policy: &mut Policy) -> Result<(), ApiClientError> {
        A::apply(ep, client, policy)?;
        B::apply(ep, client, policy)?;
        Ok(())
    }
}

/// Définit comment récupérer un body (optionnel) + encoder.
pub trait BodyPart<E>: Send + Sync + 'static {
    type Body: 'static;
    type Enc: Encodes<Self::Body>;
    fn body(ep: &E) -> Option<&Self::Body>;
}

pub struct NoBody;

impl<E> BodyPart<E> for NoBody {
    type Body = ();
    type Enc = NoContentEncoding;
    fn body(_: &E) -> Option<&Self::Body> {
        None
    }
}

pub trait ResponseSpec: Send + Sync + 'static {
    type Decoded: Send + 'static; // interne
    type Output: Send + 'static; // public
    type Dec: crate::codec::Decodes<Self::Decoded>;

    fn accept_content_type() -> &'static str {
        <Self::Dec as crate::codec::ContentType>::CONTENT_TYPE
    }
    fn is_no_content() -> bool {
        <Self::Dec as crate::codec::ContentType>::IS_NO_CONTENT
    }

    fn map(decoded: Self::Decoded) -> Result<Self::Output, crate::error::FxError>;
}

/// Helper générique : (decoder, type)
pub struct Decoded<Dec, T>(PhantomData<(Dec, T)>);

impl<Dec, T> ResponseSpec for Decoded<Dec, T>
where
    Dec: crate::codec::Decodes<T> + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    type Decoded = T;
    type Output = T;
    type Dec = Dec;

    fn map(decoded: T) -> Result<T, crate::error::FxError> {
        Ok(decoded)
    }
}

pub trait Transform<T>: Send + Sync + 'static {
    type Out: Send + 'static;
    fn map(v: T) -> Result<Self::Out, crate::error::FxError>;
}

pub struct Mapped<R, M>(PhantomData<(R, M)>);

impl<R, M> ResponseSpec for Mapped<R, M>
where
    R: ResponseSpec,
    M: Transform<R::Decoded>,
{
    type Decoded = R::Decoded;
    type Output = M::Out;
    type Dec = R::Dec;

    fn map(decoded: Self::Decoded) -> Result<Self::Output, crate::error::FxError> {
        M::map(decoded)
    }
}

/// Endpoint : uniquement des types (composables).
pub trait Endpoint<Cx: ClientContext>: Send + Sync + Sized + 'static {
    const METHOD: Method;

    type Route: RoutePart<Cx, Self>;
    type Policy: PolicyPart<Cx, Self>;
    type Pagination: PaginationPart<Cx, Self>;
    type Body: BodyPart<Self>;
    type Response: ResponseSpec;

    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn accept_content_type() -> &'static str {
        <Self::Response as ResponseSpec>::accept_content_type()
    }

    fn response_is_no_content() -> bool {
        <Self::Response as ResponseSpec>::is_no_content()
    }

    fn debug_level(&self) -> Option<DebugLevel> {
        None
    }
}
