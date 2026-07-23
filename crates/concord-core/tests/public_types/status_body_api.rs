use concord_core::advanced::{Bytes, StatusBody, StatusBodyState};

fn public_accessors(body: &StatusBody) {
    let _: StatusBodyState = body.state();
    let _: Option<usize> = body.len();
    let _: Option<&Bytes> = body.bytes();
}

fn owning_accessor(body: StatusBody) {
    let _: Option<Bytes> = body.into_bytes();
}

#[test]
fn downstream_complete_only_accessors_have_final_signatures() {
    let _: fn(&StatusBody) = public_accessors;
    let _: fn(StatusBody) = owning_accessor;
}
