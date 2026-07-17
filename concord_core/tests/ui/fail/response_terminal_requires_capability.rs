use concord_core::Request;
use concord_core::__private::EndpointSpec;

fn response_requires_metadata_capability<E>(request: Request<E>)
where
    E: EndpointSpec,
{
    let _ = request.response();
}

fn main() {}
