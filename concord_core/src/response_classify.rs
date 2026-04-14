use http::StatusCode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseClass {
    Success,
    HttpStatusError,
}

#[inline]
pub fn classify_status(status: StatusCode) -> ResponseClass {
    if status.is_success() {
        ResponseClass::Success
    } else {
        ResponseClass::HttpStatusError
    }
}
