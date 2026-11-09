// `#[public]` names a shown field of a `#[sensitive]` type or variant.

#[derive(concord::Debug)]
struct Outside {
    #[public]
    sid: String,
}

#[derive(concord::Debug)]
#[sensitive]
struct Unnamed(#[public] String, String);

#[derive(concord::Debug)]
#[public]
struct OnType {
    sid: String,
}

#[derive(concord::Debug)]
enum OnVariant {
    #[public]
    Sent { sid: String },
}

fn main() {}
