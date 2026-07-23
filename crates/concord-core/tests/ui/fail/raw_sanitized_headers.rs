use concord_core::advanced::{
    HeaderMap, HeaderName, HeaderValue, SanitizedHeaderMap, SanitizedHeaders,
};

fn as_ref_escape(headers: &SanitizedHeaderMap) {
    let _: &HeaderMap = headers.as_ref();
}

fn deref_escape(headers: &SanitizedHeaderMap) {
    let _: &HeaderMap = &*headers;
}

fn conversion_escape(headers: SanitizedHeaderMap) {
    let _: HeaderMap = headers.into();
}

fn raw_accessor_escape(headers: &SanitizedHeaderMap) {
    let _: &HeaderMap = headers.raw();
}

fn raw_iteration_escape(headers: &SanitizedHeaderMap) {
    let _: Option<(&HeaderName, &HeaderValue)> = headers.iter().next();
}

fn borrowed_as_ref_escape(headers: SanitizedHeaders<'_>) {
    let _: &HeaderMap = headers.as_ref();
}

fn borrowed_deref_escape(headers: SanitizedHeaders<'_>) {
    let _: &HeaderMap = &*headers;
}

fn borrowed_conversion_escape(headers: SanitizedHeaders<'_>) {
    let _: HeaderMap = headers.into();
}

fn borrowed_raw_accessor_escape(headers: SanitizedHeaders<'_>) {
    let _: &HeaderMap = headers.raw();
}

fn borrowed_raw_iteration_escape(headers: SanitizedHeaders<'_>) {
    let _: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
}

fn main() {}
