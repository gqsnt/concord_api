use concord_core::advanced::{
    Method, NoContent, PreparedEndpoint, PreparedEndpointBuilder,
};

fn main() {
    let _ = PreparedEndpointBuilder::<NoContent> {};
    let _ = PreparedEndpoint::<NoContent> {};
    let _ = PreparedEndpoint::builder("Prepared::private", Method::GET);
}
