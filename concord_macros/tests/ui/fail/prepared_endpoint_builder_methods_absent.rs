use concord_core::advanced::{Method, PreparedEndpoint};

fn main() {
    let builder = PreparedEndpoint::builder("Prepared::route", Method::GET);
    let _ = builder.path("/raw/fragment");

    let builder = PreparedEndpoint::builder("Prepared::page", Method::GET);
    let _ = builder.paginate(1_u32);
}
