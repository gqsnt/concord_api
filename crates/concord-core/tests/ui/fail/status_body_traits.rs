use concord_core::contracts::StatusBody;

fn require_clone<T: Clone>() {}
fn require_serialize<T: serde::Serialize>() {}

fn as_ref_escape(body: &StatusBody) {
    let _: &[u8] = body.as_ref();
}

fn deref_escape(body: &StatusBody) {
    let _: &[u8] = &**body;
}

fn main() {
    require_clone::<StatusBody>();
    require_serialize::<StatusBody>();
}
