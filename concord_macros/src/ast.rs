use std::time::Duration;

use proc_macro2::Span;
use syn::{Ident, LitBool, LitInt, LitStr, Path, Type};

#[derive(Clone)]
pub(crate) struct SourceApi {
    pub(crate) clients: Vec<SourceClient>,
}

#[derive(Clone)]
pub(crate) struct SourceClient {
    pub(crate) name: Ident,
    pub(crate) base: LitStr,
    pub(crate) values: Vec<ValueDecl>,
    pub(crate) retry: RetrySelection,
    pub(crate) layer: Layer,
    pub(crate) children: Vec<SourceChild>,
}

#[derive(Clone)]
pub(crate) enum SourceChild {
    Scope(Box<SourceScope>),
    Endpoint(SourceEndpoint),
}

#[derive(Clone)]
pub(crate) struct SourceScope {
    pub(crate) name: Ident,
    pub(crate) parameters: Vec<ValueDecl>,
    pub(crate) layer: Layer,
    pub(crate) children: Vec<SourceChild>,
}

#[derive(Clone)]
pub(crate) struct SourceEndpoint {
    pub(crate) method: MethodKind,
    pub(crate) name: Ident,
    pub(crate) alias: Option<Ident>,
    pub(crate) parameters: Vec<ValueDecl>,
    pub(crate) body: Option<RequestBodyKind>,
    pub(crate) layer: Layer,
    pub(crate) response: ResponseKind,
}

#[derive(Clone)]
pub(crate) struct ValueDecl {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
    pub(crate) kind: ValueKind,
}

#[derive(Clone)]
pub(crate) enum ValueKind {
    Required,
    Optional,
    Defaulted(DefaultExpr),
}

#[derive(Clone)]
pub(crate) enum DefaultExpr {
    String(LitStr),
    Integer { negative: bool, literal: LitInt },
    Boolean(LitBool),
    Path(Path),
}

#[derive(Clone)]
pub(crate) enum RetrySelection {
    Disabled,
    ReqwestDefault,
    Custom(Path),
}

#[derive(Clone, Default)]
pub(crate) struct Layer {
    pub(crate) host: Option<HostOperation>,
    pub(crate) path: Option<PathOperation>,
    pub(crate) query: Option<QueryOperation>,
    pub(crate) headers: Option<HeaderOperation>,
    pub(crate) timeout: Option<TimeoutOperation>,
}

#[derive(Clone)]
pub(crate) enum HostOperation {
    Replace(Vec<PublicExpr>),
    Off,
}

#[derive(Clone)]
pub(crate) enum PathOperation {
    Append(Vec<PublicExpr>),
    Replace(Vec<PublicExpr>),
    Off,
}

#[derive(Clone)]
pub(crate) enum QueryOperation {
    Modify(Vec<QueryEntry>),
    Replace(Vec<QueryEntry>),
    Off,
}

#[derive(Clone)]
pub(crate) enum QueryEntry {
    Assign { key: LitStr, value: PublicExpr },
    Remove(LitStr),
}

#[derive(Clone)]
pub(crate) enum HeaderOperation {
    Modify(Vec<HeaderEntry>),
    Replace(Vec<HeaderEntry>),
    Off,
}

#[derive(Clone)]
pub(crate) enum HeaderEntry {
    Assign { name: LitStr, value: PublicExpr },
    Remove(LitStr),
}

#[derive(Clone)]
pub(crate) enum TimeoutOperation {
    Set(DurationLiteral),
    Off,
}

#[derive(Clone)]
pub(crate) struct DurationLiteral {
    pub(crate) value: Duration,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) enum PublicExpr {
    String(LitStr),
    Integer(LitInt),
    Reference { namespace: Ident, name: Ident },
    Format { span: Span, values: Vec<PublicExpr> },
}

#[derive(Clone)]
pub(crate) enum MethodKind {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Trace,
    Connect,
    Custom(LitStr),
}

#[derive(Clone)]
pub(crate) enum ResponseKind {
    Bytes,
    NoContent,
    Buffered(CodecKind),
    Stream(MediaKind),
}

#[derive(Clone)]
pub(crate) enum RequestBodyKind {
    Buffered(CodecKind),
    Stream(MediaKind),
    Multipart,
}

#[derive(Clone)]
pub(crate) enum MediaKind {
    OctetStream,
    Custom(Type),
}

#[derive(Clone)]
pub(crate) enum CodecKind {
    Text,
    Json(Type),
    Custom(Type),
}
