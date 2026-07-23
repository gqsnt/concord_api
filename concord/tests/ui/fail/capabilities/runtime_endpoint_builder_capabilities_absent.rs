use concord::advanced::{Method, RuntimeEndpoint};

fn main() {
    let builder = RuntimeEndpoint::builder("runtime.route".into(), Method::GET);
    let _ = builder.path("/raw/fragment");

    let builder = RuntimeEndpoint::builder("runtime.page".into(), Method::GET);
    let _ = builder.paginate(1_u32);
}
