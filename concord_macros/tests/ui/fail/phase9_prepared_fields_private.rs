use std::marker::PhantomData;

use concord_core::advanced::{
    Method, PreparedBuffered, PreparedEndpoint, PreparedEndpointBuilder, PreparedNoContent, Text,
};

fn main() {
    let _ = PreparedBuffered::<Text>(PhantomData);
    let _ = PreparedEndpointBuilder::<PreparedNoContent> {};
    let _ = PreparedEndpoint::<PreparedNoContent> {};
    let _ = PreparedEndpoint::builder("Prepared::private", Method::GET);
}
