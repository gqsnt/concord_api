use concord_core::contracts::{RateLimitRequest, RateLimitResponse};

fn inspect_request(request: RateLimitRequest<'_>) {
    let _ = request.observed_url;
    let _ = request.host;
    let _ = request.path;
    let _ = request.query;
    let _ = request.body;
    let _ = request.credentials;
    let _ = request.headers;
    let _ = request.native_url;
    let _ = request.native_request;
}

fn inspect_response(response: RateLimitResponse<'_>) {
    let _ = response.body;
    let _ = response.credentials;
    let _ = response.headers;
    let _ = response.native_response;
}

fn main() {}
