use concord_core::contracts::{
    Bytes, HeaderMap, HeaderName, HeaderValue, Method, SanitizedHeaderMap, SanitizedHeaderValue,
    SanitizedHeaders, StatusCode, Url, Version,
};

fn borrowed_signatures<'a>(headers: SanitizedHeaders<'a>, name: &HeaderName) {
    let _: usize = headers.len();
    let _: bool = headers.is_empty();
    let _: bool = headers.contains_key(name);
    let _: Option<SanitizedHeaderValue<'a>> = headers.get(name);
    let _: Vec<(&'a HeaderName, SanitizedHeaderValue<'a>)> = headers.iter().collect();
}

fn owned_signatures(headers: &SanitizedHeaderMap, name: &HeaderName) {
    let _: usize = headers.len();
    let _: bool = headers.is_empty();
    let _: bool = headers.contains_key(name);
    let _: Option<SanitizedHeaderValue<'_>> = headers.get(name);
    let _: Vec<(&HeaderName, SanitizedHeaderValue<'_>)> = headers.iter().collect();
}

fn contract_wire_exports() {
    let _: Option<Bytes> = None;
    let _: Option<HeaderMap> = None;
    let _: Option<HeaderName> = None;
    let _: Option<HeaderValue> = None;
    let _: Option<Method> = None;
    let _: Option<StatusCode> = None;
    let _: Option<Version> = None;
    let _: Option<Url> = None;
}

fn requires_value_traits<T: Clone + Copy + std::fmt::Debug + Eq + PartialEq>() {}
fn requires_view_traits<T: Clone + Copy + std::fmt::Debug>() {}
fn requires_debug<T: std::fmt::Debug>() {}

#[test]
fn downstream_public_surface_has_required_exports_and_signatures() {
    let _: for<'a> fn(SanitizedHeaders<'a>, &HeaderName) = borrowed_signatures;
    let _: fn(&SanitizedHeaderMap, &HeaderName) = owned_signatures;
    contract_wire_exports();

    requires_value_traits::<SanitizedHeaderValue<'static>>();
    requires_view_traits::<SanitizedHeaders<'static>>();
    requires_debug::<SanitizedHeaderMap>();
}
