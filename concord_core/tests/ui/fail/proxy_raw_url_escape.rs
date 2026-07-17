use concord_core::advanced::{SafeProxy, Url};

fn main() {
    let proxy = SafeProxy::parse("http://proxy.example").expect("valid proxy");
    let _: &str = proxy.as_str();
    let _: Url = proxy.into();
}
