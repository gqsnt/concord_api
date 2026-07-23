use concord_core::advanced::PaginationRequest;
use concord_core::PaginatedRequest;

fn wrapper_storage(request: PaginatedRequest<()>) {
    let _ = &request.request;
    let _ = &request.limit;
    let _ = format!("{request:?}");
}

fn main() {}
