use concord_core::advanced::PaginationRequest;
use concord_core::PaginatedRequest;

fn request_capabilities(request: &mut PaginationRequest<'_>) {
    request.set_body("forbidden");
    request.set_host("forbidden");
    request.set_path("forbidden");
    request.set_credential("forbidden");
    request.native_url();
    request.native_request();
    let _ = &request.patch;
}

fn wrapper_storage(request: PaginatedRequest<()>) {
    let _ = &request.request;
    let _ = &request.limit;
    let _ = format!("{request:?}");
}

fn main() {}
