//! The compile-time capabilities of a generated call. Only generated code should name these items.

mod sealed {
    pub trait Sealed {}
}

/// The mode for an endpoint without a pager. Only generated code should name this type.
#[derive(Debug)]
pub struct NoPaginate;

/// The mode for an endpoint with a `paginate` expression. Only generated code should name this type.
#[derive(Debug)]
pub struct Paged;

/// Enables conditional sending for GET and HEAD. Only generated code should name this type.
#[derive(Debug)]
pub struct GetOrHead;

/// The mode for methods other than GET, HEAD, and POST. Only generated code should name this type.
#[derive(Debug)]
pub struct NotGetOrHead;

/// Disables conditional sending for POST. Only generated code should name this type.
#[derive(Debug)]
pub struct Post;

impl sealed::Sealed for NoPaginate {}
impl sealed::Sealed for Paged {}
impl sealed::Sealed for GetOrHead {}
impl sealed::Sealed for NotGetOrHead {}
impl sealed::Sealed for Post {}

/// Identifies whether an endpoint has a pager. Only generated code should name this trait.
pub trait PagingMode: sealed::Sealed {
    /// Evidence required to start a pager. Only generated code should name this type.
    #[doc(hidden)]
    type Proof;
}
impl PagingMode for NoPaginate {
    type Proof = std::convert::Infallible;
}
impl PagingMode for Paged {
    type Proof = ();
}

/// Identifies whether an endpoint supports conditional sending. Only generated code should name this trait.
pub trait ConditionalMode: sealed::Sealed {}
impl ConditionalMode for GetOrHead {}
impl ConditionalMode for NotGetOrHead {}
impl ConditionalMode for Post {}

/// Enables direct awaiting for calls without `paginate`. Only generated code should name this trait.
#[diagnostic::on_unimplemented(
    message = "a paged call cannot be awaited directly",
    label = "this endpoint declares `paginate`",
    note = "use `.send().await` for one page, or `.pages()` for all pages"
)]
pub trait AwaitWithoutPaginate: PagingMode {}
impl AwaitWithoutPaginate for NoPaginate {}

/// Enables `.pages()` for calls with `paginate`. Only generated code should name this trait.
#[diagnostic::on_unimplemented(
    message = "this call has no pager",
    label = "this endpoint does not declare `paginate`",
    note = "use `.await` or `.send().await` to send this call"
)]
pub trait DeclaresPaginate: PagingMode<Proof = ()> {}
impl DeclaresPaginate for Paged {}

/// Enables `.send_if_none_match()` for GET and HEAD calls. Only generated code should name this trait.
#[diagnostic::on_unimplemented(
    message = "this call cannot be sent with `If-None-Match`",
    label = "only a GET or HEAD call can be sent conditionally",
    note = "use `.send().await` to send this call"
)]
pub trait IsGetOrHead: ConditionalMode {}
impl IsGetOrHead for GetOrHead {}
